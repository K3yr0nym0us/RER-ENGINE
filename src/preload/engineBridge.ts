import { contextBridge, ipcRenderer } from 'electron'
import type {
  EngineCommand,
  EngineCommand2D,
  EngineCommand3D,
  EngineEvent,
} from '../shared-types/types'

type EngineEventListener = (event: EngineEvent) => void

const engineEventListeners = new Set<EngineEventListener>()

ipcRenderer.on('engine:event', (_ipcEvent, data: EngineEvent) => {
  for (const listener of engineEventListeners) {
    listener(data)
  }
})

function dispatchEngineCommand(cmd: EngineCommand): void {
  ipcRenderer.send('engine:cmd', cmd)
}

const onEngineEvent = (cb: EngineEventListener): void => {
  engineEventListeners.add(cb)
}

const offEngineEvent = (cb?: EngineEventListener): void => {
  if (cb) {
    engineEventListeners.delete(cb)
  } else {
    engineEventListeners.clear()
  }
}

contextBridge.exposeInMainWorld('engine', {
  send: dispatchEngineCommand,
  send2d: (cmd: EngineCommand2D) => dispatchEngineCommand(cmd),
  send3d: (cmd: EngineCommand3D) => dispatchEngineCommand(cmd),
  on: onEngineEvent,
  off: offEngineEvent,
})

contextBridge.exposeInMainWorld('engine2d', {
  send: (cmd: EngineCommand2D) => dispatchEngineCommand(cmd),
  on: onEngineEvent,
  off: offEngineEvent,
})

contextBridge.exposeInMainWorld('engine3d', {
  send: (cmd: EngineCommand3D) => dispatchEngineCommand(cmd),
  on: onEngineEvent,
  off: offEngineEvent,
})
