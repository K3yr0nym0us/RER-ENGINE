import { contextBridge, ipcRenderer } from 'electron'
import './engineBridge'
import type {
  EngineStartPayload,
  OpenProjectResult,
  ProjectSaveData,
  AppResourceUsage,
  ModalElectronOpenRequest,
  ModalElectronDelegateRequest,
} from '../shared-types/types'

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
  openHudImageDialog: (): Promise<string | null> => {
    return ipcRenderer.invoke('open-hud-image-dialog')
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
  pickProjectSavePath: (): Promise<string | null> => {
    return ipcRenderer.invoke('pick-project-save-path')
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
    entityPropertiesState?: unknown
    socketConfigModalState?: unknown
    models?: import('../shared-types/types').ModelInfo[]
  }): void => {
    ipcRenderer.send('modal-electron:patch', data)
  },
  entityPropertiesAction: (handlerId: string, action: unknown): Promise<void> => {
    return ipcRenderer.invoke('modal-electron:entity-properties-action', { handlerId, action })
  },
  socketConfigModalAction: (handlerId: string, action: unknown): Promise<void> => {
    return ipcRenderer.invoke('modal-electron:socket-config-modal-action', { handlerId, action })
  },
  playerUiEditorAction: (handlerId: string, action: unknown): Promise<void> => {
    return ipcRenderer.invoke('modal-electron:player-ui-action', { handlerId, action })
  },
  fetchPlayerUiEditorState: (handlerId: string): Promise<unknown> => {
    return ipcRenderer.invoke('modal-electron:player-ui-state', { handlerId })
  },
  onModalElectronPatch: (
    cb: (data: {
      handlerId: string
      playerUiEditorState?: unknown
      entityPropertiesState?: unknown
      socketConfigModalState?: unknown
      models?: import('../shared-types/types').ModelInfo[]
    }) => void,
  ): (() => void) => {
    const listener = (
      _event: Electron.IpcRendererEvent,
      data: {
        handlerId: string
        playerUiEditorState?: unknown
        entityPropertiesState?: unknown
        socketConfigModalState?: unknown
        models?: import('../shared-types/types').ModelInfo[]
      },
    ) => {
      cb(data)
    }
    ipcRenderer.on('modal-electron:patch', listener)
    return () => ipcRenderer.removeListener('modal-electron:patch', listener)
  },
  onModalElectronEntityPropertiesActionRequest: (
    cb: (req: { handlerId: string; action: unknown; requestId: string }) => void,
  ): (() => void) => {
    const listener = (
      _event: Electron.IpcRendererEvent,
      data: { handlerId: string; action: unknown; requestId: string },
    ) => {
      cb(data)
      ipcRenderer.send(`modal-electron:entity-properties-action-done-${data.requestId}`)
    }
    ipcRenderer.on('modal-electron:entity-properties-action-request', listener)
    return () =>
      ipcRenderer.removeListener('modal-electron:entity-properties-action-request', listener)
  },
  onModalElectronSocketConfigModalActionRequest: (
    cb: (req: { handlerId: string; action: unknown; requestId: string }) => void,
  ): (() => void) => {
    const listener = (
      _event: Electron.IpcRendererEvent,
      data: { handlerId: string; action: unknown; requestId: string },
    ) => {
      cb(data)
      ipcRenderer.send(`modal-electron:socket-config-modal-action-done-${data.requestId}`)
    }
    ipcRenderer.on('modal-electron:socket-config-modal-action-request', listener)
    return () =>
      ipcRenderer.removeListener('modal-electron:socket-config-modal-action-request', listener)
  },
  onModalElectronClosed: (
    cb: (data: { componentKey?: string }) => void,
  ): (() => void) => {
    const listener = (_event: Electron.IpcRendererEvent, data: { componentKey?: string }) => {
      cb(data)
    }
    ipcRenderer.on('modal-electron:closed', listener)
    return () => ipcRenderer.removeListener('modal-electron:closed', listener)
  },
  onModalElectronPlayerUiActionRequest: (
    cb: (req: { handlerId: string; action: unknown; requestId: string }) => void,
  ): (() => void) => {
    const listener = (
      _event: Electron.IpcRendererEvent,
      data: { handlerId: string; action: unknown; requestId: string },
    ) => {
      cb(data)
      ipcRenderer.send(`modal-electron:player-ui-action-done-${data.requestId}`)
    }
    ipcRenderer.on('modal-electron:player-ui-action-request', listener)
    return () => ipcRenderer.removeListener('modal-electron:player-ui-action-request', listener)
  },
  onModalElectronPlayerUiStateRequest: (
    cb: (req: { handlerId: string; requestId: string }) => unknown,
  ): (() => void) => {
    const listener = (
      _event: Electron.IpcRendererEvent,
      data: { handlerId: string; requestId: string },
    ) => {
      const state = cb(data)
      ipcRenderer.send(`modal-electron:player-ui-state-done-${data.requestId}`, state)
    }
    ipcRenderer.on('modal-electron:player-ui-state-request', listener)
    return () => ipcRenderer.removeListener('modal-electron:player-ui-state-request', listener)
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
  pluginsGetCatalog: () => ipcRenderer.invoke('plugins:get-catalog'),
  pluginsGetState: () => ipcRenderer.invoke('plugins:get-state'),
  pluginsSetEnabled: (pluginId: string, enabled: boolean) =>
    ipcRenderer.invoke('plugins:set-enabled', pluginId, enabled),
  pluginsInstall: (pluginId: string) => ipcRenderer.invoke('plugins:install', pluginId),
  pluginsCancelInstall: () => ipcRenderer.invoke('plugins:cancel-install'),
  pluginsUninstall: (pluginId: string) => ipcRenderer.invoke('plugins:uninstall', pluginId),
  pluginsGetLlmStatus: () => ipcRenderer.invoke('plugins:llm-status'),
  pluginsChat: (request: { messages: Array<{ role: string; content: string }> }) =>
    ipcRenderer.invoke('plugins:chat', request),
  pluginsStartLlm: () => ipcRenderer.invoke('plugins:start-llm'),
  pluginsStopLlm: () => ipcRenderer.invoke('plugins:stop-llm'),
  onPluginsDownloadProgress: (
    cb: (progress: import('../shared-types/plugins').PluginDownloadProgress) => void,
  ): (() => void) => {
    const listener = (
      _event: Electron.IpcRendererEvent,
      data: import('../shared-types/plugins').PluginDownloadProgress,
    ) => {
      cb(data)
    }
    ipcRenderer.on('plugins:download-progress', listener)
    return () => ipcRenderer.removeListener('plugins:download-progress', listener)
  },
  onPluginsUiAction: (
    cb: (action: { type: string; accordionKey?: string; targetId?: string }) => void,
  ): (() => void) => {
    const listener = (_event: Electron.IpcRendererEvent, data: Parameters<typeof cb>[0]) => {
      cb(data)
    }
    ipcRenderer.on('plugins:ui-action', listener)
    return () => ipcRenderer.removeListener('plugins:ui-action', listener)
  },
  onPluginsStateChanged: (cb: () => void): (() => void) => {
    const listener = () => cb()
    ipcRenderer.on('plugins:state-changed', listener)
    return () => ipcRenderer.removeListener('plugins:state-changed', listener)
  },
  aiAssistantShow: (config: { locale?: 'en' | 'es' }) =>
    ipcRenderer.invoke('ai-assistant:show', config),
  aiAssistantHide: () => ipcRenderer.invoke('ai-assistant:hide'),
  aiAssistantSetLayout: (layout: 'intro' | 'thinking' | 'input' | 'answer') => {
    ipcRenderer.send('ai-assistant:set-layout', layout)
  },
  aiAssistantFabDragStart: () => {
    ipcRenderer.send('ai-assistant:fab-drag-start')
  },
  aiAssistantFabDragEnd: () => {
    ipcRenderer.send('ai-assistant:fab-drag-end')
  },
  notifyAiAssistantReady: () => {
    ipcRenderer.send('ai-assistant:ready')
  },
  onAiAssistantConfig: (
    cb: (config: { locale?: 'en' | 'es' } | null) => void,
  ): (() => void) => {
    const listener = (
      _event: Electron.IpcRendererEvent,
      config: { locale?: 'en' | 'es' } | null,
    ) => {
      cb(config)
    }
    ipcRenderer.on('ai-assistant:config', listener)
    return () => ipcRenderer.removeListener('ai-assistant:config', listener)
  },
})
