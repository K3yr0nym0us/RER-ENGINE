import { useRef } from 'react';

import SideBarLeft from './components/SideBarLeft';
import LogConsole from './components/LogConsole';
import MetricsPanel from './components/MetricsPanel';
import TopBarEngine from './components/TopBarEngine';
import { QuickBuildOverlay } from './components/QuickBuildOverlay';
import { SceneImportLoadingOverlay } from './components/SceneImportLoadingOverlay';
import { EngineGpuErrorOverlay } from './components/EngineGpuErrorOverlay';

import { EngineProvider } from '@engine';
import { ModalProvider } from '@modal';
import { QuickBuildProvider } from '../../context/QuickBuildContext';
import { PlaneToolProvider } from '../../context/PlaneToolContext';
import { useAutoSave, useControlBindingsRuntime } from '@hooks';
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
}: {
  projectType: ProjectType
  gameStyle?: GameStyle
  initialSavePath?: string | null
  initialExtractDir?: string | null
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
            viewportRef={viewportRef} 
          />
        </ModalProvider>
        </PlaneToolProvider>
      </QuickBuildProvider>
    </EngineProvider>
  )
}

function EngineViewInner({ projectType, gameStyle, initialSavePath, initialExtractDir, viewportRef }: {
  projectType: ProjectType
  gameStyle?: GameStyle
  initialSavePath?: string | null
  initialExtractDir?: string | null
  viewportRef: React.RefObject<HTMLDivElement>
}) {
  const { 
    handleSave, 
    toggleAutoSave, 
    hasSavedOnce, 
    autoSaveEnabled,
    savingProject,
  } = useAutoSave({ projectType, gameStyle, initialSavePath, initialExtractDir })

  // Teclado/mando del renderer → IPC run_control_script (ventana overlay no recibe input).
  useControlBindingsRuntime()
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
        <SideBarLeft projectType={projectType} gameStyle={gameStyle} />

        <div className="d-flex flex-column flex-fill" style={{ width: '75%' }}>
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

          <div className="row g-0" style={{ height: 120 }}>
            <div className="col-8">
              <LogConsole />
            </div>
            <div className="col-4">
              <MetricsPanel />
            </div>
          </div>
        </div>
      </div>
    </div>
    </SidebarAccordionProvider>
    </SceneManagerProvider>
  )
}
