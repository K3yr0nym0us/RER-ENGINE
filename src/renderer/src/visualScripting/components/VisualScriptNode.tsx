import { Position, type NodeProps } from '@xyflow/react'

import { useTraslate } from '@hooks'
import {
  EXEC_IN,
  EXEC_OUT,
  LOOP_BODY,
  NODE_DEFINITIONS,
  THEN_0,
  THEN_1,
  THEN_FALSE,
  THEN_TRUE,
  VISUAL_NODE_TYPES,
  type VisualNodeType,
} from '../nodeDefinitions'
import { VisualScriptHandle } from './VisualScriptHandle'

export interface VisualScriptNodeData {
  nodeType: VisualNodeType
  message?: string
  expression?: string
  count?: number
  entityIds?: number[]
  entityId?: number
  animationName?: string
  scaleX?: string
  scaleY?: string
  x?: string
  y?: string
  dx?: string
  dy?: string
  [key: string]: unknown
}

const categoryColor: Record<string, string> = {
  event: '#6f42c1',
  flow: '#0d6efd',
  action: '#198754',
}

export function VisualScriptNode({ data, selected }: NodeProps) {
  const { t } = useTraslate()
  const nodeType = (data as VisualScriptNodeData).nodeType
  const def = NODE_DEFINITIONS[nodeType]
  const color = categoryColor[def?.category ?? 'action'] ?? '#6c757d'
  const d = data as VisualScriptNodeData

  const isSceneEvent = nodeType === VISUAL_NODE_TYPES.SCENE_BEGIN || nodeType === VISUAL_NODE_TYPES.TICK
  const isEntityEvent = nodeType === VISUAL_NODE_TYPES.ENTITY_START || nodeType === VISUAL_NODE_TYPES.ENTITY_UPDATE
  const isEvent = isSceneEvent || isEntityEvent
  const isSequence = nodeType === VISUAL_NODE_TYPES.SEQUENCE
  const isIf = nodeType === VISUAL_NODE_TYPES.IF
  const isForRepeat = nodeType === VISUAL_NODE_TYPES.FOR_REPEAT
  const isForEach = nodeType === VISUAL_NODE_TYPES.FOR_EACH_ENTITY
  const isLog = nodeType === VISUAL_NODE_TYPES.LOG
  const isPlayAnimation = nodeType === VISUAL_NODE_TYPES.PLAY_ANIMATION
  const isSetScale = nodeType === VISUAL_NODE_TYPES.SET_SCALE
  const isMoveTo = nodeType === VISUAL_NODE_TYPES.MOVE_TO
  const isTranslate = nodeType === VISUAL_NODE_TYPES.TRANSLATE
  const hasLoopBody = isForRepeat || isForEach
  const hasMultipleRightOutputs = isSequence || isIf || hasLoopBody

  const execOutStyle = hasMultipleRightOutputs
    ? { top: '82%', background: '#adb5bd' as const }
    : { background: '#adb5bd' as const }

  return (
    <div
      className="visual-script-node border rounded shadow-sm bg-dark text-light"
      style={{
        minWidth: 160,
        borderColor: selected ? '#ffc107' : color,
        borderWidth: selected ? 2 : 1,
      }}
    >
      <div
        className="px-2 py-1 small fw-semibold text-truncate"
        style={{ background: color, borderRadius: '0.25rem 0.25rem 0 0' }}
      >
        {def ? t(def.label) : nodeType}
      </div>
      <div className="px-2 py-2 small">
        {isLog && (
          <span className="text-truncate d-block" title={String(d.message ?? '')}>
            {String(d.message ?? '')}
          </span>
        )}
        {isPlayAnimation && (
          <span className="text-truncate d-block text-secondary" title={String(d.animationName ?? '')}>
            {d.entityId ? `${d.entityId} · ` : ''}{String(d.animationName || t('Select animation'))}
          </span>
        )}
        {isSetScale && (
          <span className="text-truncate d-block text-secondary" title={`${d.scaleX}, ${d.scaleY}`}>
            {String(d.scaleX ?? '1.0')}, {String(d.scaleY ?? '1.0')}
          </span>
        )}
        {isMoveTo && (
          <span className="text-truncate d-block text-secondary" title={`${d.x}, ${d.y}`}>
            {String(d.x ?? 'entity.x')}, {String(d.y ?? 'entity.y')}
          </span>
        )}
        {isTranslate && (
          <span className="text-truncate d-block text-secondary" title={`${d.dx}, ${d.dy}`}>
            Δ {String(d.dx ?? '0.0')}, {String(d.dy ?? '0.0')}
          </span>
        )}
        {isSequence && <span className="text-secondary">{t('Then 0 → Then 1')}</span>}
        {isIf && (
          <span className="text-truncate d-block text-secondary" title={String(d.expression ?? '')}>
            {String(d.expression ?? 'true')}
          </span>
        )}
        {isForRepeat && (
          <span className="text-secondary">{t('Count')}: {String(d.count ?? 1)}</span>
        )}
        {isForEach && (
          <span className="text-secondary">
            {(d.entityIds?.length ?? 0) > 0
              ? `${d.entityIds?.length} ${t('entities')}`
              : t('All scene entities')}
          </span>
        )}
        {isEvent && <span className="text-secondary">{t('Event')}</span>}
      </div>

      {!isEvent && (
        <VisualScriptHandle
          type="target"
          position={Position.Left}
          id={EXEC_IN}
          style={{ background: '#adb5bd' }}
        />
      )}
      {isSequence && (
        <>
          <VisualScriptHandle
            type="source"
            position={Position.Right}
            id={THEN_0}
            style={{ top: '30%', background: '#adb5bd' }}
          />
          <VisualScriptHandle
            type="source"
            position={Position.Right}
            id={THEN_1}
            style={{ top: '52%', background: '#adb5bd' }}
          />
        </>
      )}
      {isIf && (
        <>
          <VisualScriptHandle
            type="source"
            position={Position.Right}
            id={THEN_TRUE}
            style={{ top: '28%', background: '#20c997' }}
          />
          <VisualScriptHandle
            type="source"
            position={Position.Right}
            id={THEN_FALSE}
            style={{ top: '52%', background: '#dc3545' }}
          />
        </>
      )}
      {hasLoopBody && (
        <VisualScriptHandle
          type="source"
          position={Position.Right}
          id={LOOP_BODY}
          style={{ top: '38%', background: '#ffc107' }}
        />
      )}
      <VisualScriptHandle
        type="source"
        position={Position.Right}
        id={EXEC_OUT}
        style={execOutStyle}
      />
    </div>
  )
}
