import { useEffect, useState } from 'react';
import { Accordion } from 'react-bootstrap';
import { useContextEngine } from '../context/useContextEngine';
import { TransformPanel, AnimationsPanel, ScriptingPanel } from './sidebar/properties';

export function PropertiesPanel({ projectType }: { projectType?: string }) {
  const { selectedEntity } = useContextEngine()
  const { engineReady, send, scenarioEntities, worldConfig, backgroundPath, entityMetaRef, entityTransformsRef } = useContextEngine()

  // Intercepta set_transform para mantener entityTransformsRef sincronizado
  // sin depender del evento entity_selected (que solo llega cuando el usuario clica la entidad).
  const handleSend = (cmd: any) => {
    if (cmd.cmd === 'set_transform' && selectedEntity) {
      entityTransformsRef.current[selectedEntity.id] = {
        position: cmd.position ?? selectedEntity.position,
        rotation: cmd.rotation ?? selectedEntity.rotation,
        scale:    cmd.scale    ?? selectedEntity.scale,
      }
    }
    send(cmd)
  }
  
  const is2D = projectType === '2D'
  const [physicsEnabled, setPhysicsEnabled] = useState(false)
  const [physicsType, setPhysicsType] = useState('dynamic')

  useEffect(() => {
    if (!selectedEntity) return
    setPhysicsEnabled(selectedEntity.physicsEnabled)
    setPhysicsType(selectedEntity.physicsType || 'dynamic')
  }, [selectedEntity?.id])

  if (!selectedEntity) {
    return <p className="text-secondary fst-italic small mb-0 px-1">Haz click en un objeto para verlo</p>
  }

  const isScenario = scenarioEntities.some((s: any) => s.id === selectedEntity?.id)

  return (
    <div>
      <div className="mb-2">
        <p className="prop-label">Nombre</p>
        <div className="form-control form-control-sm bg-dark text-info border-secondary mt-1 prop-input">
          {selectedEntity.name}
        </div>
      </div>

      {isScenario ? (
        <div className="mb-2">
          <p className="prop-label">Colisión</p>
          <div className="d-flex align-items-center gap-2 mt-1">
            <input
              type="checkbox"
              id="scenario-collision"
              className="form-check-input"
              checked={physicsEnabled}
              onChange={(e) => {
                const next = e.target.checked
                setPhysicsEnabled(next)
                send({ cmd: 'set_physics', id: selectedEntity.id, enabled: next, body_type: 'static' })
              }}
            />
            <label htmlFor="scenario-collision" className="form-check-label text-light small mb-0">
              Con colisión
            </label>
          </div>
        </div>
      ) : (
        <div className="mb-2">
          <p className="prop-label">Física</p>
          <div className="d-flex align-items-center gap-2 mt-1">
            <input
              type="checkbox"
              id="physics-enabled"
              className="form-check-input"
              checked={physicsEnabled}
              onChange={(e) => {
                const next = e.target.checked
                setPhysicsEnabled(next)
                send({ cmd: 'set_physics', id: selectedEntity.id, enabled: next, body_type: physicsType })
              }}
            />
            <label htmlFor="physics-enabled" className="form-check-label text-light small mb-0">
              Activar física
            </label>
          </div>
          {physicsEnabled && (
            <select
              value={physicsType}
              className="form-select form-select-sm bg-dark text-light border-secondary mt-1"
              onChange={(e) => {
                const next = e.target.value
                setPhysicsType(next)
                send({ cmd: 'set_physics', id: selectedEntity.id, enabled: true, body_type: next })
              }}
            >
              <option value="dynamic">Dinámico (gravedad)</option>
              <option value="static">Estático (no se mueve)</option>
              {!is2D && <option value="kinematic">Cinemático (por código)</option>}
            </select>
          )}
        </div>
      )}

      <Accordion className="prop-accordion">
        <Accordion.Item eventKey="transform">
          <Accordion.Header>Transformaciones</Accordion.Header>
          <Accordion.Body className="py-2 px-2">
            <TransformPanel entity={selectedEntity} is2D={is2D} onSend={handleSend} />
          </Accordion.Body>
        </Accordion.Item>

        <AnimationsPanel />

        <ScriptingPanel />
      </Accordion>
    </div>
  )
}

export default PropertiesPanel