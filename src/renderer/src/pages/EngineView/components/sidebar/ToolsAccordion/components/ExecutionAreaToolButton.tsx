import { useEffect } from 'react'
import { CodeSquare } from 'react-bootstrap-icons'

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

export function ExecutionAreaToolButton({ projectType, activeTool, setActiveTool }: Props) {
  const { t } = useTraslate()
  const { engineReady, send, toolProgress } = useContextEngine()
  const { activePlaneTool, setActivePlaneTool } = usePlaneTool()
  const { setActiveBluePrint } = useQuickBuild()
  const is3D = projectType === '3D'

  const executionAreaTool = usePointDrawing('draw_execution_area', 4, send, toolProgress)
  const isActive2D = activeTool === 'draw_execution_area'
  const isActive3D = is3D && activePlaneTool === 'draw_execution_area'
  const isActive = is3D ? isActive3D : isActive2D
  const pointsPlaced = isActive2D ? executionAreaTool.progress : 0
  const pointsLeft = Math.max(0, executionAreaTool.totalPoints - pointsPlaced)

  useEffect(() => {
    if (is3D && !activePlaneTool && activeTool === 'draw_execution_area') {
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
      setActivePlaneTool('draw_execution_area')
      setActiveTool('draw_execution_area')
      return
    }

    if (isActive2D) {
      executionAreaTool.cancel()
      setActiveTool(null)
      return
    }

    setActiveTool('draw_execution_area')
    executionAreaTool.start()
  }

  const tooltipText = is3D
    ? isActive3D
      ? <>{t('Active mode')}: Trigger<br />{t('Move over viewport, Q/E to rotate, click to place once')}<br />{t('Click again to deactivate')}</>
      : <>{t('Create a transparent trigger plane')}<br />{t('Default size 4×3 m')}</>
    : isActive2D
      ? `${t('Active tool')} (${pointsPlaced}/${executionAreaTool.totalPoints}). ${t('Click again to cancel')}`
      : t('Click 4 areas to create an execution area')

  const buttonClass = isActive
    ? 'btn btn-sm btn-danger mb-2 d-flex flex-column justify-content-center align-items-center'
    : 'btn btn-sm btn-outline-danger mb-2 d-flex flex-column justify-content-center align-items-center'

  return (
    <AppTooltip content={tooltipText} place="bottom">
      <button
        className={buttonClass}
        style={{ height: 64, width: 64 }}
        onClick={handleClick}
        disabled={!engineReady}
        aria-pressed={isActive}
      >
        <span style={{ fontSize: 9, lineHeight: 1.1 }}>
          {isActive3D ? t('Active') : t('Create')}
        </span>
        <CodeSquare className="my-1" size={20} />
        <span style={{ fontSize: 9, lineHeight: 1.1 }}>
          {isActive3D ? '? ON' : isActive && !is3D ? `${pointsLeft} ${t('remaining')}` : 'Trigger'}
        </span>
      </button>
    </AppTooltip>
  )
}
