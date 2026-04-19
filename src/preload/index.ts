import { contextBridge, ipcRenderer } from 'electron'
import type { EngineCommand, EngineEvent } from '../shared/types'

contextBridge.exposeInMainWorld('engine', {
  send: (cmd: EngineCommand): void => {
    ipcRenderer.send('engine:cmd', cmd)
  },
  on: (cb: (event: EngineEvent) => void): void => {
    ipcRenderer.on('engine:event', (_ipcEvent, data: EngineEvent) => cb(data))
  },
})

// API general para comunicación renderer ↔ main
contextBridge.exposeInMainWorld('electronAPI', {
  sendViewportBounds: (bounds: { x: number; y: number; width: number; height: number }): void => {
    ipcRenderer.send('viewport-bounds', bounds)
  },
  openModelDialog: (): Promise<string | null> => {
    return ipcRenderer.invoke('open-model-dialog')
  },
  onRequestViewportBounds: (cb: () => void): void => {
    ipcRenderer.on('request-viewport-bounds', cb)
  },
})
