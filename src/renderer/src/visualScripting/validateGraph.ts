import type { VisualGraphDocument, VisualGraphNode } from '@shared-types'

import {
  EXEC_IN,
  EXEC_OUT,
  LOOP_BODY,
  THEN_0,
  THEN_1,
  THEN_FALSE,
  THEN_TRUE,
  VISUAL_NODE_TYPES,
  type VisualGraphContext,
  type VisualNodeType,
} from './nodeDefinitions'

export interface GraphValidationIssue {
  message: string
}

const SCENE_EVENT_TYPES: VisualNodeType[] = [
  VISUAL_NODE_TYPES.SCENE_BEGIN,
  VISUAL_NODE_TYPES.TICK,
]

const ENTITY_EVENT_TYPES: VisualNodeType[] = [
  VISUAL_NODE_TYPES.ENTITY_START,
  VISUAL_NODE_TYPES.ENTITY_UPDATE,
]

function graphContext(doc: VisualGraphDocument): VisualGraphContext {
  return doc.context ?? 'scene'
}

function nodesById(nodes: VisualGraphNode[]): Map<string, VisualGraphNode> {
  return new Map(nodes.map((n) => [n.id, n]))
}

function execEdgesFrom(
  doc: VisualGraphDocument,
  nodeId: string,
  sourceHandle: string,
): string | null {
  const edge = doc.edges.find(
    (e) => e.source === nodeId && e.sourceHandle === sourceHandle,
  )
  return edge?.target ?? null
}

function visitExecSubtree(
  doc: VisualGraphDocument,
  nodeId: string | null,
  visited: Set<string>,
  issues: GraphValidationIssue[],
): void {
  if (!nodeId) return
  if (visited.has(nodeId)) {
    issues.push({ message: 'Cycle detected in execution flow' })
    return
  }
  visited.add(nodeId)
  const byId = nodesById(doc.nodes)
  const node = byId.get(nodeId)
  if (!node) return

  if (node.type === VISUAL_NODE_TYPES.SEQUENCE) {
    visitExecSubtree(doc, execEdgesFrom(doc, nodeId, THEN_0), visited, issues)
    visitExecSubtree(doc, execEdgesFrom(doc, nodeId, THEN_1), visited, issues)
  } else if (node.type === VISUAL_NODE_TYPES.IF) {
    visitExecSubtree(doc, execEdgesFrom(doc, nodeId, THEN_TRUE), visited, issues)
    visitExecSubtree(doc, execEdgesFrom(doc, nodeId, THEN_FALSE), visited, issues)
  } else if (
    node.type === VISUAL_NODE_TYPES.FOR_REPEAT
    || node.type === VISUAL_NODE_TYPES.FOR_EACH_ENTITY
  ) {
    visitExecSubtree(doc, execEdgesFrom(doc, nodeId, LOOP_BODY), visited, issues)
  }
  visitExecSubtree(doc, execEdgesFrom(doc, nodeId, EXEC_OUT), visited, issues)
}

