import { Accordion } from 'react-bootstrap'
import { usePointDrawing } from '../../hooks/usePointDrawing'
import { ColliderPanel } from '../../2D/components/ColliderPanel'
import { useContextEngine } from '../../context/useContextEngine'

export function ToolsAccordion() {
  const { engineReady, send, toolProgress, colliderEntities, removeCollider } = useContextEngine()
  const colliderTool = usePointDrawing('draw_collider', 4, send, toolProgress)

  return (
    <Accordion.Item eventKey="herramientas">
      <Accordion.Header>Herramientas</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        {!colliderTool.isActive ? (
          <button
            className="btn btn-sm btn-outline-info w-100 mb-2"
            onClick={colliderTool.start}
            disabled={!engineReady}
            title="Click 4 veces en el viewport para definir un cuadro de colisiones"
          >
            ⬡ Cuadro de colisiones
          </button>
        ) : (
          <div className="mb-2">
            <div className="text-info small mb-1">
              Cuadro de colisiones — {colliderTool.progress}/{colliderTool.totalPoints} puntos colocados
            </div>
            <div className="d-flex gap-1 mb-2">
              {[...Array(colliderTool.totalPoints)].map((_, i) => (
                <div
                  key={i}
                  style={{
                    width:           12,
                    height:          12,
                    borderRadius:    '50%',
                    border:          '1px solid var(--bs-info)',
                    backgroundColor: i < colliderTool.progress ? 'var(--bs-info)' : 'transparent',
                  }}
                />
              ))}
            </div>
            <button
              className="btn btn-sm btn-outline-danger w-100"
              onClick={colliderTool.cancel}
            >
              Cancelar
            </button>
          </div>
        )}
        {colliderEntities.length > 0 && (
          <div className="mt-1">
            <div className="text-muted small mb-1">Colisionadores ({colliderEntities.length})</div>
            <ColliderPanel
              entries={colliderEntities}
              onRemove={removeCollider}
              config={{ addBtnLabel: '', emptyText: 'Sin colisionadores' }}
              highlightId={null}
            />
          </div>
        )}
      </Accordion.Body>
    </Accordion.Item>
  )
}

export default ToolsAccordion