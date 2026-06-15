import { ipcMain } from 'electron'

import type { EngineCommand } from '../../shared-types/types'
import { handleEngineCommand, type EngineIpcRegistryDeps } from './engineIpcRegistry'

let ipcRegistered = false

/** Registra el canal único `engine:cmd` con validación por motor activo. */
export function registerEngineCmdIpc(deps: EngineIpcRegistryDeps): void {
  if (ipcRegistered) return
  ipcRegistered = true

  ipcMain.on('engine:cmd', (_event, cmd: EngineCommand) => {
    handleEngineCommand(cmd, deps)
  })
}
