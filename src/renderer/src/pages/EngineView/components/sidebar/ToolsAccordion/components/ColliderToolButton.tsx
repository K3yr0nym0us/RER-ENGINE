import { useEffect } from 'react'
import { Bricks } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { usePointDrawing } from '@hooks'
import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'
import type { ProjectType } from '@shared-types'

import { useQuickBuild } from '../../../../../../context/QuickBuildContext'
import { usePlaneTool } from '../../../../../../context/PlaneToolContext'

interface Props {
  projectType: ProjectType
  activeTool: 'draw_collider' | 'draw_execution_area' | null
  setActiveTool: (tool: 'draw_collider' | 'draw_execution_area' | null) => void
}

export function ColliderToolButton({ projectType, activeTool, setActiveTool }: Props) {
  const { t } = useTraslate()
  const { engineReady, send, toolProgress } = useContextEngine()
  const { activePlaneTool, setActivePlaneTool } = usePlaneTool()
  const { setActiveBluePrint } = useQuickBuild()
  const is3D = projectType === '3D'

  const colliderTool = usePointDrawing('draw_collider', 4, send, toolProgress)
  const isActive2D = activeTool === 'draw_collider'
  const isActive3D = is3D && activePlaneTool === 'draw_collider'
  const isActive = is3D ? isActive3D : isActive2D
  const pointsPlaced = isActive2D ? colliderTool.progress : 0
  const pointsLeft = Math.max(0, colliderTool.totalPoints - pointsPlaced)

  useEffect(() => {
    if (is3D && !activePlaneTool && activeTool === 'draw_collider') {
      setActiveTool(null)
    }
  }, [is3D, activePlaneTool, activeTool, setActiveTool])

  const handleClick = () => {
    if (is3D) {
      if (isActive3D) {
        setActivePlaneTool(null)
        setActiveTool(null)
        return
      }
      setActiveBluePrint(null)
      setActivePlaneTool('draw_collider')
      setActiveTool('draw_collider')
      return
    }

    if (isActive2D) {
      colliderTool.cancel()
      setActiveTool(null)
      return
    }

    setActiveTool('draw_collider')
    colliderTool.start()
  }

  const tooltipText = is3D
    ? isActive3D
      ? <>{t('Active mode')}: {t('Invisible wall')}<br />{t('Move over viewport, Q/E to rotate, click to place once')}<br />{t('Click again to deactivate')}</>
      : <>{t('Create a transparent collision wall')}<br />{t('Default size 4×3 m')}</>
    : isActive2D
      ? `${t('Active tool')} (${pointsPlaced}/${colliderTool.totalPoints}). ${t('Click again to cancel')}`
      : t('Click 4 areas to create a collision box')

  const buttonClass = isActive
    ? 'btn btn-sm btn-info mb-2 d-flex flex-column justify-content-center align-items-center'
    : 'btn btn-sm btn-outline-info mb-2 d-flex flex-column justify-content-center align-items-center'

  return (
    <AppTooltip content={tooltipText} place="right">
      <button
        className={buttonClass}
        style={{ height: 64, width: 64 }}
        onClick={handleClick}
        disabled={!engineReady}
        aria-pressed={isActive}
      >
        <span style={{ fontSize: 10, lineHeight: 1.1 }}>
          {isActive3D ? t('Active') : t('Create')}
        </span>
        <Bricks className="my-1" size={20} />
        <span style={{ fontSize: 10, lineHeight: 1.1 }}>
          {isActive3D ? '? ON' : isActive && !is3D ? `${pointsLeft} ${t('remaining')}` : t('Wall')}
        </span>
      </button>
    </AppTooltip>
  )
}
