import { useEffect } from 'react'

import { Bricks } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { usePointDrawing } from '@hooks'
import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'

interface Props {
  activeTool: 'draw_collider' | 'draw_execution_area' | null
  setActiveTool: (tool: 'draw_collider' | 'draw_execution_area' | null) => void
}

export function ColliderToolButton({ activeTool, setActiveTool }: Props) {
  const { t } = useTraslate()
  const { engineReady, send, toolProgress } = useContextEngine()
  const colliderTool = usePointDrawing('draw_collider', 4, send, toolProgress)
  const isActive = activeTool === 'draw_collider'
  const pointsPlaced = isActive ? colliderTool.progress : 0
  const pointsLeft = Math.max(0, colliderTool.totalPoints - pointsPlaced)

  useEffect(() => {
    if (!isActive) return
    if (!colliderTool.isActive) {
      setActiveTool(null)
    }
  }, [isActive, colliderTool.isActive, setActiveTool])

  const tooltipText = isActive
    ? `${t('Active tool')} (${pointsPlaced}/${colliderTool.totalPoints}). ${t('Click again to cancel')}`
    : t('Click 4 areas to create a collision box')

  const buttonClass = isActive
    ? 'btn btn-sm btn-info mb-2 d-flex flex-column justify-content-center align-items-center'
    : 'btn btn-sm btn-outline-info mb-2 d-flex flex-column justify-content-center align-items-center'

  const handleClick = () => {
    if (isActive) {
      colliderTool.cancel()
      setActiveTool(null)
      return
    }

    setActiveTool('draw_collider')
    colliderTool.start()
  }

  return (
    <AppTooltip content={tooltipText} place="right">
      <button
        className={buttonClass}
        style={{ height: 64, width: 64 }}
        onClick={handleClick}
        disabled={!engineReady}
        aria-pressed={isActive}
      >
        <span style={{ fontSize: 10, lineHeight: 1.1 }}>{t('Create')}</span>
        <Bricks className="my-1" size={20} />
        <span style={{ fontSize: 10, lineHeight: 1.1 }}>
          {isActive ? `${pointsLeft} ${t('remaining')}` : t('Wall')}
        </span>
      </button>
    </AppTooltip>
  )
}