/** Valida el grafo antes de compilar. Los mensajes son claves i18n (inglés). */
export function validateGraph(doc: VisualGraphDocument): GraphValidationIssue[] {
  const issues: GraphValidationIssue[] = []
  const byId = nodesById(doc.nodes)
  const context = graphContext(doc)

  if (context === 'scene' && (doc.sceneId ?? 0) <= 0) {
    issues.push({ message: 'Invalid scene id' })
  }
  if (context === 'entity' && (doc.entityId ?? 0) <= 0) {
    issues.push({ message: 'Invalid entity id' })
  }

  const eventTypes = context === 'entity' ? ENTITY_EVENT_TYPES : SCENE_EVENT_TYPES

  for (const eventType of eventTypes) {
    const matches = doc.nodes.filter((n) => n.type === eventType)
    if (matches.length > 1) {
      const label = eventType === VISUAL_NODE_TYPES.SCENE_BEGIN
        ? 'Scene start'
        : eventType === VISUAL_NODE_TYPES.TICK
          ? 'Each frame'
          : eventType === VISUAL_NODE_TYPES.ENTITY_START
            ? 'On start'
            : 'Each frame'
      issues.push({ message: `Only one ${label} node allowed` })
    }
  }

  for (const edge of doc.edges) {
    if (!byId.has(edge.source) || !byId.has(edge.target)) {
      issues.push({ message: `Orphan edge: ${edge.id}` })
    }
  }

  const visited = new Set<string>()
  const roots = doc.nodes.filter((n) => eventTypes.includes(n.type as VisualNodeType))
  for (const ev of roots) {
    visitExecSubtree(doc, execEdgesFrom(doc, ev.id, EXEC_OUT), visited, issues)
  }

  for (const node of doc.nodes) {
    if (node.type === VISUAL_NODE_TYPES.LOG) {
      const msg = node.data?.message
      if (typeof msg !== 'string' || !msg.trim()) {
        issues.push({ message: `Print node ${node.id}: empty message` })
      }
    }
    if (node.type === VISUAL_NODE_TYPES.IF) {
      const expr = node.data?.expression
      if (typeof expr !== 'string' || !expr.trim()) {
        issues.push({ message: `Branch node ${node.id}: empty condition` })
      }
    }
    if (node.type === VISUAL_NODE_TYPES.FOR_REPEAT) {
      const count = Number(node.data?.count)
      if (!Number.isFinite(count) || count < 0) {
        issues.push({ message: `Repeat node ${node.id}: invalid count` })
      }
    }
    if (node.type === VISUAL_NODE_TYPES.FOR_EACH_ENTITY && context !== 'scene') {
      issues.push({ message: 'For each entity is only available in scene logic' })
    }
    const entityOnlyActions = [
      VISUAL_NODE_TYPES.SET_SCALE,
      VISUAL_NODE_TYPES.MOVE_TO,
      VISUAL_NODE_TYPES.TRANSLATE,
    ] as const
    if (context !== 'entity' && entityOnlyActions.includes(node.type as typeof entityOnlyActions[number])) {
      issues.push({ message: `${node.type} is only available in entity logic` })
    }
    if (node.type === VISUAL_NODE_TYPES.SET_SCALE) {
      if (!String(node.data?.scaleX ?? '').trim() || !String(node.data?.scaleY ?? '').trim()) {
        issues.push({ message: `Set scale node ${node.id}: empty value` })
      }
    }
    if (node.type === VISUAL_NODE_TYPES.MOVE_TO) {
      if (!String(node.data?.x ?? '').trim() || !String(node.data?.y ?? '').trim()) {
        issues.push({ message: `Teleport to node ${node.id}: empty value` })
      }
    }
    if (node.type === VISUAL_NODE_TYPES.TRANSLATE) {
      if (!String(node.data?.dx ?? '').trim() || !String(node.data?.dy ?? '').trim()) {
        issues.push({ message: `Translate node ${node.id}: empty value` })
      }
    }
    if (node.type === VISUAL_NODE_TYPES.PLAY_ANIMATION) {
      const targetId = Number(node.data?.entityId)
      const ownerId = doc.entityId ?? 0
      const resolvedId = context === 'entity' && (!Number.isFinite(targetId) || targetId <= 0)
        ? ownerId
        : targetId
      if (!Number.isFinite(resolvedId) || resolvedId <= 0) {
        issues.push({ message: `Play animation node ${node.id}: select entity` })
      }
      const animName = node.data?.animationName
      if (typeof animName !== 'string' || !animName.trim()) {
        issues.push({ message: `Play animation node ${node.id}: select animation` })
      }
    }
    if (eventTypes.includes(node.type as VisualNodeType)) continue
    const hasExecIn = doc.edges.some(
      (e) => e.target === node.id && e.targetHandle === EXEC_IN,
    )
    const isFlowRoot = node.type === VISUAL_NODE_TYPES.SEQUENCE
    if (!hasExecIn && !isFlowRoot) {
      issues.push({ message: `Node ${node.id} has no execution input` })
    }
  }

  return issues
}
