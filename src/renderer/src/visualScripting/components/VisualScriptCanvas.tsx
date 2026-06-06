import { useCallback, useEffect, useMemo, useState } from 'react'

import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  addEdge,
  useNodesState,
  useEdgesState,
  type Connection,
  type Edge,
  type Node,
  type NodeTypes,
} from '@xyflow/react'
import '@xyflow/react/dist/style.css'

import type { Entity3D, VisualGraphDocument, VisualGraphEdge, VisualGraphNode } from '@shared-types'

import {
  NODE_DEFINITIONS,
  VISUAL_NODE_TYPES,
  nodesForContext,
  type VisualGraphContext,
  type VisualNodeType,
} from '../nodeDefinitions'
import { VisualScriptNode, type VisualScriptNodeData } from './VisualScriptNode'
import { THEME_PRIMARY } from '../../styles/theme'

const MINIMAP_NODE_COLORS: Record<string, string> = {
  event: THEME_PRIMARY,
  flow: '#0d6efd',
  action: '#198754',
}
import { VisualScriptVariablesPanel } from './VisualScriptVariablesPanel'
import { animationNamesForEntity } from '../resolveSceneEntities'
import { useTraslate } from '@hooks'

const nodeTypes: NodeTypes = {
  visualScript: VisualScriptNode,
}

function newNodeId(): string {
  return `n_${Date.now()}_${Math.random().toString(36).slice(2, 9)}`
}

function graphToFlow(doc: VisualGraphDocument): { nodes: Node[]; edges: Edge[] } {
  return {
    nodes: doc.nodes.map((n) => ({
      id: n.id,
      type: 'visualScript',
      position: n.position,
      data: { nodeType: n.type as VisualNodeType, ...n.data },
    })),
    edges: doc.edges.map((e) => ({
      id: e.id,
      source: e.source,
      target: e.target,
      sourceHandle: e.sourceHandle,
      targetHandle: e.targetHandle,
    })),
  }
}

function flowToGraph(
  context: VisualGraphContext,
  sceneId: number | undefined,
  entityId: number | undefined,
  nodes: Node[],
  edges: Edge[],
): VisualGraphDocument {
  const graphNodes: VisualGraphNode[] = nodes.map((n) => {
    const d = n.data as VisualScriptNodeData
    const { nodeType, ...rest } = d
    return {
      id: n.id,
      type: nodeType,
      position: n.position,
      data: rest,
    }
  })
  const graphEdges: VisualGraphEdge[] = edges.map((e) => ({
    id: e.id,
    source: e.source,
    sourceHandle: e.sourceHandle ?? 'exec',
    target: e.target,
    targetHandle: e.targetHandle ?? 'exec_in',
  }))
  return {
    version: 1,
    context,
    ...(sceneId != null ? { sceneId } : {}),
    ...(entityId != null ? { entityId } : {}),
    nodes: graphNodes,
    edges: graphEdges,
  }
}

interface Props {
  context: VisualGraphContext
  sceneId?: number
  entityId?: number
  entityName?: string
  sceneEntities?: Entity3D[]
  initialGraph?: VisualGraphDocument
  /** Ocupa el espacio vertical disponible (ventana redimensionable). */
  fill?: boolean
  panelHeight?: number
  onGraphChange?: (doc: VisualGraphDocument) => void
}

