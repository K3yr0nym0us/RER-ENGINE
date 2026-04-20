import { useState } from 'react'
import { ProjectSelector } from './components/ProjectSelector'
import { GameStyleSelector } from './3D/components/GameStyleSelector'
import { EngineView } from './components/EngineView'
import type { ProjectType, GameStyle, ProjectConfig } from '../../shared-types/types'

// ── Componente principal ─────────────────────────────────────────────────────

export default function App() {
  const [projectType, setProjectType] = useState<ProjectType | null>(null)
  const [gameStyle,   setGameStyle]   = useState<GameStyle   | null>(null)

  // Cargar proyecto existente: salta directamente al motor
  const handleLoadProject = (cfg: ProjectConfig) => {
    setProjectType(cfg.type)
    setGameStyle(cfg.gameStyle)
  }

  if (!projectType) {
    return (
      <ProjectSelector
        onSelect={setProjectType}
        onLoadProject={handleLoadProject}
      />
    )
  }

  // 2D y scratch saltan directamente al motor (sin elegir estilo de juego)
  if (!gameStyle && projectType !== '2D' && projectType !== 'scratch') {
    return (
      <GameStyleSelector
        projectType={projectType}
        onSelect={setGameStyle}
        onBack={() => setProjectType(null)}
      />
    )
  }

  return <EngineView projectType={projectType} />
}
