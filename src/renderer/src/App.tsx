import { useState } from 'react';

import { TypeProjectSelector } from './pages/TypeProjectSelector/TypeProjectSelector';
import { GameStyleSelector } from './pages/GameStyleSelector/GameStyleSelector';
import { EngineView } from './pages/EngineView/EngineView';

import type { ProjectType, GameStyle, OpenProjectResult, EngineStartPayload } from '@shared-types';

// ── Componente principal ─────────────────────────────────────────────────────

export default function App() {
  const [projectType,   setProjectType]   = useState<ProjectType   | null>(null)
  const [gameStyle,     setGameStyle]     = useState<GameStyle     | null>(null)
  const [initialSavePath, setInitialSavePath] = useState<string | null>(null)
  const [initialExtractDir, setInitialExtractDir] = useState<string | null>(null)

  const handleSelectProjectType = (nextType: ProjectType) => {
    setProjectType(nextType)
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

  const handleLoadProject = (result: OpenProjectResult) => {
    if (!result.extractDir?.trim()) {
      console.error('[App] Abrir proyecto requiere extractDir del .save')
      return
    }
    setInitialSavePath(result.filePath)
    setInitialExtractDir(result.extractDir)
    setProjectType(result.project.type)
    setGameStyle(result.project.gameStyle)
    const payload: EngineStartPayload = {
      projectType: result.project.type,
      mode: result.project.type === '2D' ? false : result.project.gameStyle,
      save_path: result.filePath,
      extract_dir: result.extractDir,
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

  if (!gameStyle && projectType !== '2D') {
    return (
      <GameStyleSelector
        projectType={projectType}
        savePath={initialSavePath}
        extractDir={initialExtractDir}
        onSelect={setGameStyle}
        onBack={() => setProjectType(null)}
      />
    )
  }

  return (
    <EngineView
      projectType={projectType}
      gameStyle={gameStyle ?? undefined}
      initialSavePath={initialSavePath}
      initialExtractDir={initialExtractDir}
    />
  )
}