export function VisualScriptCanvas({
  context,
  sceneId,
  entityId,
  entityName,
  sceneEntities,
  initialGraph,
  fill,
  panelHeight,
  onGraphChange,
}: Props) {
  const { t } = useTraslate()
  const emptyGraph = useMemo(
    () => initialGraph ?? {
      version: 1 as const,
      context,
      ...(sceneId != null ? { sceneId } : {}),
      ...(entityId != null ? { entityId } : {}),
      nodes: [],
      edges: [],
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  )

  const initial = useMemo(() => graphToFlow(emptyGraph), [emptyGraph])

  const [nodes, setNodes, onNodesChange] = useNodesState(initial.nodes)
  const [edges, setEdges, onEdgesChange] = useEdgesState(initial.edges)
  const [selectedId, setSelectedId] = useState<string | null>(null)

  const availableNodes = useMemo(() => nodesForContext(context), [context])

  const emitChange = useCallback(
    (n: Node[], e: Edge[]) => {
      onGraphChange?.(flowToGraph(context, sceneId, entityId, n, e))
    },
    [onGraphChange, context, sceneId, entityId],
  )

  const onConnect = useCallback(
    (conn: Connection) => {
      setEdges((eds) => {
        const next = addEdge({ ...conn, id: `e_${newNodeId()}` }, eds)
        emitChange(nodes, next)
        return next
      })
    },
    [emitChange, nodes, setEdges],
  )

  const addNode = (type: VisualNodeType) => {
    const def = NODE_DEFINITIONS[type]
    const id = newNodeId()
    const defaultData = { ...def.defaultData }
    if (type === VISUAL_NODE_TYPES.PLAY_ANIMATION) {
      const targetEntity = entityId != null
        ? sceneEntities?.find((entity) => entity.id === entityId)
        : sceneEntities?.[0]
      if (targetEntity) {
        defaultData.entityId = targetEntity.id
        const names = animationNamesForEntity(targetEntity)
        if (names[0]) defaultData.animationName = names[0]
      }
    }
    const node: Node = {
      id,
      type: 'visualScript',
      position: { x: 80 + nodes.length * 24, y: 80 + nodes.length * 24 },
      data: { nodeType: type, ...defaultData },
    }
    setNodes((nds) => {
      const next = [...nds, node]
      emitChange(next, edges)
      return next
    })
  }

  const selectedNode = nodes.find((n) => n.id === selectedId)
  const selectedData = selectedNode?.data as VisualScriptNodeData | undefined

  const playAnimationEntityId = selectedData?.nodeType === VISUAL_NODE_TYPES.PLAY_ANIMATION
    ? Number(selectedData.entityId) || entityId || sceneEntities?.[0]?.id || 0
    : 0
  const playAnimationEntity = sceneEntities?.find((entity) => entity.id === playAnimationEntityId)
  const playAnimationOptions = animationNamesForEntity(playAnimationEntity)

  const updateSelectedField = (patch: Partial<VisualScriptNodeData>) => {
    if (!selectedId) return
    setNodes((nds) => {
      const next = nds.map((n) =>
        n.id === selectedId
          ? { ...n, data: { ...(n.data as VisualScriptNodeData), ...patch } }
          : n,
      )
      emitChange(next, edges)
      return next
    })
  }

  useEffect(() => {
    if (selectedData?.nodeType !== VISUAL_NODE_TYPES.PLAY_ANIMATION) return
    const currentId = Number(selectedData.entityId)
    if (currentId > 0) return
    const fallbackId = entityId ?? sceneEntities?.[0]?.id
    if (!fallbackId) return
    const fallbackEntity = sceneEntities?.find((entity) => entity.id === fallbackId)
    const names = animationNamesForEntity(fallbackEntity)
    updateSelectedField({
      entityId: fallbackId,
      animationName: String(selectedData.animationName ?? '') || names[0] || '',
    })
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId, sceneEntities, entityId])

  useEffect(() => {
    emitChange(nodes, edges)
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const fixedCanvasHeight = Math.max(360, panelHeight ?? Math.floor((window.screen?.availHeight ?? 900) * 0.42))
  const rootStyle = fill
    ? { flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' as const }
    : { height: fixedCanvasHeight, minHeight: fixedCanvasHeight, display: 'flex', flexDirection: 'column' as const }

  const nodeButton = (type: VisualNodeType) => {
    const def = NODE_DEFINITIONS[type]
    const btnClass = def.category === 'event'
      ? 'btn-outline-info'
      : def.category === 'flow'
        ? 'btn-outline-primary'
        : 'btn-outline-success'
    return (
      <button
        key={type}
        type="button"
        className={`btn btn-sm ${btnClass}`}
        onClick={() => addNode(type)}
      >
        + {t(def.label)}
      </button>
    )
  }

  return (
    <div className="d-flex flex-column visual-scripting-canvas-root" style={rootStyle}>
      <div className="d-flex gap-1 flex-wrap mb-2 flex-shrink-0">
        {availableNodes.map(nodeButton)}
      </div>
      <div
        className="d-flex flex-grow-1 border rounded overflow-hidden visual-scripting-canvas"
        style={{ minHeight: fill ? 0 : Math.max(280, fixedCanvasHeight - 48) }}
      >
        <VisualScriptVariablesPanel
          context={context}
          sceneEntities={sceneEntities}
          entityId={entityId}
          entityName={entityName}
          onPickAnimation={
            selectedData?.nodeType === VISUAL_NODE_TYPES.PLAY_ANIMATION
              ? (pickedEntityId, animationName) => {
                updateSelectedField({ entityId: pickedEntityId, animationName })
              }
              : undefined
          }
        />
        <div className="flex-grow-1 visual-scripting-flow" style={{ minWidth: 0, height: '100%' }}>
          <ReactFlow
            nodes={nodes}
            edges={edges}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            nodeTypes={nodeTypes}
            fitView
            onSelectionChange={({ nodes: sel }) => setSelectedId(sel[0]?.id ?? null)}
            proOptions={{ hideAttribution: true }}
            style={{ width: '100%', height: '100%' }}
          >
            <Background gap={16} size={1} color="#3d4d66" />
            <Controls showInteractive={false} position="bottom-left" />
            <MiniMap
              position="bottom-right"
              bgColor="#1a1f2e"
              maskColor="rgba(15, 20, 25, 0.8)"
              nodeStrokeWidth={2}
              nodeColor={(node) => {
                const nodeType = (node.data as VisualScriptNodeData)?.nodeType
                const category = nodeType ? NODE_DEFINITIONS[nodeType]?.category : undefined
                return MINIMAP_NODE_COLORS[category ?? ''] ?? '#6c757d'
              }}
              style={{
                background: '#1a1f2e',
                border: '1px solid #3d4466',
                borderRadius: 6,
              }}
            />
          </ReactFlow>
        </div>
        {selectedData?.nodeType === VISUAL_NODE_TYPES.LOG && (
          <div className="border-start border-secondary p-2 bg-dark text-light visual-scripting-inspector" style={{ width: 220 }}>
            <label className="form-label small mb-1 text-secondary">{t('Message')}</label>
            <input
              className="form-control form-control-sm bg-dark text-light border-secondary"
              value={String(selectedData.message ?? '')}
              onChange={(e) => updateSelectedField({ message: e.target.value })}
            />
          </div>
        )}
        {selectedData?.nodeType === VISUAL_NODE_TYPES.IF && (
          <div className="border-start border-secondary p-2 bg-dark text-light visual-scripting-inspector" style={{ width: 220 }}>
            <label className="form-label small mb-1 text-secondary">{t('Condition (Rhai)')}</label>
            <input
              className="form-control form-control-sm bg-dark text-light border-secondary"
              value={String(selectedData.expression ?? 'true')}
              onChange={(e) => updateSelectedField({ expression: e.target.value })}
            />
          </div>
        )}
        {selectedData?.nodeType === VISUAL_NODE_TYPES.FOR_REPEAT && (
          <div className="border-start border-secondary p-2 bg-dark text-light visual-scripting-inspector" style={{ width: 220 }}>
            <label className="form-label small mb-1 text-secondary">{t('Count')}</label>
            <input
              type="number"
              min={0}
              className="form-control form-control-sm bg-dark text-light border-secondary"
              value={Number(selectedData.count ?? 1)}
              onChange={(e) => updateSelectedField({ count: Math.max(0, Number(e.target.value)) })}
            />
          </div>
        )}
        {selectedData?.nodeType === VISUAL_NODE_TYPES.FOR_EACH_ENTITY && (
          <div className="border-start border-secondary p-2 bg-dark text-light visual-scripting-inspector" style={{ width: 220 }}>
            <label className="form-label small mb-1 text-secondary">{t('Entity ids (optional)')}</label>
            <input
              className="form-control form-control-sm bg-dark text-light border-secondary"
              placeholder={t('All scene entities')}
              value={(selectedData.entityIds ?? []).join(', ')}
              onChange={(e) => {
                const ids = e.target.value
                  .split(',')
                  .map((s) => Number(s.trim()))
                  .filter((n) => Number.isFinite(n) && n > 0)
                updateSelectedField({ entityIds: ids })
              }}
            />
            <p className="small text-secondary mt-1 mb-0">{t('Loop variable: entity_id')}</p>
          </div>
        )}
        {selectedData?.nodeType === VISUAL_NODE_TYPES.SET_SCALE && (
          <div className="border-start border-secondary p-2 bg-dark text-light visual-scripting-inspector" style={{ width: 240 }}>
            <label className="form-label small mb-1 text-secondary">{t('Scale X (Rhai)')}</label>
            <input
              className="form-control form-control-sm bg-dark text-light border-secondary mb-2"
              value={String(selectedData.scaleX ?? '1.0')}
              onChange={(e) => updateSelectedField({ scaleX: e.target.value })}
            />
            <label className="form-label small mb-1 text-secondary">{t('Scale Y (Rhai)')}</label>
            <input
              className="form-control form-control-sm bg-dark text-light border-secondary"
              value={String(selectedData.scaleY ?? '1.0')}
              onChange={(e) => updateSelectedField({ scaleY: e.target.value })}
            />
          </div>
        )}
        {selectedData?.nodeType === VISUAL_NODE_TYPES.MOVE_TO && (
          <div className="border-start border-secondary p-2 bg-dark text-light visual-scripting-inspector" style={{ width: 240 }}>
            <label className="form-label small mb-1 text-secondary">{t('X (Rhai)')}</label>
            <input
              className="form-control form-control-sm bg-dark text-light border-secondary mb-2"
              value={String(selectedData.x ?? 'entity.x')}
              onChange={(e) => updateSelectedField({ x: e.target.value })}
            />
            <label className="form-label small mb-1 text-secondary">{t('Y (Rhai)')}</label>
            <input
              className="form-control form-control-sm bg-dark text-light border-secondary"
              value={String(selectedData.y ?? 'entity.y')}
              onChange={(e) => updateSelectedField({ y: e.target.value })}
            />
          </div>
        )}
        {selectedData?.nodeType === VISUAL_NODE_TYPES.TRANSLATE && (
          <div className="border-start border-secondary p-2 bg-dark text-light visual-scripting-inspector" style={{ width: 240 }}>
            <label className="form-label small mb-1 text-secondary">{t('Delta X (Rhai)')}</label>
            <input
              className="form-control form-control-sm bg-dark text-light border-secondary mb-2"
              value={String(selectedData.dx ?? '0.0')}
              onChange={(e) => updateSelectedField({ dx: e.target.value })}
            />
            <label className="form-label small mb-1 text-secondary">{t('Delta Y (Rhai)')}</label>
            <input
              className="form-control form-control-sm bg-dark text-light border-secondary"
              value={String(selectedData.dy ?? '0.0')}
              onChange={(e) => updateSelectedField({ dy: e.target.value })}
            />
          </div>
        )}
        {selectedData?.nodeType === VISUAL_NODE_TYPES.PLAY_ANIMATION && (
          <div className="border-start border-secondary p-2 bg-dark text-light visual-scripting-inspector" style={{ width: 240 }}>
            <label className="form-label small mb-1 text-secondary">{t('Entity')}</label>
            <select
              className="form-select form-select-sm bg-dark text-light border-secondary mb-2"
              value={playAnimationEntityId}
              onChange={(e) => {
                const nextEntityId = Number(e.target.value)
                const nextEntity = sceneEntities?.find((entity) => entity.id === nextEntityId)
                const names = animationNamesForEntity(nextEntity)
                const nextAnimation = names.includes(String(selectedData.animationName ?? ''))
                  ? String(selectedData.animationName ?? '')
                  : (names[0] ?? '')
                updateSelectedField({ entityId: nextEntityId, animationName: nextAnimation })
              }}
            >
              {(sceneEntities ?? []).map((entity) => (
                <option key={entity.id} value={entity.id}>
                  {entity.name || `Entity ${entity.id}`} ({entity.id})
                </option>
              ))}
            </select>
            <label className="form-label small mb-1 text-secondary">{t('Animation')}</label>
            <select
              className="form-select form-select-sm bg-dark text-light border-secondary"
              value={String(selectedData.animationName ?? '')}
              onChange={(e) => updateSelectedField({ animationName: e.target.value })}
              disabled={playAnimationOptions.length === 0}
            >
              {playAnimationOptions.length === 0 ? (
                <option value="">{t('No animations for this entity')}</option>
              ) : (
                playAnimationOptions.map((name) => (
                  <option key={name} value={name}>{name}</option>
                ))
              )}
            </select>
            <p className="small text-secondary mt-2 mb-0">{t('Pick from Animations panel or select here')}</p>
          </div>
        )}
      </div>
    </div>
  )
}

export function getGraphFromCanvas(
  context: VisualGraphContext,
  sceneId: number | undefined,
  entityId: number | undefined,
  nodes: Node[],
  edges: Edge[],
): VisualGraphDocument {
  return flowToGraph(context, sceneId, entityId, nodes, edges)
}
