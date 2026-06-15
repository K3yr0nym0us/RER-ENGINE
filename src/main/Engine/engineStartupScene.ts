import type { GameStyle } from '../../shared-types/types'
import { DEFAULT_3D_CAMERA_MODE } from '../../shared-types/types'
import type { EngineCmdSend } from './engineIpcRegistry'

export interface EngineStartupSceneDeps {
  getEngineBinary: () => string | null
  getProjectType: () => '2D' | '3D' | null
  getExtractDir: () => string
  getGameStyle: () => GameStyle | null
  sendToEngine: EngineCmdSend
  is3dStartupSceneSent: () => boolean
  mark3dStartupSceneSent: () => void
}

/** Tras `ready`: un solo `set_scene` acorde al binario en ejecución. */
export function sendEngineStartupScene(deps: EngineStartupSceneDeps): void {
  const binary = deps.getEngineBinary()
  const projectType = deps.getProjectType()
  const extractDir = deps.getExtractDir()

  if (binary === 'rer_engine_2d' && projectType === '2D') {
    deps.sendToEngine({
      cmd: 'set_scene',
      scene: '2D',
      save_path: extractDir,
    })
    console.log(`[engine] 2D set_scene enviado (extract_dir=${extractDir || '(nuevo)'})`)
    return
  }

  if (binary === 'rer_engine_3d' && projectType === '3D') {
    if (!extractDir) return
    if (deps.is3dStartupSceneSent()) return
    deps.mark3dStartupSceneSent()
    const scene = deps.getGameStyle() ?? DEFAULT_3D_CAMERA_MODE
    deps.sendToEngine({
      cmd: 'set_scene',
      scene,
      save_path: extractDir,
    })
    console.log(`[engine] 3D set_scene enviado (extract_dir=${extractDir})`)
  }
}
