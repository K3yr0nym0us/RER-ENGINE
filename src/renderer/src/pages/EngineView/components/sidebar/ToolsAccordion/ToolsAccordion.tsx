import { Accordion } from 'react-bootstrap';

import { Bricks, ExclamationDiamond } from 'react-bootstrap-icons';

import AppTooltip from '../../../../../components/AppTooltip';
import { usePointDrawing } from '../../../../../hooks/usePointDrawing';

import { useContextEngine } from '@engine';

export function ToolsAccordion() {
  const { engineReady, send, toolProgress } = useContextEngine()
  const colliderTool = usePointDrawing('draw_collider', 4, send, toolProgress)
  const executionAreaTool = usePointDrawing('draw_execution_area', 4, send, toolProgress)
  const isColliderActive = colliderTool.isActive
  const isExecutionAreaActive = executionAreaTool.isActive
  const pointsPlaced = colliderTool.progress
  const pointsLeft = Math.max(0, colliderTool.totalPoints - pointsPlaced)
  const executionPointsPlaced = executionAreaTool.progress
  const executionPointsLeft = Math.max(0, executionAreaTool.totalPoints - executionPointsPlaced)

  const tooltipText = isColliderActive
    ? `Herramienta activa (${pointsPlaced}/${colliderTool.totalPoints}). Click de nuevo para cancelar`
    : 'Haz click en 4 zonas del motor para crear un cuadro de colisiones'

  const buttonClass = isColliderActive
    ? 'btn btn-sm btn-info mb-2 d-flex flex-column justify-content-center align-items-center'
    : 'btn btn-sm btn-outline-info mb-2 d-flex flex-column justify-content-center align-items-center'

  const handleToggleColliderTool = () => {
    if (isColliderActive) {
      colliderTool.cancel()
      return
    }
    if (isExecutionAreaActive) {
      executionAreaTool.cancel()
    }
    colliderTool.start()
  }

  const handleToggleExecutionAreaTool = () => {
    if (isExecutionAreaActive) {
      executionAreaTool.cancel()
      return
    }
    if (isColliderActive) {
      colliderTool.cancel()
    }
    executionAreaTool.start()
  }

  return (
    <Accordion.Item eventKey="herramientas">
      <Accordion.Header>Herramientas</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <div className="d-flex gap-2 flex-wrap">
          <AppTooltip content={tooltipText} place="right">
            <button
              className={buttonClass}
              style={{ height: 64, width: 64 }}
              onClick={handleToggleColliderTool}
              disabled={!engineReady}
              aria-pressed={isColliderActive}
              title={isColliderActive ? 'Cancelar creación de colisionador' : 'Iniciar creación de colisionador'}
            >
              <Bricks size={24} />
              <span style={{ fontSize: 10, lineHeight: 1.1 }}>
                {isColliderActive ? `${pointsLeft} faltan` : 'Crear'}
              </span>
            </button>
          </AppTooltip>

          <AppTooltip
            content={isExecutionAreaActive
              ? `Herramienta activa (${executionPointsPlaced}/${executionAreaTool.totalPoints}). Click de nuevo para cancelar`
              : 'Haz click en 4 zonas del motor para crear un cuadro de ejecución'}
            place="right"
          >
            <button
              className={isExecutionAreaActive
                ? 'btn btn-sm btn-danger mb-2 d-flex flex-column justify-content-center align-items-center'
                : 'btn btn-sm btn-outline-danger mb-2 d-flex flex-column justify-content-center align-items-center'}
              style={{ height: 64, width: 64 }}
              onClick={handleToggleExecutionAreaTool}
              disabled={!engineReady}
              aria-pressed={isExecutionAreaActive}
              title={isExecutionAreaActive ? 'Cancelar creación de área de ejecución' : 'Iniciar creación de área de ejecución'}
            >
              <ExclamationDiamond size={24} />
              <span style={{ fontSize: 9, lineHeight: 1.1 }}>
                {isExecutionAreaActive ? `${executionPointsLeft} faltan` : 'Trigger'}
              </span>
            </button>
          </AppTooltip>
        </div>
      </Accordion.Body>
    </Accordion.Item>
  )
}

export default ToolsAccordion