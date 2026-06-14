import { useState } from 'react';

import { TypeProjectSelector } from './pages/TypeProjectSelector/TypeProjectSelector';
import { EngineView } from './pages/EngineView/EngineView';

import type { ProjectType, GameStyle, OpenProjectResult, EngineStartPayload } from '@shared-types';
import { DEFAULT_3D_CAMERA_MODE } from '@shared-types';

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
      const defaultCameraMode = DEFAULT_3D_CAMERA_MODE
      setGameStyle(defaultCameraMode)
      const payload: EngineStartPayload = {
        projectType: nextType,
        mode: defaultCameraMode,
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

  return (
    <EngineView
      projectType={projectType}
      gameStyle={gameStyle ?? undefined}
      initialSavePath={initialSavePath}
      initialExtractDir={initialExtractDir}
      onGameStyleChange={setGameStyle}
    />
  )
}
