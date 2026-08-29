import { useRef } from 'react';

import SidebarLeft from './components/SidebarLeft';
import SidebarRight from './components/SidebarRight';
import LogConsole from './components/LogConsole';

import TopBarEngine from './components/TopBarEngine';
import { QuickBuildOverlay } from './components/QuickBuildOverlay';
import { SceneImportLoadingOverlay } from './components/SceneImportLoadingOverlay';
import { EngineGpuErrorOverlay } from './components/EngineGpuErrorOverlay';

import { EngineProvider } from '@engine';
import { ModalProvider } from '@modal';
import { QuickBuildProvider } from '../../context/QuickBuildContext';
import { PlaneToolProvider } from '../../context/PlaneToolContext';
import { useAutoSave } from '@hooks';
import { useEntityPropertiesModal } from './hooks/useEntityPropertiesModal';
import { SceneManagerProvider } from './hooks/useSceneManager';
import { SidebarAccordionProvider } from '../../context/SidebarAccordionContext';
import { useAiAssistantOverlaySync } from '../../plugins/useAiAssistantOverlaySync';

import type { GameStyle, ProjectType } from '@shared-types';

export function EngineView({
  projectType,
  gameStyle,
  initialSavePath,
  initialExtractDir,
  onGameStyleChange,
}: {
  projectType: ProjectType
  gameStyle?: GameStyle
  initialSavePath?: string | null
  initialExtractDir?: string | null
  onGameStyleChange?: (mode: GameStyle) => void
}) {
  const viewportRef = useRef<HTMLDivElement>(null)

  return (
    <EngineProvider 
      viewportRef={viewportRef} 
      projectType={projectType} 
      gameStyle={gameStyle} 
      initialSavePath={initialSavePath}
      initialExtractDir={initialExtractDir}
    >
      <QuickBuildProvider>
        <PlaneToolProvider>
        <ModalProvider>
          <EngineViewInner 
            projectType={projectType} 
            gameStyle={gameStyle} 
            initialSavePath={initialSavePath}
            initialExtractDir={initialExtractDir}
            onGameStyleChange={onGameStyleChange}
            viewportRef={viewportRef} 
          />
        </ModalProvider>
        </PlaneToolProvider>
      </QuickBuildProvider>
    </EngineProvider>
  )
}

function EngineViewInner({ projectType, gameStyle, initialSavePath, initialExtractDir, onGameStyleChange, viewportRef }: {
  projectType: ProjectType
  gameStyle?: GameStyle
  initialSavePath?: string | null
  initialExtractDir?: string | null
  onGameStyleChange?: (mode: GameStyle) => void
  viewportRef: React.RefObject<HTMLDivElement>
}) {
  const { 
    handleSave, 
    toggleAutoSave, 
    hasSavedOnce, 
    autoSaveEnabled,
    savingProject,
  } = useAutoSave({ projectType, gameStyle, initialSavePath, initialExtractDir })

  // En play el input lo gestiona la ventana winit del motor (overlay), no el renderer.
  useEntityPropertiesModal()
  useAiAssistantOverlaySync()

  return (
    <SceneManagerProvider
      initialSavePath={initialSavePath}
      initialExtractDir={initialExtractDir}
      projectType={projectType}
      gameStyle={gameStyle}
      onSaveProject={handleSave}
    >
      <SidebarAccordionProvider>
        <div className="app-shell d-flex flex-column">
          <div className="d-flex flex-grow-1 overflow-hidden">
            <SidebarLeft
              projectType={projectType}
              gameStyle={gameStyle}
              initialSavePath={initialSavePath}
              initialExtractDir={initialExtractDir}
              onGameStyleChange={onGameStyleChange}
            />

            <div className="d-flex flex-column flex-fill" style={{ width: '60% !important' }}>
              <TopBarEngine 
                projectType={projectType}
                handleSave={handleSave}
                toggleAutoSave={toggleAutoSave}
                hasSavedOnce={hasSavedOnce}
                autoSaveEnabled={autoSaveEnabled}
                savingProject={savingProject}
              />

              <main
                className="flex-fill position-relative overflow-hidden engine-viewport-area"
                ref={viewportRef}
                style={{ background: 'var(--bs-body-bg)', marginTop: 0 }}
              >
                <EngineGpuErrorOverlay />
                <QuickBuildOverlay viewportRef={viewportRef} />
                <SceneImportLoadingOverlay />
              </main>

              <LogConsole />
            </div>

            <SidebarRight projectType={projectType} />
          </div>
        </div>
      </SidebarAccordionProvider>
    </SceneManagerProvider>
  )
}
