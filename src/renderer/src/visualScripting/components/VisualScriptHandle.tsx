import type { CSSProperties } from 'react'

import { Handle, Position } from '@xyflow/react'

import { useTraslate } from '@hooks'
import { HANDLE_TOOLTIP_KEYS } from '../handleTooltips'

interface VisualScriptHandleProps {
  type: 'source' | 'target'
  position: Position
  id: string
  style?: CSSProperties
  tooltipKey?: string
}

/**
 * Handle de React Flow con tooltip nativo (`title`).
 * No envolver en OverlayTrigger: rompe el posicionamiento absoluto de xyflow.
 */
export function VisualScriptHandle({
  type,
  position,
  id,
  style,
  tooltipKey,
}: VisualScriptHandleProps) {
  const { t } = useTraslate()
  const key = tooltipKey ?? HANDLE_TOOLTIP_KEYS[id] ?? id

  return (
    <Handle
      type={type}
      position={position}
      id={id}
      title={t(key)}
      style={style}
    />
  )
}
