import { useRef } from 'react';

import SideBarLeft from './components/SideBarLeft';
import LogConsole from './components/LogConsole';
import TopBarEngine from './components/TopBarEngine';

import { EngineProvider } from '@engine';
import { ModalProvider } from '@modal';
import { useAutoSave } from '../../hooks/useAutoSave';

import type { ProjectType, ProjectSaveData } from '@shared-types';

export function EngineView({ projectType, initialSave, initialSavePath }: { projectType: ProjectType; initialSave?: ProjectSaveData | null; initialSavePath?: string | null }) {
  const viewportRef = useRef<HTMLDivElement>(null)

  return (
    <EngineProvider viewportRef={viewportRef} projectType={projectType} initialSave={initialSave}>
      <ModalProvider>
        <EngineViewInner projectType={projectType} initialSave={initialSave} initialSavePath={initialSavePath} viewportRef={viewportRef} />
      </ModalProvider>
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

  return (
    <div className="app-shell d-flex flex-column">
      <div className="d-flex flex-grow-1 overflow-hidden">

        <SideBarLeft projectType={projectType} />

        <div className="d-flex flex-column flex-fill">
          <TopBarEngine 
            projectType={projectType}
            handleSave={handleSave}
            toggleAutoSave={toggleAutoSave}
          />

          <main
            className="flex-fill position-relative overflow-hidden engine-viewport-area"
            ref={viewportRef}
            style={{ background: 'transparent', marginTop: 0 }}
          />

          <LogConsole />
        </div>
      </div>
    </div>
  )
}