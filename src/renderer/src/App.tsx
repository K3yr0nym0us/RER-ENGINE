import { useEffect, useRef, useState, useCallback } from 'react'
import { Accordion } from 'react-bootstrap'
import { useEngine } from './useEngine'
import type { SelectedEntity } from './useEngine'
import { SceneTree } from './SceneTree'

// ── PropertiesPanel ──────────────────────────────────────────────────────────
function PropertiesPanel({
  entity,
  onSend,
}: {
  entity: SelectedEntity | null
  onSend: (cmd: object) => void
}) {
  const [pos, setPos] = useState<[string, string, string]>(['0', '0', '0'])
  const [rot, setRot] = useState<[string, string, string, string]>(['0', '0', '0', '1'])
  const [scl, setScl] = useState<[string, string, string]>(['1', '1', '1'])

  useEffect(() => {
    if (!entity) return
    setPos(entity.position.map((n) => n.toFixed(1)) as [string, string, string])
    setRot(entity.rotation.map((n) => n.toFixed(1)) as [string, string, string, string])
    setScl(entity.scale.map((n) => n.toFixed(1)) as [string, string, string])
  }, [entity?.id, entity])

  const commit = useCallback((
    overridePos?: [string, string, string],
    overrideRot?: [string, string, string, string],
    overrideScl?: [string, string, string],
  ) => {
    if (!entity) return
    const p = (overridePos ?? pos).map(Number) as [number, number, number]
    const r = (overrideRot ?? rot).map(Number) as [number, number, number, number]
    const s = (overrideScl ?? scl).map(Number) as [number, number, number]
    onSend({ cmd: 'set_transform', id: entity.id, position: p, rotation: r, scale: s })
  }, [entity, pos, rot, scl, onSend])

  if (!entity) {
    return <p className="text-secondary fst-italic small mb-0 px-1">Haz click en un objeto para verlo</p>
  }

  const axisColors = ['text-danger', 'text-success', 'text-info']

  const makeVec3Row = (
    label: string,
    vals: [string, string, string],
    setter: (v: [string, string, string]) => void,
    override: (next: [string, string, string]) => void,
    step = '0.1',
  ) => (
    <div className="mb-2">
      <label className="text-uppercase fw-bold" style={{ fontSize: 10, letterSpacing: '0.07em', color: '#d8d4f8' }}>{label}</label>
      <div className="d-flex gap-1 mt-1">
        {(['X', 'Y', 'Z'] as const).map((ax, i) => (
          <div key={ax} className="flex-fill">
            <div className={`text-center ${axisColors[i]}`} style={{ fontSize: 9, fontWeight: 600 }}>{ax}</div>
            <input
              type="number"
              step={step}
              value={vals[i]}
              className="form-control form-control-sm text-center bg-dark text-light border-secondary"
              style={{ fontSize: 11, padding: '2px 4px' }}
              onChange={(e) => {
                const next = [...vals] as [string, string, string]
                next[i] = e.target.value
                setter(next)
                override(next)
              }}
            />
          </div>
        ))}
      </div>
    </div>
  )

  return (
    <div className="px-1">
      <div className="mb-2">
        <label className="text-uppercase fw-bold" style={{ fontSize: 10, letterSpacing: '0.07em', color: '#d8d4f8' }}>Nombre</label>
        <div className="form-control form-control-sm bg-dark text-info border-secondary mt-1" style={{ fontSize: 11 }}>
          {entity.name}
        </div>
      </div>
      {makeVec3Row('Posición', pos, setPos, (n) => commit(n, undefined, undefined))}
      {makeVec3Row('Escala',   scl, setScl, (n) => commit(undefined, undefined, n))}
      <div className="mb-2">
        <label className="text-uppercase fw-bold" style={{ fontSize: 10, letterSpacing: '0.07em', color: '#d8d4f8' }}>Rotación (xyzw)</label>
        <div className="d-flex gap-1 mt-1">
          {(['X', 'Y', 'Z', 'W'] as const).map((ax, i) => (
            <div key={ax} className="flex-fill">
              <div className={`text-center ${i < 3 ? axisColors[i] : 'text-purple'}`} style={{ fontSize: 9, fontWeight: 600, color: i === 3 ? '#a78bfa' : undefined }}>{ax}</div>
              <input
                type="number"
                step="0.01"
                value={rot[i]}
                className="form-control form-control-sm text-center bg-dark text-light border-secondary"
                style={{ fontSize: 11, padding: '2px 4px' }}
                onChange={(e) => {
                  const next = [...rot] as [string, string, string, string]
                  next[i] = e.target.value
                  setRot(next)
                  commit(undefined, next, undefined)
                }}
              />
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

// ── Componente principal ─────────────────────────────────────────────────────
export default function App() {
  const logRef      = useRef<HTMLDivElement>(null)
  const viewportRef = useRef<HTMLDivElement>(null)

  const {
    engineReady, engineError, log, entities, selectedEntity,
    loadModel, send, retryEngine,
  } = useEngine(viewportRef)

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight
  }, [log])

  const statusBadge = engineReady
    ? <span className="badge bg-success">Conectado</span>
    : engineError
      ? <span className="badge bg-danger">Error</span>
      : <span className="badge bg-warning text-dark">Iniciando…</span>

  return (
    <div className="app-shell d-flex flex-column">
      <div className="d-flex flex-grow-1 overflow-hidden">

        {/* ── Sidebar ─────────────────────────────────────────────────────── */}
        <aside className="app-sidebar p-3 border-end border-secondary-subtle overflow-auto">
          {/* Logo + estado */}
          <div className="d-flex align-items-center justify-content-between mb-1">
            <span style={{ fontSize: 16, fontWeight: 700, color: '#c084fc', letterSpacing: '0.03em' }}>⬡ Oxide Engine</span>
            {statusBadge}
          </div>

          {engineError && (
            <div className="alert alert-danger py-1 px-2 small mb-0">{engineError}</div>
          )}

          <hr className="border-secondary my-1" />

          <Accordion defaultActiveKey="assets" className="sidebar-accordion">

            <Accordion.Item eventKey="assets">
              <Accordion.Header>Assets</Accordion.Header>
              <Accordion.Body className="py-2 px-2">
                <button
                  className="btn btn-outline-light btn-sm w-100 fw-bold"
                  disabled={!engineReady}
                  onClick={() =>
                    window.electronAPI.openModelDialog().then((p: string | null) => { if (p) loadModel(p) })
                  }
                >
                  Cargar modelo (.glb)
                </button>
              </Accordion.Body>
            </Accordion.Item>

            <Accordion.Item eventKey="escena">
              <Accordion.Header>Escena</Accordion.Header>
              <Accordion.Body className="py-1 px-1">
                <SceneTree entities={entities} selectedId={selectedEntity?.id} />
              </Accordion.Body>
            </Accordion.Item>

            <Accordion.Item eventKey="propiedades">
              <Accordion.Header>Propiedades</Accordion.Header>
              <Accordion.Body className="py-2 px-2">
                <PropertiesPanel entity={selectedEntity ?? null} onSend={send} />
              </Accordion.Body>
            </Accordion.Item>

          </Accordion>
        </aside>

        {/* ── Viewport ────────────────────────────────────────────────────── */}
        <main className="flex-fill position-relative overflow-hidden" ref={viewportRef} style={{ background: 'transparent' }}>
          {engineError && (
            <div className="position-absolute inset-0 d-flex flex-column align-items-center justify-content-center gap-3"
              style={{ inset: 0, background: 'rgba(5,5,12,0.93)', zIndex: 10 }}>
              <span style={{ fontSize: 32 }}>⚠</span>
              <p className="text-danger text-center small mb-0" style={{ maxWidth: 360 }}>{engineError}</p>
              <button className="btn btn-sm btn-primary" onClick={retryEngine}>Reintentar</button>
            </div>
          )}
          {!engineReady && !engineError && (
            <div className="position-absolute d-flex flex-column align-items-center justify-content-center gap-2 text-secondary"
              style={{ inset: 0, background: '#050508', userSelect: 'none' }}>
              <span style={{ fontSize: 36, opacity: 0.2 }}>⬡</span>
              <span className="small">Iniciando motor Rust…</span>
            </div>
          )}
        </main>
      </div>

      {/* ── Log ─────────────────────────────────────────────────────────────── */}
      <div
        ref={logRef}
        className="console-panel overflow-auto font-monospace px-3 py-2 small"
      >
        {log.length === 0
          ? <span className="text-secondary">Sin eventos aún…</span>
          : log.map((entry, i) => (
            <div key={i} className={entry.isError ? 'text-danger' : 'text-success'}>
              {entry.text}
            </div>
          ))
        }
      </div>
    </div>
  )
}

