import { contextBridge, ipcRenderer } from 'electron'
import type {
  EngineCommand,
  EngineCommand2D,
  EngineCommand3D,
} from '../shared-types/types'

/** Único canal hacia el motor: el editor nativo Rust ya consume los eventos directamente. */
function dispatchEngineCommand(cmd: EngineCommand): void {
  ipcRenderer.send('engine:cmd', cmd)
}

contextBridge.exposeInMainWorld('engine2d', {
  send: (cmd: EngineCommand2D) => dispatchEngineCommand(cmd),
})

contextBridge.exposeInMainWorld('engine3d', {
  send: (cmd: EngineCommand3D) => dispatchEngineCommand(cmd),
})
