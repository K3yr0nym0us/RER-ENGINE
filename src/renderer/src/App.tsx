import { useState } from 'react';

import { ProjectSelector } from './components/ProjectSelector';
import { GameStyleSelector } from './3D/components/GameStyleSelector';
import { EngineView } from './components/EngineView';

import type { ProjectType, GameStyle, ProjectSaveData } from '../../shared-types/types';

// ── Componente principal ─────────────────────────────────────────────────────

export default function App() {
  const [projectType,   setProjectType]   = useState<ProjectType   | null>(null)
  const [gameStyle,     setGameStyle]     = useState<GameStyle     | null>(null)
  const [initialSave,   setInitialSave]   = useState<ProjectSaveData | null>(null)

  // Cargar proyecto existente: salta directamente al motor con datos previos
  const handleLoadProject = (data: ProjectSaveData) => {
    setInitialSave(data)
    setProjectType(data.type)
    setGameStyle(data.gameStyle)
  }

  if (!projectType) {
    return (
      <ProjectSelector
        onSelect={setProjectType}
        onLoadProject={handleLoadProject}
      />
    )
  }

  // 2D salta directamente al motor (sin elegir estilo de juego)
  if (!gameStyle && projectType !== '2D') {
    return (
      <GameStyleSelector
        projectType={projectType}
        onSelect={setGameStyle}
        onBack={() => setProjectType(null)}
      />
    )
  }

  return <EngineView projectType={projectType} initialSave={initialSave} />
}
