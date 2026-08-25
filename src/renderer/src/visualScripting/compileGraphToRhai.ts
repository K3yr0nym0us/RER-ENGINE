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
} from './nodeDefinitions'
import { compileEntityIdExpr, compileRhaiExprField, graphContext } from './compileEntityId'
import { normalizeReflectionTier } from '../context/useContextEngine/types'
import { validateGraph } from './validateGraph'

function escapeRhaiString(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"')
}

function nodesById(nodes: VisualGraphNode[]): Map<string, VisualGraphNode> {
  return new Map(nodes.map((n) => [n.id, n]))
}

function nextExecTarget(
  doc: VisualGraphDocument,
  nodeId: string,
  sourceHandle: string,
): string | null {
  const edge = doc.edges.find(
    (e) => e.source === nodeId && e.sourceHandle === sourceHandle,
  )
  if (!edge) return null
  const target = doc.nodes.find((n) => n.id === edge.target)
  if (!target || edge.targetHandle !== EXEC_IN) return null
  return target.id
}

function indentBlock(lines: string[], extra = 4): string[] {
  const pad = ' '.repeat(extra)
  return lines.map((l) => (l ? `${pad}${l}` : l))
}

function compileExecChain(
  doc: VisualGraphDocument,
  byId: Map<string, VisualGraphNode>,
  startId: string | null,
  visited: Set<string>,
  options?: CompileGraphOptions,
): string[] {
  const lines: string[] = []
  let current = startId
  while (current) {
    if (visited.has(current)) break
    visited.add(current)
    const node = byId.get(current)
    if (!node) break

    switch (node.type) {
      case VISUAL_NODE_TYPES.LOG: {
        const msg = String(node.data?.message ?? '')
        lines.push(`engine.log("${escapeRhaiString(msg)}");`)
        current = nextExecTarget(doc, current, EXEC_OUT)
        break
      }
      case VISUAL_NODE_TYPES.PLAY_ANIMATION: {
        const targetId = Math.floor(Number(node.data?.entityId ?? 0))
        const animName = String(node.data?.animationName ?? '').trim()
        const idExpr = compileEntityIdExpr(doc, targetId)
        lines.push(`engine.play_animation(${idExpr}, "${escapeRhaiString(animName)}");`)
        current = nextExecTarget(doc, current, EXEC_OUT)
        break
      }
      case VISUAL_NODE_TYPES.SET_SCALE: {
        const sx = compileRhaiExprField(node.data?.scaleX, '1.0')
        const sy = compileRhaiExprField(node.data?.scaleY, '1.0')
        lines.push(`engine.set_scale(entity.id, ${sx}, ${sy});`)
        current = nextExecTarget(doc, current, EXEC_OUT)
        break
      }
      case VISUAL_NODE_TYPES.MOVE_TO: {
        const x = compileRhaiExprField(node.data?.x, 'entity.x')
        const y = compileRhaiExprField(node.data?.y, 'entity.y')
        lines.push(`engine.move_to(entity.id, ${x}, ${y});`)
        current = nextExecTarget(doc, current, EXEC_OUT)
        break
      }
      case VISUAL_NODE_TYPES.TRANSLATE: {
        const dx = compileRhaiExprField(node.data?.dx, '0.0')
        const dy = compileRhaiExprField(node.data?.dy, '0.0')
        lines.push(`engine.translate(entity.id, ${dx}, ${dy});`)
        current = nextExecTarget(doc, current, EXEC_OUT)
        break
      }
      case VISUAL_NODE_TYPES.SET_REFLECTION_TIER: {
        const tier = normalizeReflectionTier(node.data?.tier)
        lines.push(`engine.set_reflection_tier("${tier}");`)
        current = nextExecTarget(doc, current, EXEC_OUT)
        break
      }
      case VISUAL_NODE_TYPES.SEQUENCE: {
        lines.push(...compileExecChain(doc, byId, nextExecTarget(doc, current, THEN_0), visited, options))
        lines.push(...compileExecChain(doc, byId, nextExecTarget(doc, current, THEN_1), visited, options))
        current = nextExecTarget(doc, current, EXEC_OUT)
        break
      }
      case VISUAL_NODE_TYPES.IF: {
        const expr = String(node.data?.expression ?? 'true').trim() || 'true'
        const trueBody = compileExecChain(doc, byId, nextExecTarget(doc, current, THEN_TRUE), visited, options)
        const falseBody = compileExecChain(doc, byId, nextExecTarget(doc, current, THEN_FALSE), visited, options)
        lines.push(`if ${expr} {`)
        lines.push(...indentBlock(trueBody))
        if (falseBody.length > 0) {
          lines.push('} else {')
          lines.push(...indentBlock(falseBody))
        }
        lines.push('}')
        current = nextExecTarget(doc, current, EXEC_OUT)
        break
      }
      case VISUAL_NODE_TYPES.FOR_REPEAT: {
        const count = Math.max(0, Math.floor(Number(node.data?.count ?? 1)))
        const body = compileExecChain(doc, byId, nextExecTarget(doc, current, LOOP_BODY), visited, options)
        lines.push(`for _i in 0..${count} {`)
        lines.push(...indentBlock(body))
        lines.push('}')
        current = nextExecTarget(doc, current, EXEC_OUT)
        break
      }
      case VISUAL_NODE_TYPES.FOR_EACH_ENTITY: {
        const fromNode = Array.isArray(node.data?.entityIds)
          ? (node.data.entityIds as number[]).filter((id) => Number.isFinite(id))
          : []
        const fromScene = options?.sceneEntities?.map((e) => e.id) ?? []
        const ids = fromNode.length > 0 ? fromNode : fromScene
        const body = compileExecChain(doc, byId, nextExecTarget(doc, current, LOOP_BODY), visited, options)
        lines.push(`for entity_id in [${ids.join(', ')}] {`)
        lines.push(...indentBlock(['let entity = #{ id: entity_id };', ...body]))
        lines.push('}')
        current = nextExecTarget(doc, current, EXEC_OUT)
        break
      }
      default:
        current = nextExecTarget(doc, current, EXEC_OUT)
    }
  }
  return lines
}

