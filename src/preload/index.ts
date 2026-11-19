import { contextBridge, ipcRenderer } from 'electron'
import type {
  EngineStartPayload,
  OpenProjectResult,
} from '../shared-types/types'

// API mínima del shell Electron: elegir motor, abrir .save e idioma.
contextBridge.exposeInMainWorld('electronAPI', {
  setGameStyle: (payload: EngineStartPayload): void => {
    ipcRenderer.send('set-game-style', payload)
  },
  /** Electron solo entrega la ruta; el editor Rust abre el `.save` y decide 2D/3D. */
  openProjectDialog: (): Promise<OpenProjectResult | null> => {
    return ipcRenderer.invoke('open-project-dialog')
  },
  /** Guarda el idioma elegido en el launcher; main lo pasa como `--locale` al editor. */
  setLocale: (locale: 'en' | 'es'): void => {
    ipcRenderer.send('set-locale', locale)
  },
})
