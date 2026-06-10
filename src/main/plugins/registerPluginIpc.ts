import { ipcMain, type BrowserWindow } from 'electron'

import type { PluginId, PluginInstallResult, PluginsState } from '../../shared-types/plugins'
import { autoStartLocalAiAssistant, registerLocalAiAssistantIpc } from './local-ai-assistant/ipc'
import { startLlamaServer, stopLlamaServer } from './local-ai-assistant/llamaServerProcess'
import { cancelPluginInstall, installPlugin, uninstallPlugin } from './pluginInstall'
import { PLUGIN_CATALOG } from './pluginCatalog'
import {
  isPluginEnabled,
  isPluginInstalled,
  readPluginsState,
  writePluginsState,
} from './pluginState'

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

  registerLocalAiAssistantIpc(getMainWindow)

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
        if (pluginId === 'local-ai-assistant' && record?.modelPath && record.llamaServerPath) {
          void startLlamaServer(record.llamaServerPath, record.modelPath).then(() => {
            notifyPluginsStateChanged(getMainWindow)
          })
        } else {
          notifyPluginsStateChanged(getMainWindow)
        }
        return state
      }
      state.enabled = state.enabled.filter((id) => id !== pluginId)
      if (pluginId === 'local-ai-assistant') {
        void stopLlamaServer()
      }
      writePluginsState(state)
      notifyPluginsStateChanged(getMainWindow)
      return state
    },
  )

  ipcMain.handle(
    'plugins:install',
    async (_event, pluginId: PluginId): Promise<PluginInstallResult> => {
      const result = await installPlugin(pluginId)
      if (result.ok && pluginId === 'local-ai-assistant') {
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

  ipcMain.handle('plugins:cancel-install', () => {
    cancelPluginInstall()
    return { ok: true }
  })

  ipcMain.handle(
    'plugins:uninstall',
    async (_event, pluginId: PluginId): Promise<PluginInstallResult> => {
      const result = await uninstallPlugin(pluginId)
      notifyPluginsStateChanged(getMainWindow)
      return result
    },
  )
}

/** Arranca plugins habilitados que requieren proceso en background. */
export async function autoStartEnabledPlugins(): Promise<void> {
  await autoStartLocalAiAssistant()
}
