import { useEffect, useRef } from 'react'
import { Accordion } from 'react-bootstrap'
import { useEngine } from '../hooks/useEngine'
import { SceneTree } from './SceneTree'
import { PropertiesPanel } from './PropertiesPanel'
import { ScenarioPanel } from '../2D/components/ScenarioPanel'
import type { ProjectType } from '../../../shared-types/types'

export function EngineView({ projectType }: { projectType: ProjectType }) {
  const logRef      = useRef<HTMLDivElement>(null)
  const viewportRef = useRef<HTMLDivElement>(null)

  const {
    engineReady, engineError, log, entities, selectedEntity,
    loadModel, send, retryEngine,
  } = useEngine(viewportRef, projectType)

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight
  }, [log])

  const statusBadge = engineReady
    ? <span className="badge bg-success">◉</span>
    : engineError
      ? <span className="badge bg-danger">Error</span>
      : <span className="badge bg-warning text-dark">Iniciando…</span>

  const typeBadgeClass = `engine-type-badge engine-type-badge--${projectType === '3D' ? '3d' : '2d'}`

  return (
    <div className="app-shell d-flex flex-column">
      <div className="d-flex flex-grow-1 overflow-hidden">

        {/* ── Sidebar ─────────────────────────────────────────────────────── */}
        <aside className="app-sidebar p-3 border-end border-secondary-subtle overflow-auto">
          {/* Logo + estado */}
          <div className="d-flex align-items-center justify-content-between mb-1">
            <span style={{ fontSize: 16, fontWeight: 700, color: '#c084fc', letterSpacing: '0.03em' }}>⬡ RER-ENGINE</span>
            <div className="d-flex align-items-center gap-2">
              <span className={typeBadgeClass}>{projectType}</span>
              {statusBadge}
            </div>
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

            {projectType === '2D' && (
              <ScenarioPanel engineReady={engineReady} send={send} />
            )}

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
          : log.map((entry) => (
            <div key={entry.id} className={entry.isError ? 'text-danger' : 'text-success'}>
              {entry.text}
            </div>
          ))
        }
      </div>
    </div>
  )
}
