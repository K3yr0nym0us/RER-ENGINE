import { ipcMain, shell, webContents, type BrowserWindow } from 'electron'

import type {
  AssistantChatDelta,
  AssistantChatRequest,
  AssistantChatResponse,
  AssistantGoConnectResult,
  AssistantModelsResult,
  AssistantSettings,
} from '../../../shared-types/plugins'
import { ASSISTANT_PLUGIN_ID } from './openCodePaths'
import { getAssistantPreferences, isPluginEnabled, isPluginInstalled, readPluginsState, updateAssistantPreferences } from '../pluginState'
import {
  cancelAssistantChat,
  runAssistantChat,
  type UiActionEmitter,
} from './assistantChat'
import { isPluginInstallInProgress, prepareWorkspaceForInstalledPlugin } from './install'
import {
  getOpenCodeEndpoint,
  getOpenCodeServerStatus,
  refreshOpenCodeReachability,
  startOpenCodeServer,
  stopOpenCodeServer,
} from './openCodeRuntime'
import {
  connectOpenCodeGo,
  disconnectOpenCodeGo,
  isOpenCodeAccountConnected,
  isOpenCodeGoConnected,
  listModels,
  startOpenCodeAccountLogin,
} from './openCodeClient'

type GetMainWindow = () => BrowserWindow | null

const PLUGIN_ID = ASSISTANT_PLUGIN_ID

let ipcRegistered = false

function broadcastDelta(delta: AssistantChatDelta): void {
  for (const wc of webContents.getAllWebContents()) {
    if (wc.isDestroyed()) continue
    try {
      wc.send('plugins:chat-delta', delta)
    } catch {
      // ignore destroyed mid-send
    }
  }
}

export function registerLocalAiAssistantIpc(getMainWindow: GetMainWindow): void {
  if (ipcRegistered) return
  ipcRegistered = true

  ipcMain.handle('plugins:llm-status', async () => {
    const installing = isPluginInstallInProgress()
    if (installing) {
      return {
        status: 'downloading',
        error: null,
        enabled: isPluginEnabled(PLUGIN_ID),
        installed: isPluginInstalled(PLUGIN_ID),
      }
    }
    if (isPluginEnabled(PLUGIN_ID)) {
      await refreshOpenCodeReachability()
    }
    const { status, error } = getOpenCodeServerStatus()
    return {
      status,
      error,
      enabled: isPluginEnabled(PLUGIN_ID),
      installed: isPluginInstalled(PLUGIN_ID),
    }
  })

  ipcMain.handle(
    'plugins:chat',
    async (_event, request: AssistantChatRequest): Promise<AssistantChatResponse> => {
      const mainWindow = getMainWindow()
      const locale = request.locale === 'es' ? 'es' : 'en'
      const emitUiAction: UiActionEmitter = (action) => {
        if (!mainWindow || mainWindow.isDestroyed()) return
        mainWindow.webContents.send('plugins:ui-action', action)
      }
      const result = await runAssistantChat(request, { emitUiAction, emitDelta: broadcastDelta }, locale)
      return { ok: result.ok, content: result.content, error: result.error, debug: result.debug }
    },
  )

  ipcMain.handle('plugins:chat-cancel', (_event, conversationId: string) => {
    if (typeof conversationId === 'string' && conversationId) {
      cancelAssistantChat(conversationId)
    }
    return { ok: true }
  })

  ipcMain.handle('plugins:start-llm', async () => {
    const prepared = await prepareWorkspaceForInstalledPlugin()
    if (!prepared.ok) return { ok: false, error: prepared.error }
    return startOpenCodeServer()
  })

  ipcMain.handle('plugins:stop-llm', async () => {
    await stopOpenCodeServer()
    return { ok: true }
  })

  ipcMain.handle('plugins:list-models', async (): Promise<AssistantModelsResult> => {
    const endpoint = getOpenCodeEndpoint()
    if (!endpoint) return { ok: false, error: 'OpenCode server is not running' }
    return listModels(endpoint)
  })

  ipcMain.handle('plugins:set-model', (_event, modelRef: string | null) => {
    const value = typeof modelRef === 'string' && modelRef ? modelRef : null
    updateAssistantPreferences(PLUGIN_ID, { selectedModel: value })
    return { ok: true }
  })

  ipcMain.handle('plugins:get-assistant-settings', async (): Promise<AssistantSettings> => {
    const prefs = getAssistantPreferences(PLUGIN_ID)
    const endpoint = getOpenCodeEndpoint()
    const goConnected = endpoint ? await isOpenCodeGoConnected(endpoint) : Boolean(prefs.goConnected)
    const opencodeConnected = endpoint ? await isOpenCodeAccountConnected(endpoint) : false
    return {
      selectedModel: prefs.selectedModel ?? null,
      goConnected,
      opencodeConnected,
    }
  })

  ipcMain.handle('plugins:start-opencode-login', async () => {
    const endpoint = getOpenCodeEndpoint()
    if (!endpoint) return { ok: false, error: 'OpenCode server is not running' }
    const started = await startOpenCodeAccountLogin(endpoint)
    if (started.ok && started.url) {
      await shell.openExternal(started.url)
    }
    return started
  })

  ipcMain.handle(
    'plugins:connect-go',
    async (_event, apiKey: string): Promise<AssistantGoConnectResult> => {
      const endpoint = getOpenCodeEndpoint()
      if (!endpoint) return { ok: false, error: 'OpenCode server is not running' }
      if (typeof apiKey !== 'string' || apiKey.trim().length < 8) {
        return { ok: false, error: 'Enter a valid OpenCode Go token' }
      }
      const result = await connectOpenCodeGo(endpoint, apiKey.trim())
      updateAssistantPreferences(PLUGIN_ID, { goConnected: result.ok })
      return result
    },
  )

  ipcMain.handle('plugins:disconnect-go', async (): Promise<AssistantGoConnectResult> => {
    const endpoint = getOpenCodeEndpoint()
    if (!endpoint) return { ok: false, error: 'OpenCode server is not running' }
    const result = await disconnectOpenCodeGo(endpoint)
    if (result.ok) updateAssistantPreferences(PLUGIN_ID, { goConnected: false })
    return result
  })
}

/** Arranca el servidor OpenCode si el plugin quedó habilitado en sesiones anteriores. */
export async function autoStartLocalAiAssistant(): Promise<void> {
  if (!isPluginEnabled(PLUGIN_ID)) return
  const state = readPluginsState()
  const record = state.installed[PLUGIN_ID]
  if (!record?.opencodePath || !record.workspaceDir) return
  const prepared = await prepareWorkspaceForInstalledPlugin()
  if (!prepared.ok) return
  await startOpenCodeServer()
}
