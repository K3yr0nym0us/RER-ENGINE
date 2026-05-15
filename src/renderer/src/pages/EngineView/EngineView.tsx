import { useRef } from 'react';

import SideBarLeft from './components/SideBarLeft';
import LogConsole from './components/LogConsole';
import MetricsPanel from './components/MetricsPanel';
import SceneTabsBar from './components/SceneTabsBar';
import TopBarEngine from './components/TopBarEngine';
import { QuickBuildOverlay } from './components/QuickBuildOverlay';

import { EngineProvider } from '@engine';
import { ModalProvider } from '@modal';
import { QuickBuildProvider } from '../../context/QuickBuildContext';
import { useAutoSave } from '@hooks';

import type { GameStyle, ProjectType, ProjectSaveData } from '@shared-types';

export function EngineView({
  projectType,
  gameStyle,
  initialSave,
  initialSavePath,
}: {
  projectType: ProjectType
  gameStyle?: GameStyle
  initialSave?: ProjectSaveData | null
  initialSavePath?: string | null
}) {
  const viewportRef = useRef<HTMLDivElement>(null)

  return (
    <EngineProvider viewportRef={viewportRef} projectType={projectType} gameStyle={gameStyle} initialSave={initialSave}>
      <QuickBuildProvider>
        <ModalProvider>
          <EngineViewInner projectType={projectType} gameStyle={gameStyle} initialSave={initialSave} initialSavePath={initialSavePath} viewportRef={viewportRef} />
        </ModalProvider>
      </QuickBuildProvider>
    </EngineProvider>
  )
}

function EngineViewInner({ projectType, gameStyle, initialSave, initialSavePath, viewportRef }: {
  projectType: ProjectType
  gameStyle?: GameStyle
  initialSave?: ProjectSaveData | null
  initialSavePath?: string | null
  viewportRef: React.RefObject<HTMLDivElement>
}) {
  const { handleSave, toggleAutoSave, hasSavedOnce, autoSaveEnabled } = useAutoSave({ projectType, gameStyle, initialSave, initialSavePath })

  return (
    <div className="app-shell d-flex flex-column">
      <SceneTabsBar initialSave={initialSave} projectType={projectType} />

      <div className="d-flex flex-grow-1 overflow-hidden">

        <SideBarLeft projectType={projectType} />

        <div className="d-flex flex-column flex-fill">
          <TopBarEngine 
            projectType={projectType}
            handleSave={handleSave}
            toggleAutoSave={toggleAutoSave}
            hasSavedOnce={hasSavedOnce}
            autoSaveEnabled={autoSaveEnabled}
          />

          <main
            className="flex-fill position-relative overflow-hidden engine-viewport-area"
            ref={viewportRef}
            style={{ background: 'transparent', marginTop: 0 }}
          >
            <QuickBuildOverlay />
          </main>

          <div className="row g-0" style={{ height: 120 }}>
            <div className="col-9">
              <LogConsole />
            </div>
            <div className="col-3">
              <MetricsPanel />
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}