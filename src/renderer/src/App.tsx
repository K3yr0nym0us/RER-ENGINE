import { useState } from 'react';

import { TypeProjectSelector } from './pages/TypeProjectSelector/TypeProjectSelector';
import { GameStyleSelector } from './pages/GameStyleSelector/GameStyleSelector';
import { EngineView } from './pages/EngineView/EngineView';

import type { ProjectType, GameStyle, OpenProjectResult, ProjectSaveData, EngineStartPayload } from '@shared-types';

// ── Componente principal ─────────────────────────────────────────────────────

export default function App() {
  const [projectType,   setProjectType]   = useState<ProjectType   | null>(null)
  const [gameStyle,     setGameStyle]     = useState<GameStyle     | null>(null)
  const [initialSave,   setInitialSave]   = useState<ProjectSaveData | null>(null)
  const [initialSavePath, setInitialSavePath] = useState<string | null>(null)
  const [initialExtractDir, setInitialExtractDir] = useState<string | null>(null)

  const handleSelectProjectType = (nextType: ProjectType) => {
    setProjectType(nextType)
    setInitialSave(null)
    setInitialSavePath(null)
    setInitialExtractDir(null)
    if (nextType === '2D') {
      const defaultGameStyle: GameStyle = 'top-down'
      setGameStyle(defaultGameStyle)
      const payload: EngineStartPayload = {
        projectType: nextType,
        mode: false,
        save_path: false,
      }
      window.electronAPI.setGameStyle(payload)
    } else {
      setGameStyle(null)
      const payload: EngineStartPayload = {
        projectType: nextType,
        mode: 'first-person',
        save_path: false,
      }
      window.electronAPI.setGameStyle(payload)
    }
  }

  // Cargar proyecto existente: salta directamente al motor con datos previos
  const handleLoadProject = (result: OpenProjectResult) => {
    const engineLoadsFromExtract = Boolean(result.extractDir?.trim())
    setInitialSave(engineLoadsFromExtract ? null : (result.project as ProjectSaveData))
    setInitialSavePath(result.filePath)
    setInitialExtractDir(engineLoadsFromExtract ? result.extractDir : null)
    setProjectType(result.project.type)
    setGameStyle(result.project.gameStyle)
    const payload: EngineStartPayload = {
      projectType: result.project.type,
      mode: result.project.type === '2D' ? false : result.project.gameStyle,
      save_path: result.filePath,
      extract_dir: engineLoadsFromExtract ? result.extractDir : false,
    }
    window.electronAPI.setGameStyle(payload)
  }

  if (!projectType) {
    return (
      <TypeProjectSelector
        onSelect={handleSelectProjectType}
        onLoadProject={handleLoadProject}
      />
    )
  }

  // 2D salta directamente al motor (sin elegir estilo de juego)
  if (!gameStyle && projectType !== '2D') {
    return (
      <GameStyleSelector
        projectType={projectType}
        savePath={initialSavePath}
        onSelect={setGameStyle}
        onBack={() => setProjectType(null)}
      />
    )
  }

  return (
    <EngineView
      projectType={projectType}
      gameStyle={gameStyle ?? undefined}
      initialSave={initialSave}
      initialSavePath={initialSavePath}
      initialExtractDir={initialExtractDir}
    />
  )
}
