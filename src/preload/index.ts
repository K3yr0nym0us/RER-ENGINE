import { contextBridge, ipcRenderer } from 'electron'
import type {
  EngineCommand,
  EngineEvent,
  EngineStartPayload,
  OpenProjectResult,
  ProjectSaveData,
  AppResourceUsage,
} from '../shared-types/types'

type EngineEventListener = (event: EngineEvent) => void

const engineEventListeners = new Set<EngineEventListener>()

ipcRenderer.on('engine:event', (_ipcEvent, data: EngineEvent) => {
  for (const listener of engineEventListeners) {
    listener(data)
  }
})

contextBridge.exposeInMainWorld('engine', {
  send: (cmd: EngineCommand): void => {
    ipcRenderer.send('engine:cmd', cmd)
  },
  on: (cb: EngineEventListener): void => {
    engineEventListeners.add(cb)
  },
  off: (cb?: EngineEventListener): void => {
    if (cb) {
      engineEventListeners.delete(cb)
    } else {
      engineEventListeners.clear()
    }
  },
})

// API general para comunicación renderer ↔ main
contextBridge.exposeInMainWorld('electronAPI', {
  setGameStyle: (payload: EngineStartPayload): void => {
    ipcRenderer.send('set-game-style', payload)
  },
  sendViewportBounds: (bounds: { x: number; y: number; width: number; height: number }): void => {
    ipcRenderer.send('viewport-bounds', bounds)
  },
  hideEngineViewport: (): void => {
    ipcRenderer.send('hide-engine-viewport')
  },
  restoreEngineViewport: (bounds?: { x: number; y: number; width: number; height: number }) => {
    if (bounds) {
      ipcRenderer.send('restore-engine-viewport', bounds)
    } else {
      ipcRenderer.send('restore-engine-viewport')
    }
  },
  openModelDialog: (): Promise<string | null> => {
    return ipcRenderer.invoke('open-model-dialog')
  },
  openProjectDialog: (): Promise<OpenProjectResult | null> => {
    return ipcRenderer.invoke('open-project-dialog')
  },
  openAudioDialog: (): Promise<string | null> => {
    return ipcRenderer.invoke('open-audio-dialog')
  },
  openFontDialog: (): Promise<string | null> => {
    return ipcRenderer.invoke('open-font-dialog')
  },
  openScenarioDialog: (): Promise<string | null> => {
    return ipcRenderer.invoke('open-scenario-dialog')
  },
  openCharacterDialog: (): Promise<string | null> => {
    return ipcRenderer.invoke('open-character-dialog')
  },
  openSpriteDialog: (): Promise<string | null> => {
    return ipcRenderer.invoke('open-sprite-dialog')
  },
  getImageDataUrl: (filePath: string): Promise<string | null> => {
    return ipcRenderer.invoke('get-image-data-url', filePath)
  },
  openBackgroundDialog: (): Promise<string | null> => {
    return ipcRenderer.invoke('open-background-dialog')
  },
  saveProject: (data: ProjectSaveData): Promise<string | null> => {
    return ipcRenderer.invoke('save-project', data)
  },
  saveProjectSilent: (filePath: string, data: ProjectSaveData): Promise<boolean> => {
    return ipcRenderer.invoke('save-project-silent', filePath, data)
  },
  onRequestViewportBounds: (cb: () => void): void => {
    ipcRenderer.on('request-viewport-bounds', cb)
  },
  onAutoSaveRequest: (cb: (filePath: string) => void): void => {
    ipcRenderer.removeAllListeners('autosave:request')
    ipcRenderer.on('autosave:request', (_event, filePath: string) => cb(filePath))
  },
  getAppResourceUsage: (): Promise<AppResourceUsage> => {
    return ipcRenderer.invoke('get-app-resource-usage')
  },
})
