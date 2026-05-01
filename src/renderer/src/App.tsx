import { useState } from 'react';

import { TypeProjectSelector } from './pages/TypeProjectSelector/TypeProjectSelector';
import { GameStyleSelector } from './pages/GameStyleSelector/GameStyleSelector';
import { EngineView } from './pages/EngineView/EngineView';

import type { ProjectType, GameStyle, OpenProjectResult, ProjectSaveData } from '@shared-types';

// ── Componente principal ─────────────────────────────────────────────────────

export default function App() {
  const [projectType,   setProjectType]   = useState<ProjectType   | null>(null)
  const [gameStyle,     setGameStyle]     = useState<GameStyle     | null>(null)
  const [initialSave,   setInitialSave]   = useState<ProjectSaveData | null>(null)
  const [initialSavePath, setInitialSavePath] = useState<string | null>(null)

  // Cargar proyecto existente: salta directamente al motor con datos previos
  const handleLoadProject = (result: OpenProjectResult) => {
    setInitialSave(result.project)
    setInitialSavePath(result.filePath)
    setProjectType(result.project.type)
    setGameStyle(result.project.gameStyle)
  }

  if (!projectType) {
    return (
      <TypeProjectSelector
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

  return <EngineView projectType={projectType} initialSave={initialSave} initialSavePath={initialSavePath} />
}
