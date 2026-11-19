import { useCallback } from 'react'
import type { EngineCommand2D, EngineCommand3D } from '@shared-types'

/** Envío de comandos al motor nativo correspondiente. */
export function useEngineSend() {
  const send2d = useCallback((cmd: EngineCommand2D) => {
    window.engine2d.send(cmd)
  }, [])

  const send3d = useCallback((cmd: EngineCommand3D) => {
    window.engine3d.send(cmd)
  }, [])

  return { send2d, send3d }
}
