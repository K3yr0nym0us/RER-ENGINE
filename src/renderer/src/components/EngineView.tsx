import { useEffect, useRef } from 'react';

import { Accordion } from 'react-bootstrap';
import { PropertiesPanel } from './PropertiesPanel';
import { AssetGroupPanel, type AssetGroupConfig } from '../2D/components/ScenarioPanel';
import { WorldPanel } from '../2D/components/WorldPanel';

import { useEngine } from '../hooks/useEngine';

import type { ProjectType } from '../../../shared-types/types';

const SCENARIO_CONFIG: AssetGroupConfig = {
  openDialog:  () => window.electronAPI.openScenarioDialog(),
  loadCmd:     'load_scenario',
  dupCmd:      'duplicate_scenario',
  addBtnLabel: '+ Agregar escenario (PNG)',
  emptyText:   'Sin escenarios cargados',
}

const CHARACTER_CONFIG: AssetGroupConfig = {
  openDialog:  () => window.electronAPI.openCharacterDialog(),
  loadCmd:     'load_character',
  dupCmd:      'duplicate_character',
  addBtnLabel: '+ Agregar personaje (PNG)',
  emptyText:   'Sin personajes cargados',
}

export function EngineView({ projectType }: { projectType: ProjectType }) {
  const logRef      = useRef<HTMLDivElement>(null)
  const viewportRef = useRef<HTMLDivElement>(null)

  const {
    engineReady, engineError, log, entities, selectedEntity,
    scenarioEntities, removeScenario, duplicateScenario,
    characterEntities, removeCharacter, duplicateCharacter,
    worldConfig, setWorldSize, setGridVisible, setGridCellSize,
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

          <Accordion defaultActiveKey="mundo" className="sidebar-accordion">

            {projectType === '2D' && (
              <WorldPanel
                engineReady={engineReady}
                worldConfig={worldConfig}
                onWorldSize={setWorldSize}
                onGridVisible={setGridVisible}
                onCellSize={setGridCellSize}
              />
            )}

            <Accordion.Item eventKey="assets">
              <Accordion.Header>Assets</Accordion.Header>
              <Accordion.Body className="py-2 px-2">
                <Accordion>
                  <Accordion.Item eventKey="escenarios">
                    <Accordion.Header>Escenarios</Accordion.Header>
                    <Accordion.Body className="py-2 px-2">
                      {projectType === '3D' && (
                        <button
                          className="btn btn-outline-light btn-sm w-100 fw-bold"
                          disabled={!engineReady}
                          onClick={() =>
                            window.electronAPI.openModelDialog().then((p: string | null) => { if (p) loadModel(p) })
                          }
                        >
                          Cargar modelo (.glb)
                        </button>
                      )}
                      {projectType === '2D' && (
                        <AssetGroupPanel
                          engineReady={engineReady}
                          send={send}
                          entries={scenarioEntities}
                          onRemove={removeScenario}
                          onDuplicate={duplicateScenario}
                          config={SCENARIO_CONFIG}
                        />
                      )}
                    </Accordion.Body>
                  </Accordion.Item>
                  <Accordion.Item eventKey="personajes">
                    <Accordion.Header>Personajes</Accordion.Header>
                    <Accordion.Body className="py-2 px-2">
                      {projectType === '2D' && (
                        <AssetGroupPanel
                          engineReady={engineReady}
                          send={send}
                          entries={characterEntities}
                          onRemove={removeCharacter}
                          onDuplicate={duplicateCharacter}
                          config={CHARACTER_CONFIG}
                        />
                      )}
                    </Accordion.Body>
                  </Accordion.Item>
                </Accordion>
              </Accordion.Body>
            </Accordion.Item>
          </Accordion>

          {selectedEntity && (
            <div className="pt-4">
              <b className="ms-2">Elemento seleccionado:</b>
              <Accordion defaultActiveKey="propiedades" className="sidebar-accordion mt-1">
                <Accordion.Item eventKey="propiedades">
                  <Accordion.Header>Propiedades</Accordion.Header>
                  <Accordion.Body className="py-2 px-2">
                    <PropertiesPanel entity={selectedEntity ?? null} onSend={send} projectType={projectType} />
                  </Accordion.Body>
                </Accordion.Item>
              </Accordion>
            </div>
          )}
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
