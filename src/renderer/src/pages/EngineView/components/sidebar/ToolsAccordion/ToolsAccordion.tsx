import { Accordion } from 'react-bootstrap';

import { Bricks } from 'react-bootstrap-icons';

import { usePointDrawing } from '../../../../../hooks/usePointDrawing';

import { useContextEngine } from '@engine';

export function ToolsAccordion() {
  const { engineReady, send, toolProgress } = useContextEngine()
  const colliderTool = usePointDrawing('draw_collider', 4, send, toolProgress)

  return (
    <Accordion.Item eventKey="herramientas">
      <Accordion.Header>Herramientas</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <button
          className="btn btn-sm btn-outline-info mb-2"
          style={{ height: 50, width: 50 }}
          onClick={colliderTool.start}
          disabled={!engineReady}
          title="Haz click en 4 zonas del motor para crear un cuadro de colisiones"
        >
          <Bricks size={24} />
        </button>
      </Accordion.Body>
    </Accordion.Item>
  )
}

export default ToolsAccordion