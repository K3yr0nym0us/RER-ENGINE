import { useEffect, useRef, useState, useCallback } from 'react';

import { Accordion } from 'react-bootstrap';
import { FloppyFill, ClockFill } from 'react-bootstrap-icons';
import { useAutoSave } from '../hooks/useAutoSave';
import { PropertiesPanel } from './PropertiesPanel';
import { AssetGroupPanel, type AssetGroupConfig } from '../2D/components/ScenarioPanel';
import { WorldPanel } from '../2D/components/WorldPanel';

import { useEngine } from '../hooks/useEngine';

import type { ProjectType, ProjectSaveData } from '../../../shared-types/types';

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

export function EngineView({ projectType, initialSave }: { projectType: ProjectType; initialSave?: ProjectSaveData | null }) {
  const logRef         = useRef<HTMLDivElement>(null)
  const viewportRef    = useRef<HTMLDivElement>(null)
  const [hasSavedOnce, setHasSavedOnce] = useState(false)
  const lastSavePath   = useRef<string | null>(null)

  const {
    engineReady, engineError, log, entities, selectedEntity, hoveredEntityId,
    backgroundPath,
    scenarioEntities, removeScenario, duplicateScenario,
    characterEntities, removeCharacter, duplicateCharacter,
    worldConfig, setWorldSize, setGridVisible, setGridCellSize,
    entityTransformsRef, playerEntityIdRef, camera2dRef,
    loadModel, send, retryEngine,
  } = useEngine(viewportRef, projectType, initialSave)

  const loadBackground = () => {
    window.electronAPI.openBackgroundDialog().then((p: string | null) => {
      if (p) send({ cmd: 'load_background', path: p })
    })
  }

  // Si se cargó desde un proyecto guardado, marcar como ya guardado
  useEffect(() => {
    if (initialSave) setHasSavedOnce(true)
  }, [initialSave])

  const buildSaveData = useCallback((): ProjectSaveData => {
    const transforms = entityTransformsRef.current
    const DEFAULT_POS: [number,number,number]         = [0, 0, 0]
    const DEFAULT_ROT: [number,number,number,number]  = [0, 0, 0, 1]
    const DEFAULT_SCL: [number,number,number]         = [1, 1, 1]
    const allEntities: ProjectSaveData['entities'] = [
      ...scenarioEntities.map((e) => ({
        id:       e.id,
        path:     e.path,
        kind:     'scenario' as const,
        position: transforms[e.id]?.position ?? DEFAULT_POS,
        rotation: transforms[e.id]?.rotation ?? DEFAULT_ROT,
        scale:    transforms[e.id]?.scale    ?? DEFAULT_SCL,
      })),
      ...characterEntities
        .filter((e) => !(e.path === '[Player]' && e.id === playerEntityIdRef.current))
        .map((e) => ({
        id:       e.id,
        path:     e.path,
        kind:     'character' as const,
        position: transforms[e.id]?.position ?? DEFAULT_POS,
        rotation: transforms[e.id]?.rotation ?? DEFAULT_ROT,
        scale:    transforms[e.id]?.scale    ?? DEFAULT_SCL,
      })),
    ]
    const playerId = playerEntityIdRef.current
    const playerTransform = playerId !== null
      ? {
          position: transforms[playerId]?.position ?? DEFAULT_POS,
          scale:    transforms[playerId]?.scale    ?? DEFAULT_SCL,
        }
      : null
    return {
      version:         1,
      type:            projectType,
      gameStyle:       (initialSave?.gameStyle ?? 'side-scroller'),
      world:           worldConfig,
      backgroundPath:  backgroundPath ?? null,
      entities:        allEntities,
      playerTransform,
      camera2d:        camera2dRef.current,
      savedAt:         new Date().toISOString(),
    }
  }, [projectType, initialSave, scenarioEntities, characterEntities, worldConfig, backgroundPath, entityTransformsRef, playerEntityIdRef, camera2dRef])

  const handleSave = useCallback(async () => {
    const data = buildSaveData()
    if (lastSavePath.current) {
      // Ya se guardó antes — usar ruta conocida sin dialog
      await window.electronAPI.saveProjectSilent(lastSavePath.current, data)
      setHasSavedOnce(true)
      return
    }
    // Primera vez: mostrar dialog para elegir ruta
    const ok = await window.electronAPI.saveProject(data)
    if (ok) {
      setHasSavedOnce(true)
      // Guardar la ruta elegida para auto-save (el main la conoce; la inferimos
      // en el próximo save mostrando de nuevo el dialog si se recarga la app)
    }
  }, [buildSaveData])

  const { autoSaveEnabled, toggleAutoSave } = useAutoSave({
    onSave: handleSave,
    hasSavedOnce,
  })

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

          {/* ── Guardar proyecto ──────────────────────────────────────────── */}
          <div className="d-flex gap-2 mb-2 align-items-center">
            <button
              className="btn btn-sm btn-outline-light flex-fill d-flex align-items-center justify-content-center gap-2"
              title="Guardar proyecto"
              disabled={!engineReady}
              onClick={handleSave}
            >
              <FloppyFill size={13} />
              <span style={{ fontSize: 11 }}>Guardar</span>
            </button>
            <button
              className={`btn btn-sm d-flex align-items-center gap-1 ${
                autoSaveEnabled ? 'btn-warning text-dark' : 'btn-outline-secondary'
              }`}
              title={hasSavedOnce ? (autoSaveEnabled ? 'Desactivar auto-guardado (cada 5 min)' : 'Activar auto-guardado (cada 5 min)') : 'Guarda el proyecto al menos una vez para activar'}
              disabled={!hasSavedOnce || !engineReady}
              onClick={toggleAutoSave}
              style={{ whiteSpace: 'nowrap' }}
            >
              <ClockFill size={11} />
              <span style={{ fontSize: 10 }}>Auto</span>
            </button>
          </div>

          <Accordion defaultActiveKey="mundo" className="sidebar-accordion">

            {projectType === '2D' && (
              <WorldPanel
                engineReady={engineReady}
                worldConfig={worldConfig}
                backgroundPath={backgroundPath ?? null}
                onWorldSize={setWorldSize}
                onGridVisible={setGridVisible}
                onCellSize={setGridCellSize}
                onLoadBackground={loadBackground}
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
                          highlightId={hoveredEntityId ?? selectedEntity?.id ?? null}
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
                          highlightId={hoveredEntityId ?? selectedEntity?.id ?? null}
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
