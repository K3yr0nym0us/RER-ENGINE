import { ipcMain, type BrowserWindow } from 'electron'

import type {
  AssistantChatRequest,
  AssistantChatResponse,
} from '../../../shared-types/plugins'
import { isPluginEnabled, isPluginInstalled, readPluginsState } from '../pluginState'
import { runAssistantChat } from './assistantChat'
import { isPluginInstallInProgress } from './install'
import {
  getLlamaServerStatus,
  refreshLlamaServerReachability,
  startLlamaServer,
  stopLlamaServer,
} from './llamaServerProcess'

type GetMainWindow = () => BrowserWindow | null

const PLUGIN_ID = 'local-ai-assistant' as const

let ipcRegistered = false

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
      await refreshLlamaServerReachability()
    }
    const { status, error } = getLlamaServerStatus()
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
      const result = await runAssistantChat(
        request.messages,
        (action) => {
          if (!mainWindow || mainWindow.isDestroyed()) return
          mainWindow.webContents.send('plugins:ui-action', action)
        },
        locale,
      )
      return { ok: result.ok, content: result.content, error: result.error, debug: result.debug }
    },
  )

  ipcMain.handle('plugins:start-llm', async () => {
    const state = readPluginsState()
    const record = state.installed[PLUGIN_ID]
    if (!record?.modelPath || !record.llamaServerPath) {
      return { ok: false, error: 'Plugin not installed' }
    }
    return startLlamaServer(record.llamaServerPath, record.modelPath)
  })

  ipcMain.handle('plugins:stop-llm', async () => {
    await stopLlamaServer()
    return { ok: true }
  })
}

/** Arranca llama-server si el plugin quedó habilitado en sesiones anteriores. */
export async function autoStartLocalAiAssistant(): Promise<void> {
  if (!isPluginEnabled(PLUGIN_ID)) return
  const state = readPluginsState()
  const record = state.installed[PLUGIN_ID]
  if (!record?.modelPath || !record.llamaServerPath) return
  await startLlamaServer(record.llamaServerPath, record.modelPath)
}
