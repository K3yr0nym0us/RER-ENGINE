import { useEffect, useRef, useState, useCallback } from 'react';

import SideBarLeft from './SideBarLeft';
import LogConsole from './LogConsole';
import TopBarEngine from './TopBarEngine';

import { EngineProvider, useContextEngine } from '../context/useContextEngine';
import { useAutoSave } from '../hooks/useAutoSave';

import type { ProjectType, ProjectSaveData } from '../../../shared-types/types';

export function EngineView({ projectType, initialSave }: { projectType: ProjectType; initialSave?: ProjectSaveData | null }) {
  const viewportRef    = useRef<HTMLDivElement>(null)
  const [hasSavedOnce, setHasSavedOnce] = useState(false)
  const lastSavePath   = useRef<string | null>(null)

  return (
    <EngineProvider viewportRef={viewportRef} projectType={projectType} initialSave={initialSave}>
      <EngineViewInner projectType={projectType} initialSave={initialSave} viewportRef={viewportRef} hasSavedOnce={hasSavedOnce} setHasSavedOnce={setHasSavedOnce} lastSavePath={lastSavePath} />
    </EngineProvider>
  )
}

function EngineViewInner({ projectType, initialSave, viewportRef, hasSavedOnce, setHasSavedOnce, lastSavePath }: {
  projectType: ProjectType
  initialSave?: ProjectSaveData | null
  viewportRef: React.RefObject<HTMLDivElement>
  hasSavedOnce: boolean
  setHasSavedOnce: (v: boolean) => void
  lastSavePath: React.MutableRefObject<string | null>
}) {
  const {
    engineReady, engineError, log, entities, selectedEntity, hoveredEntityId,
    backgroundPath,
    scenarioEntities, removeScenario, duplicateScenario,
    characterEntities, removeCharacter, duplicateCharacter,
    worldConfig, setWorldSize, setGridVisible, setGridCellSize,
    colliderEntities, removeCollider, toolProgress,
    entityTransformsRef, entityMetaRef, playerEntityIdRef, camera2dRef,
    loadModel, send, retryEngine,
  } = useContextEngine()


  useEffect(() => {
    if (initialSave) setHasSavedOnce(true)
  }, [initialSave])

  const buildSaveData = useCallback((): ProjectSaveData => {
    const transforms = entityTransformsRef.current
    const meta       = entityMetaRef.current
    const DEFAULT_POS: [number,number,number]        = [0, 0, 0]
    const DEFAULT_ROT: [number,number,number,number] = [0, 0, 0, 1]
    const DEFAULT_SCL: [number,number,number]        = [1, 1, 1]
    const playerId = playerEntityIdRef.current
    const allEntities: ProjectSaveData['entities'] = Object.entries(meta)
      .filter(([idStr, m]) =>
        !(m.kind === 'character' && m.path === '[Player]' && Number(idStr) === playerId)
      )
      .map(([idStr, m]) => {
        const id = Number(idStr)
        return {
          id,
          kind:            m.kind,
          path:            m.path,
          position:        transforms[id]?.position ?? DEFAULT_POS,
          rotation:        transforms[id]?.rotation ?? DEFAULT_ROT,
          scale:           transforms[id]?.scale    ?? DEFAULT_SCL,
          physics_enabled: m.physicsEnabled,
          physics_type:    m.physicsType,
          points:          m.points,
        }
      })
    const playerTransform = playerId !== null
      ? {
          position: transforms[playerId]?.position ?? DEFAULT_POS,
          scale:    transforms[playerId]?.scale    ?? DEFAULT_SCL,
        }
      : null
    return {
      version:        1,
      type:           projectType,
      gameStyle:      (initialSave?.gameStyle ?? 'side-scroller'),
      world:          worldConfig,
      backgroundPath: backgroundPath ?? null,
      entities:       allEntities,
      playerTransform,
      camera2d:       camera2dRef.current,
      savedAt:        new Date().toISOString(),
    }
  }, [projectType, initialSave, worldConfig, backgroundPath])

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

  return (
    <div className="app-shell d-flex flex-column">
      <div className="d-flex flex-grow-1 overflow-hidden">

        <SideBarLeft 
          projectType={projectType}
          engineReady={engineReady}
          send={send}
          scenarioEntities={scenarioEntities}
          removeScenario={removeScenario}
          duplicateScenario={duplicateScenario}
          characterEntities={characterEntities}
          removeCharacter={removeCharacter}
          duplicateCharacter={duplicateCharacter}
          backgroundPath={backgroundPath}
          worldConfig={worldConfig}
          setWorldSize={(width, height) => setWorldSize(width, height)}
          setGridVisible={(visible) => setGridVisible(visible)}
          setGridCellSize={(size) => setGridCellSize(size)}
          toolProgress={toolProgress}
          colliderEntities={colliderEntities}
          removeCollider={removeCollider}
          loadModel={loadModel}
          hoveredEntityId={hoveredEntityId}
          selectedEntity={selectedEntity}
        />

        <div className="d-flex flex-column flex-fill">
          <TopBarEngine 
            engineError={engineError}
            autoSaveEnabled={autoSaveEnabled}
            hasSavedOnce={hasSavedOnce}
            engineReady={engineReady}
            projectType={projectType}
            handleSave={handleSave}
            toggleAutoSave={toggleAutoSave}
          />

          <main
            className="flex-fill position-relative overflow-hidden engine-viewport-area"
            ref={viewportRef}
            style={{ background: 'transparent', marginTop: 0 }}
          >
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


          <LogConsole log={log} />
        </div>
      </div>
    </div>
  )
}
