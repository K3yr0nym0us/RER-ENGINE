import { useEffect } from 'react'

import { CodeSquare } from 'react-bootstrap-icons'

import AppTooltip from '../../../../../../components/AppTooltip'
import { usePointDrawing } from '../../../../../../hooks/usePointDrawing'
import { useContextEngine } from '@engine'

interface Props {
  activeTool: 'draw_collider' | 'draw_execution_area' | null
  setActiveTool: (tool: 'draw_collider' | 'draw_execution_area' | null) => void
}

export function ExecutionAreaToolButton({ activeTool, setActiveTool }: Props) {
  const { engineReady, send, toolProgress } = useContextEngine()
  const executionAreaTool = usePointDrawing('draw_execution_area', 4, send, toolProgress)
  const isActive = activeTool === 'draw_execution_area'
  const pointsPlaced = isActive ? executionAreaTool.progress : 0
  const pointsLeft = Math.max(0, executionAreaTool.totalPoints - pointsPlaced)

  useEffect(() => {
    if (!isActive) return
    if (!executionAreaTool.isActive) {
      setActiveTool(null)
    }
  }, [isActive, executionAreaTool.isActive, setActiveTool])

  const tooltipText = isActive
    ? `Herramienta activa (${pointsPlaced}/${executionAreaTool.totalPoints}). Click de nuevo para cancelar`
    : 'Haz click en 4 zonas del motor para crear un cuadro de ejecución'

  const buttonClass = isActive
    ? 'btn btn-sm btn-danger mb-2 d-flex flex-column justify-content-center align-items-center'
    : 'btn btn-sm btn-outline-danger mb-2 d-flex flex-column justify-content-center align-items-center'

  const handleClick = () => {
    if (isActive) {
      executionAreaTool.cancel()
      setActiveTool(null)
      return
    }

    setActiveTool('draw_execution_area')
    executionAreaTool.start()
  }

  return (
    <AppTooltip content={tooltipText} place="bottom">
      <button
        className={buttonClass}
        style={{ height: 64, width: 64 }}
        onClick={handleClick}
        disabled={!engineReady}
        aria-pressed={isActive}
      >
        <span style={{ fontSize: 9, lineHeight: 1.1 }}>Crear</span>
        <CodeSquare className="my-1" size={20} />
        <span style={{ fontSize: 9, lineHeight: 1.1 }}>
          {isActive ? `${pointsLeft} faltan` : 'Trigger'}
        </span>
      </button>
    </AppTooltip>
  )
}
