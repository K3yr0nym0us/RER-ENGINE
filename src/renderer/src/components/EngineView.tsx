import { useRef } from 'react';

import SideBarLeft from './SideBarLeft';
import LogConsole from './LogConsole';
import TopBarEngine from './TopBarEngine';

import { EngineProvider } from '../context/useContextEngine';
import { useAutoSave } from '../hooks/useAutoSave';

import type { ProjectType, ProjectSaveData } from '../../../shared-types/types';

export function EngineView({ projectType, initialSave }: { projectType: ProjectType; initialSave?: ProjectSaveData | null }) {
  const viewportRef = useRef<HTMLDivElement>(null)

  return (
    <EngineProvider viewportRef={viewportRef} projectType={projectType} initialSave={initialSave}>
      <EngineViewInner projectType={projectType} initialSave={initialSave} viewportRef={viewportRef} />
    </EngineProvider>
  )
}

function EngineViewInner({ projectType, initialSave, viewportRef }: {
  projectType: ProjectType
  initialSave?: ProjectSaveData | null
  viewportRef: React.RefObject<HTMLDivElement>
}) {
  const { handleSave, toggleAutoSave } = useAutoSave({ projectType, initialSave })

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