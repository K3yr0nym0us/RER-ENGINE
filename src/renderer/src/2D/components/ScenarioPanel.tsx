import { useState } from 'react'
import { Accordion } from 'react-bootstrap'

interface Props {
  engineReady: boolean
  send:        (cmd: object) => void
}

export function ScenarioPanel({ engineReady, send }: Props) {
  const [scenarios,     setScenarios]     = useState<string[]>([])
  const [activeScenario, setActiveScenario] = useState<string | null>(null)

  const applyScenario = (path: string) => {
    setActiveScenario(path)
    send({ cmd: 'load_scenario', path })
  }

  const handleLoadScenario = () => {
    window.electronAPI.openScenarioDialog().then((p: string | null) => {
      if (!p) return
      setScenarios((prev) => prev.includes(p) ? prev : [...prev, p])
      applyScenario(p)
    })
  }

  const scenarioLabel = (p: string) => p.split('/').pop() ?? p

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
        {scenarios.length === 0 ? (
          <p className="text-secondary fst-italic small mb-0 px-1">Sin escenarios cargados</p>
        ) : (
          <ul className="list-unstyled mb-0">
            {scenarios.map((p) => (
              <li key={p}>
                <button
                  className={`btn btn-sm w-100 text-start text-truncate scenario-btn ${activeScenario === p ? 'btn-info text-dark fw-bold' : 'btn-outline-secondary'}`}
                  title={p}
                  onClick={() => applyScenario(p)}
                >
                  🗺 {scenarioLabel(p)}
                </button>
              </li>
            ))}
          </ul>
        )}
      </Accordion.Body>
    </Accordion.Item>
  )
}
