import fs from 'fs'
import path from 'path'
import type { BrowserWindow } from 'electron'
import type { PluginDownloadProgress, PluginId, PluginInstallResult } from '../../shared-types/plugins'
import { downloadFile } from './fileDownload'
import { getPluginCatalogEntry } from './pluginCatalog'
import { downloadAndExtractLlamaRuntime } from './llamaRuntime'
import {
  getPluginDir,
  readPluginsState,
  writePluginsState,
} from './pluginState'

let installInProgress = false

function sendProgress(mainWindow: BrowserWindow | null, progress: PluginDownloadProgress): void {
  if (!mainWindow || mainWindow.isDestroyed()) return
  mainWindow.webContents.send('plugins:download-progress', progress)
}

export function isPluginInstallInProgress(): boolean {
  return installInProgress
}

export async function installPlugin(
  pluginId: PluginId,
  mainWindow: BrowserWindow | null,
): Promise<PluginInstallResult> {
  if (installInProgress) {
    return { ok: false, error: 'Another plugin installation is in progress' }
  }

  if (process.platform !== 'win32') {
    return { ok: false, error: 'Local AI plugin install is only supported on Windows in v1' }
  }

  const entry = getPluginCatalogEntry(pluginId)
  if (!entry) {
    return { ok: false, error: 'Unknown plugin' }
  }

  installInProgress = true

  try {
    const pluginDir = getPluginDir(pluginId)
    const binDir = path.join(pluginDir, 'bin')
    const modelsDir = path.join(pluginDir, 'models')
    fs.mkdirSync(binDir, { recursive: true })
    fs.mkdirSync(modelsDir, { recursive: true })

    const llamaExePath = path.join(binDir, entry.llamaServer.executableName)

    if (!fs.existsSync(llamaExePath)) {
      sendProgress(mainWindow, {
        pluginId,
        phase: 'llama-server',
        percent: 0,
        bytesReceived: 0,
        bytesTotal: entry.llamaServer.sizeBytes,
      })

      await downloadAndExtractLlamaRuntime(
        entry.llamaServer,
        binDir,
        (percent, bytesReceived, bytesTotal) => {
          sendProgress(mainWindow, {
            pluginId,
            phase: 'llama-server',
            percent,
            bytesReceived,
            bytesTotal,
          })
        },
      )
    }

    const modelPath = path.join(modelsDir, entry.model.filename)

    if (!fs.existsSync(modelPath)) {
      sendProgress(mainWindow, {
        pluginId,
        phase: 'model',
        percent: 0,
        bytesReceived: 0,
        bytesTotal: entry.model.sizeBytes,
      })

      await downloadFile(
        entry.model.downloadUrl,
        modelPath,
        (p) => {
          sendProgress(mainWindow, {
            pluginId,
            phase: 'model',
            percent: p.percent,
            bytesReceived: p.bytesReceived,
            bytesTotal: p.bytesTotal || entry.model.sizeBytes,
          })
        },
        entry.model.sizeBytes * 0.5,
      )
    }

    const state = readPluginsState()
    state.installed[pluginId] = {
      version: entry.version,
      modelPath,
      llamaServerPath: llamaExePath,
      installedAt: new Date().toISOString(),
    }
    if (!state.enabled.includes(pluginId)) {
      state.enabled.push(pluginId)
    }
    writePluginsState(state)

    return { ok: true }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    return { ok: false, error: message }
  } finally {
    installInProgress = false
  }
}

export async function uninstallPlugin(pluginId: PluginId): Promise<PluginInstallResult> {
  const pluginDir = getPluginDir(pluginId)
  const state = readPluginsState()

  delete state.installed[pluginId]
  state.enabled = state.enabled.filter((id) => id !== pluginId)
  writePluginsState(state)

  try {
    if (fs.existsSync(pluginDir)) {
      fs.rmSync(pluginDir, { recursive: true, force: true })
    }
    return { ok: true }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    return { ok: false, error: message }
  }
}
