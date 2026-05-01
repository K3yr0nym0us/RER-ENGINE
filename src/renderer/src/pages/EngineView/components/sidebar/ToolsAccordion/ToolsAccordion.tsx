import { Accordion } from 'react-bootstrap';

import { Bricks } from 'react-bootstrap-icons';

import AppTooltip from '../../../../../components/AppTooltip';
import { usePointDrawing } from '../../../../../hooks/usePointDrawing';

import { useContextEngine } from '@engine';

export function ToolsAccordion() {
  const { engineReady, send, toolProgress } = useContextEngine()
  const colliderTool = usePointDrawing('draw_collider', 4, send, toolProgress)
  const isColliderActive = colliderTool.isActive
  const pointsPlaced = colliderTool.progress
  const pointsLeft = Math.max(0, colliderTool.totalPoints - pointsPlaced)

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
    colliderTool.start()
  }

  return (
    <Accordion.Item eventKey="herramientas">
      <Accordion.Header>Herramientas</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
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
      </Accordion.Body>
    </Accordion.Item>
  )
}

export default ToolsAccordion