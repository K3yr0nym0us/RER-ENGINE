import { contextBridge, ipcRenderer } from 'electron'
import './engineBridge'
import type {
  EngineStartPayload,
  OpenProjectResult,
  ProjectSaveData,
} from '../shared-types/types'

// API mínima del shell Electron: elegir motor, abrir .save e idioma.
contextBridge.exposeInMainWorld('electronAPI', {
  setGameStyle: (payload: EngineStartPayload): void => {
    ipcRenderer.send('set-game-style', payload)
  },
  openProjectDialog: (): Promise<OpenProjectResult | null> => {
    return ipcRenderer.invoke('open-project-dialog')
  },
  readProjectManifest: (): Promise<ProjectSaveData | null> => {
    return ipcRenderer.invoke('read-project-manifest')
  },
})
