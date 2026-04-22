import { contextBridge, ipcRenderer } from 'electron'
import type { EngineCommand, EngineEvent, ProjectSaveData } from '../shared-types/types'

contextBridge.exposeInMainWorld('engine', {
  send: (cmd: EngineCommand): void => {
    ipcRenderer.send('engine:cmd', cmd)
  },
  on: (cb: (event: EngineEvent) => void): void => {
    // Eliminar listeners anteriores antes de registrar uno nuevo para evitar
    // duplicados cuando React StrictMode monta el componente dos veces.
    ipcRenderer.removeAllListeners('engine:event')
    ipcRenderer.on('engine:event', (_ipcEvent, data: EngineEvent) => cb(data))
  },
  off: (): void => {
    ipcRenderer.removeAllListeners('engine:event')
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
  openProjectDialog: (): Promise<ProjectSaveData | null> => {
    return ipcRenderer.invoke('open-project-dialog')
  },
  openScenarioDialog: (): Promise<string | null> => {
    return ipcRenderer.invoke('open-scenario-dialog')
  },
  openCharacterDialog: (): Promise<string | null> => {
    return ipcRenderer.invoke('open-character-dialog')
  },
  openBackgroundDialog: (): Promise<string | null> => {
    return ipcRenderer.invoke('open-background-dialog')
  },
  saveProject: (data: ProjectSaveData): Promise<boolean> => {
    return ipcRenderer.invoke('save-project', data)
  },
  saveProjectSilent: (filePath: string, data: ProjectSaveData): Promise<boolean> => {
    return ipcRenderer.invoke('save-project-silent', filePath, data)
  },
  onRequestViewportBounds: (cb: () => void): void => {
    ipcRenderer.on('request-viewport-bounds', cb)
  },
})