function compileEventBody(
  doc: VisualGraphDocument,
  eventNodeId: string,
  options?: CompileGraphOptions,
): string {
  const byId = nodesById(doc.nodes)
  const visited = new Set<string>()
  const start = nextExecTarget(doc, eventNodeId, EXEC_OUT)
  const body = compileExecChain(doc, byId, start, visited, options)
  if (body.length === 0) {
    return '    // (empty)\n'
  }
  return `${body.map((l) => `    ${l}`).join('\n')}\n`
}

export interface CompileGraphOptions {
  sceneEntities?: Array<{ id: number; name?: string; category?: string }>
}

export interface CompileResult {
  source: string
  errors: string[]
}

function compileSceneGraph(
  doc: VisualGraphDocument,
  options?: CompileGraphOptions,
): string {
  const begin = doc.nodes.find((n) => n.type === VISUAL_NODE_TYPES.SCENE_BEGIN)
  const tick = doc.nodes.find((n) => n.type === VISUAL_NODE_TYPES.TICK)

  const startBody = begin ? compileEventBody(doc, begin.id, options) : '    // (empty)\n'
  const tickBody = tick ? compileEventBody(doc, tick.id, options) : '    // (empty)\n'

  return `// RER — scene logic (compiled from nodes)
// scene_id: ${doc.sceneId ?? 0}

fn on_scene_start() {
${startBody}}

fn on_scene_tick(dt) {
${tickBody}}
`
}

function compileEntityGraph(doc: VisualGraphDocument): string {
  const start = doc.nodes.find((n) => n.type === VISUAL_NODE_TYPES.ENTITY_START)
  const update = doc.nodes.find((n) => n.type === VISUAL_NODE_TYPES.ENTITY_UPDATE)

  const startBody = start ? compileEventBody(doc, start.id) : '    // (empty)\n'
  const updateBody = update ? compileEventBody(doc, update.id) : '    // (empty)\n'

  return `// RER — entity logic (compiled from nodes)
// entity_id: ${doc.entityId ?? 0}

fn on_start(entity) {
${startBody}}

fn update(entity, dt) {
${updateBody}}
`
}

/** Compila el grafo canónico a fuente Rhai para el motor. */
export function compileGraphToRhai(
  doc: VisualGraphDocument,
  options?: CompileGraphOptions,
): CompileResult {
  const validation = validateGraph(doc)
  if (validation.length > 0) {
    return {
      source: '',
      errors: validation.map((v) => v.message),
    }
  }

  const context = graphContext(doc)
  const source = context === 'entity'
    ? compileEntityGraph(doc)
    : compileSceneGraph(doc, options)

  return { source, errors: [] }
}
