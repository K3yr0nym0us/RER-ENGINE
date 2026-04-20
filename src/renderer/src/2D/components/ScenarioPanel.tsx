import { useState } from 'react'
import { Accordion } from 'react-bootstrap'
import { Files, Trash, Map } from 'react-bootstrap-icons'
import type { ScenarioEntry } from '../../hooks/useEngine'

interface Props {
  engineReady:      boolean
  send:             (cmd: object) => void
  scenarioEntities: ScenarioEntry[]
  onRemove:         (id: number) => void
  onDuplicate:      (id: number) => void
}

export function ScenarioPanel({ engineReady, send, scenarioEntities, onRemove, onDuplicate }: Props) {
  const [activeId, setActiveId] = useState<number | null>(null)
  const [scales,   setScales]   = useState<Record<number, number>>({})

  const getScale = (id: number) => scales[id] ?? 1.0

  const handleLoadScenario = () => {
    window.electronAPI.openScenarioDialog().then((p: string | null) => {
      if (!p) return
      send({ cmd: 'load_scenario', path: p })
    })
  }

  const handleScaleChange = (id: number, value: number) => {
    setScales((prev) => ({ ...prev, [id]: value }))
    send({ cmd: 'set_scenario_scale', id, scale: value })
  }

  const handleRemove = (id: number) => {
    if (activeId === id) setActiveId(null)
    setScales((prev) => { const next = { ...prev }; delete next[id]; return next })
    onRemove(id)
  }

  const scenarioLabel = (path: string) => path.split('/').pop() ?? path

  return (
    <Accordion.Item eventKey="escenarios">
      <Accordion.Header>Escenarios</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <button
          className="btn btn-outline-info btn-sm w-100 fw-bold mb-2"
          disabled={!engineReady}
          onClick={handleLoadScenario}
        >
          + Agregar escenario (PNG)
        </button>

        {scenarioEntities.length === 0 ? (
          <p className="text-secondary fst-italic small mb-0 px-1">Sin escenarios cargados</p>
        ) : (
          <ul className="list-unstyled mb-0">
            {scenarioEntities.map(({ id, path }) => (
              <li key={id} className="mb-1">
                {/* Cabecera de la entrada: nombre + botones */}
                <div className="d-flex align-items-center gap-1">
                  <button
                    className={`btn btn-sm flex-fill text-start text-truncate scenario-btn ${activeId === id ? 'btn-info text-dark fw-bold' : 'btn-outline-secondary'}`}
                    title={path}
                    onClick={() => setActiveId((prev) => prev === id ? null : id)}
                  >
                    <Map className="me-1" />{scenarioLabel(path)}
                  </button>
                  <button
                    className="btn btn-sm btn-outline-secondary"
                    title="Duplicar escenario"
                    onClick={() => onDuplicate(id)}
                  ><Files /></button>
                  <button
                    className="btn btn-sm btn-outline-danger"
                    title="Quitar escenario"
                    onClick={() => handleRemove(id)}
                  ><Trash /></button>
                </div>

                {/* Panel de escala (se expande al seleccionar) */}
                {activeId === id && (
                  <div className="mt-1 px-1">
                    <label className="form-label small text-secondary mb-1 d-flex justify-content-between">
                      <span>Escala</span>
                      <span className="text-info fw-bold">{getScale(id).toFixed(2)}×</span>
                    </label>
                    <input
                      type="range"
                      className="form-range"
                      min={0.05}
                      max={5}
                      step={0.05}
                      value={getScale(id)}
                      onChange={(e) => handleScaleChange(id, parseFloat(e.target.value))}
                    />
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
      </Accordion.Body>
    </Accordion.Item>
  )
}

