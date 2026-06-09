import { ipcMain, type BrowserWindow } from 'electron'

import type {
  AssistantChatRequest,
  AssistantChatResponse,
  PluginId,
  PluginInstallResult,
  PluginsState,
} from '../../shared-types/plugins'
import { runAssistantChat } from './assistantChat'
import { installPlugin, isPluginInstallInProgress, uninstallPlugin } from './pluginInstall'
import { PLUGIN_CATALOG } from './pluginCatalog'
import {
  isPluginEnabled,
  isPluginInstalled,
  readPluginsState,
  writePluginsState,
} from './pluginState'
import {
  getLlamaServerStatus,
  refreshLlamaServerReachability,
  startLlamaServer,
  stopLlamaServer,
} from './llamaServerProcess'

type GetMainWindow = () => BrowserWindow | null

let ipcRegistered = false

function notifyPluginsStateChanged(getMainWindow: GetMainWindow): void {
  const win = getMainWindow()
  if (!win || win.isDestroyed()) return
  win.webContents.send('plugins:state-changed')
}

export function registerPluginIpc(getMainWindow: GetMainWindow): void {
  if (ipcRegistered) return
  ipcRegistered = true

  ipcMain.handle('plugins:get-catalog', () => PLUGIN_CATALOG)

  ipcMain.handle('plugins:get-state', (): PluginsState => readPluginsState())

  ipcMain.handle(
    'plugins:set-enabled',
    (_event, pluginId: PluginId, enabled: boolean): PluginsState => {
      const state = readPluginsState()
      if (enabled) {
        if (!isPluginInstalled(pluginId)) return state
        if (!state.enabled.includes(pluginId)) state.enabled.push(pluginId)
        writePluginsState(state)
        const record = state.installed[pluginId]
        if (record?.modelPath && record.llamaServerPath) {
          void startLlamaServer(record.llamaServerPath, record.modelPath).then(() => {
            notifyPluginsStateChanged(getMainWindow)
          })
        } else {
          notifyPluginsStateChanged(getMainWindow)
        }
        return state
      }
      state.enabled = state.enabled.filter((id) => id !== pluginId)
      void stopLlamaServer()
      writePluginsState(state)
      notifyPluginsStateChanged(getMainWindow)
      return state
    },
  )

  ipcMain.handle(
    'plugins:install',
    async (_event, pluginId: PluginId): Promise<PluginInstallResult> => {
      const result = await installPlugin(pluginId, getMainWindow())
      if (result.ok) {
        const state = readPluginsState()
        const record = state.installed[pluginId]
        if (record?.modelPath && record.llamaServerPath) {
          await startLlamaServer(record.llamaServerPath, record.modelPath)
        }
      }
      notifyPluginsStateChanged(getMainWindow)
      return result
    },
  )

  ipcMain.handle(
    'plugins:uninstall',
    async (_event, pluginId: PluginId): Promise<PluginInstallResult> => {
      await stopLlamaServer()
      const result = await uninstallPlugin(pluginId)
      notifyPluginsStateChanged(getMainWindow)
      return result
    },
  )

  ipcMain.handle('plugins:llm-status', async () => {
    const installing = isPluginInstallInProgress()
    if (installing) {
      return {
        status: 'downloading',
        error: null,
        enabled: isPluginEnabled('local-ai-assistant'),
        installed: isPluginInstalled('local-ai-assistant'),
      }
    }
    if (isPluginEnabled('local-ai-assistant')) {
      await refreshLlamaServerReachability()
    }
    const { status, error } = getLlamaServerStatus()
    return {
      status,
      error,
      enabled: isPluginEnabled('local-ai-assistant'),
      installed: isPluginInstalled('local-ai-assistant'),
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
    const record = state.installed['local-ai-assistant']
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
export async function autoStartEnabledPlugins(): Promise<void> {
  if (!isPluginEnabled('local-ai-assistant')) return
  const state = readPluginsState()
  const record = state.installed['local-ai-assistant']
  if (!record?.modelPath || !record.llamaServerPath) return
  await startLlamaServer(record.llamaServerPath, record.modelPath)
}
