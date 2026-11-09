import { useState } from 'react';

import { TypeProjectSelector } from './pages/TypeProjectSelector/TypeProjectSelector';
import { HeadlessEditorHost } from './pages/EngineView/HeadlessEditorHost';

import type { ProjectType, GameStyle, OpenProjectResult, EngineStartPayload } from '@shared-types';
import { DEFAULT_3D_CAMERA_MODE } from '@shared-types';

interface ActiveProject {
  projectType: ProjectType
  gameStyle?: GameStyle
  initialSavePath?: string | null
  initialExtractDir?: string | null
}

// ── Componente principal ─────────────────────────────────────────────────────
//
// La ventana inicial solo elige proyecto. Al elegir, se monta un host headless
// que mantiene el estado React para las ventanas legacy y Electron lanza el
// editor/motor Rust. No se renderiza el editor React.

export default function App() {
  const [project, setProject] = useState<ActiveProject | null>(null)

  const handleSelectProjectType = (nextType: ProjectType) => {
    const gameStyle: GameStyle = nextType === '2D' ? 'top-down' : DEFAULT_3D_CAMERA_MODE
    setProject({
      projectType: nextType,
      gameStyle,
      initialSavePath: null,
      initialExtractDir: null,
    })
    const payload: EngineStartPayload = {
      projectType: nextType,
      mode: nextType === '2D' ? false : gameStyle,
      save_path: false,
    }
    window.electronAPI.setGameStyle(payload)
  }

  const handleLoadProject = (result: OpenProjectResult) => {
    if (!result.extractDir?.trim()) {
      console.error('[App] Abrir proyecto requiere extractDir del .save')
      return
    }
    setProject({
      projectType: result.project.type,
      gameStyle: result.project.gameStyle,
      initialSavePath: result.filePath,
      initialExtractDir: result.extractDir,
    })
    const payload: EngineStartPayload = {
      projectType: result.project.type,
      mode: result.project.type === '2D' ? false : result.project.gameStyle,
      save_path: result.filePath,
      extract_dir: result.extractDir,
    }
    window.electronAPI.setGameStyle(payload)
  }

  if (!project) {
    return (
      <TypeProjectSelector
        onSelect={handleSelectProjectType}
        onLoadProject={handleLoadProject}
      />
    )
  }

  return <HeadlessEditorHost {...project} />
}
