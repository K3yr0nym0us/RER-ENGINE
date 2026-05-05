import { useRef, useState } from 'react';

import SideBarLeft from './components/SideBarLeft';
import LogConsole from './components/LogConsole';
import SceneTabsBar from './components/SceneTabsBar';
import TopBarEngine from './components/TopBarEngine';
import { QuickBuildOverlay } from './components/QuickBuildOverlay';

import { EngineProvider } from '@engine';
import { ModalProvider } from '@modal';
import { QuickBuildProvider } from '../../context/QuickBuildContext';
import { useAutoSave } from '../../hooks/useAutoSave';

import type { ProjectType, ProjectSaveData } from '@shared-types';

export function EngineView({ projectType, initialSave, initialSavePath }: { projectType: ProjectType; initialSave?: ProjectSaveData | null; initialSavePath?: string | null }) {
  const viewportRef = useRef<HTMLDivElement>(null)

  return (
    <EngineProvider viewportRef={viewportRef} projectType={projectType} initialSave={initialSave}>
      <QuickBuildProvider>
        <ModalProvider>
          <EngineViewInner projectType={projectType} initialSave={initialSave} initialSavePath={initialSavePath} viewportRef={viewportRef} />
        </ModalProvider>
      </QuickBuildProvider>
    </EngineProvider>
  )
}

function EngineViewInner({ projectType, initialSave, initialSavePath, viewportRef }: {
  projectType: ProjectType
  initialSave?: ProjectSaveData | null
  initialSavePath?: string | null
  viewportRef: React.RefObject<HTMLDivElement>
}) {
  const { handleSave, toggleAutoSave } = useAutoSave({ projectType, initialSave, initialSavePath })
  const [debugOverlayVisible, setDebugOverlayVisible] = useState(true)

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
            debugOverlayVisible={debugOverlayVisible}
            onToggleDebugOverlay={() => setDebugOverlayVisible(v => !v)}
          />

          <main
            className="flex-fill position-relative overflow-hidden engine-viewport-area"
            ref={viewportRef}
            style={{ background: 'transparent', marginTop: 0 }}
          >
            <QuickBuildOverlay />
          </main>

          <LogConsole />
        </div>
      </div>
    </div>
  )
}