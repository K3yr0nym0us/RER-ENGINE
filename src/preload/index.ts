import { contextBridge, ipcRenderer } from 'electron'
import type {
  EngineCommand,
  EngineEvent,
  EngineStartPayload,
  OpenProjectResult,
  ProjectSaveData,
  AppResourceUsage,
  ModalElectronOpenRequest,
  ModalElectronDelegateRequest,
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
  getProjectExtractDir: (): Promise<string | null> => {
    return ipcRenderer.invoke('get-project-extract-dir')
  },
  readProjectManifest: (): Promise<ProjectSaveData | null> => {
    return ipcRenderer.invoke('read-project-manifest')
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
  openModalElectron: (request: ModalElectronOpenRequest): Promise<void> => {
    return ipcRenderer.invoke('modal-electron:open', request)
  },
  closeModalElectron: (): Promise<void> => {
    return ipcRenderer.invoke('modal-electron:close')
  },
  completeModalElectron: (handlerId: string, result: unknown, callbackKey?: string): void => {
    ipcRenderer.send('modal-electron:result', { handlerId, result, callbackKey })
  },
  notifyModalElectronReady: (): void => {
    ipcRenderer.send('modal-electron:ready')
  },
  resizeModalElectron: (contentHeight: number): void => {
    ipcRenderer.send('modal-electron:resize', contentHeight)
  },
  delegateModalElectron: (
    request: ModalElectronDelegateRequest,
  ): Promise<{ blueprints?: unknown[] } | null> => {
    return ipcRenderer.invoke('modal-electron:delegate', request)
  },
  onModalElectronDelegateRequest: (
    cb: (request: ModalElectronDelegateRequest & { requestId: string }) => Promise<{ blueprints?: unknown[] } | null>,
  ): (() => void) => {
    const listener = async (
      _event: Electron.IpcRendererEvent,
      data: ModalElectronDelegateRequest & { requestId: string },
    ) => {
      const result = await cb(data)
      ipcRenderer.send(`modal-electron:delegate-response-${data.requestId}`, result)
    }
    ipcRenderer.on('modal-electron:delegate-request', listener)
    return () => ipcRenderer.removeListener('modal-electron:delegate-request', listener)
  },
  onModalElectronRender: (
    cb: (payload: ModalElectronOpenRequest | null) => void,
  ): (() => void) => {
    const listener = (_event: Electron.IpcRendererEvent, payload: ModalElectronOpenRequest | null) => {
      cb(payload ?? null)
    }
    ipcRenderer.on('modal-electron:render', listener)
    return () => ipcRenderer.removeListener('modal-electron:render', listener)
  },
  requestParentModalOpen: (req: {
    parentHandlerId: string
    action: string
    payload?: Record<string, unknown>
  }): void => {
    ipcRenderer.send('modal-electron:parent-open', req)
  },
  onModalElectronParentOpenRequest: (
    cb: (req: { parentHandlerId: string; action: string; payload?: Record<string, unknown> }) => void,
  ): (() => void) => {
    const listener = (
      _event: Electron.IpcRendererEvent,
      data: { parentHandlerId: string; action: string; payload?: Record<string, unknown> },
    ) => {
      cb(data)
    }
    ipcRenderer.on('modal-electron:parent-open', listener)
    return () => ipcRenderer.removeListener('modal-electron:parent-open', listener)
  },
  patchModalElectron: (data: {
    handlerId: string
    playerUiEditorState?: unknown
  }): void => {
    ipcRenderer.send('modal-electron:patch', data)
  },
  playerUiEditorAction: (handlerId: string, action: unknown): Promise<void> => {
    return ipcRenderer.invoke('modal-electron:player-ui-action', { handlerId, action })
  },
  onModalElectronPatch: (
    cb: (data: { handlerId: string; playerUiEditorState?: unknown }) => void,
  ): (() => void) => {
    const listener = (
      _event: Electron.IpcRendererEvent,
      data: { handlerId: string; playerUiEditorState?: unknown },
    ) => {
      cb(data)
    }
    ipcRenderer.on('modal-electron:patch', listener)
    return () => ipcRenderer.removeListener('modal-electron:patch', listener)
  },
  onModalElectronPlayerUiActionRequest: (
    cb: (req: { handlerId: string; action: unknown; requestId: string }) => void,
  ): (() => void) => {
    const listener = async (
      _event: Electron.IpcRendererEvent,
      data: { handlerId: string; action: unknown; requestId: string },
    ) => {
      await cb(data)
      ipcRenderer.send(`modal-electron:player-ui-action-done-${data.requestId}`)
    }
    ipcRenderer.on('modal-electron:player-ui-action-request', listener)
    return () => ipcRenderer.removeListener('modal-electron:player-ui-action-request', listener)
  },
  onModalElectronResult: (
    cb: (handlerId: string, result: unknown, callbackKey?: string) => void,
  ): (() => void) => {
    const listener = (
      _event: Electron.IpcRendererEvent,
      data: { handlerId: string; result: unknown; callbackKey?: string },
    ) => {
      cb(data.handlerId, data.result, data.callbackKey)
    }
    ipcRenderer.on('modal-electron:result', listener)
    return () => ipcRenderer.removeListener('modal-electron:result', listener)
  },
})
