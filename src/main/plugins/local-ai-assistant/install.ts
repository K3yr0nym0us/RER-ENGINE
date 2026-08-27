import fs from 'fs'
import path from 'path'
import type {
  PluginCatalogEntry,
  PluginDownloadPhase,
  PluginDownloadProgress,
  PluginId,
  PluginInstallResult,
} from '../../../shared-types/plugins'
import { cancelActiveDownload, DownloadCancelledError, downloadFile, resetDownloadCancelState } from '../fileDownload'
import { getPluginCatalogEntry } from '../pluginCatalog'
import { downloadAndExtractLlamaRuntime } from './llamaRuntime'
import { ensureMsvc2015to2022X64 } from './msvcRedistributable'
import { resolvePluginUiLocale } from './pluginUiLocale'
import {
  getPluginDir,
  readPluginsState,
  writePluginsState,
} from '../pluginState'
import { broadcastPluginsProgress, resetProgressBroadcastThrottle } from '../pluginProgressBroadcast'

const INSTALL_CANCELLED_ERROR = 'Installation cancelled'

let installInProgress = false
let installCancelled = false

function sendProgress(progress: PluginDownloadProgress, force = false): void {
  broadcastPluginsProgress(progress, force)
}

function buildProgress(
  pluginId: PluginId,
  entry: PluginCatalogEntry,
  phase: PluginDownloadPhase,
  percent: number,
  bytesReceived: number,
  bytesTotal: number,
): PluginDownloadProgress {
  const bytesOverallTotal = entry.llamaServer.sizeBytes + entry.model.sizeBytes
  let bytesOverallReceived: number
  let step: number
  let stepsTotal = 2

  if (phase === 'msvc-redist') {
    bytesOverallReceived = 0
    step = 0
    stepsTotal = 3
    percent = Math.max(percent, 5)
  } else if (phase === 'llama-server') {
    bytesOverallReceived = bytesReceived
    step = 1
  } else if (phase === 'extracting') {
    bytesOverallReceived = entry.llamaServer.sizeBytes
    step = 1
    percent = 100
  } else {
    bytesOverallReceived = entry.llamaServer.sizeBytes + bytesReceived
    step = 2
  }

  const overallPercent =
    phase === 'msvc-redist'
      ? Math.min(15, Math.max(5, percent))
      : Math.min(100, Math.round((bytesOverallReceived / bytesOverallTotal) * 100))

  return {
    pluginId,
    phase,
    step,
    stepsTotal,
    percent,
    overallPercent,
    bytesReceived,
    bytesTotal,
    bytesOverallReceived,
    bytesOverallTotal,
  }
}

function throwIfInstallCancelled(): void {
  if (installCancelled) {
    throw new DownloadCancelledError()
  }
}

function cleanupPartialPluginDir(pluginDir: string): void {
  try {
    if (fs.existsSync(pluginDir)) {
      fs.rmSync(pluginDir, { recursive: true, force: true })
    }
  } catch {
    // ignore
  }
}

export function isPluginInstallInProgress(): boolean {
  return installInProgress
}

export function cancelPluginInstall(): void {
  if (!installInProgress) return
  installCancelled = true
  cancelActiveDownload()
}

export async function installPlugin(pluginId: PluginId): Promise<PluginInstallResult> {
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
  installCancelled = false
  resetDownloadCancelState()
  resetProgressBroadcastThrottle()

  const pluginDir = getPluginDir(pluginId)

  try {
    sendProgress(buildProgress(pluginId, entry, 'msvc-redist', 5, 0, 0), true)
    const msvc = await ensureMsvc2015to2022X64(resolvePluginUiLocale(), undefined, () => {
      sendProgress(buildProgress(pluginId, entry, 'msvc-redist', 10, 0, 0), true)
    })
    if (!msvc.ok) {
      return msvc
    }
    throwIfInstallCancelled()

    const binDir = path.join(pluginDir, 'bin')
    const modelsDir = path.join(pluginDir, 'models')
    fs.mkdirSync(binDir, { recursive: true })
    fs.mkdirSync(modelsDir, { recursive: true })

    const llamaExePath = path.join(binDir, entry.llamaServer.executableName)

    if (!fs.existsSync(llamaExePath)) {
      sendProgress(
        buildProgress(pluginId, entry, 'llama-server', 0, 0, entry.llamaServer.sizeBytes),
        true,
      )

      await downloadAndExtractLlamaRuntime(
        entry.llamaServer,
        binDir,
        (percent, bytesReceived, bytesTotal) => {
          throwIfInstallCancelled()
          sendProgress(
            buildProgress(pluginId, entry, 'llama-server', percent, bytesReceived, bytesTotal),
          )
        },
        () => {
          throwIfInstallCancelled()
          sendProgress(
            buildProgress(
              pluginId,
              entry,
              'extracting',
              100,
              entry.llamaServer.sizeBytes,
              entry.llamaServer.sizeBytes,
            ),
            true,
          )
        },
      )

      throwIfInstallCancelled()
    }

    const modelPath = path.join(modelsDir, entry.model.filename)

    if (!fs.existsSync(modelPath)) {
      throwIfInstallCancelled()

      sendProgress(
        buildProgress(pluginId, entry, 'model', 0, 0, entry.model.sizeBytes),
        true,
      )

      await downloadFile(
        entry.model.downloadUrl,
        modelPath,
        (p) => {
          throwIfInstallCancelled()
          sendProgress(
            buildProgress(
              pluginId,
              entry,
              'model',
              p.percent,
              p.bytesReceived,
              p.bytesTotal || entry.model.sizeBytes,
            ),
          )
        },
        entry.model.sizeBytes * 0.5,
        entry.model.sizeBytes,
      )
    }

    throwIfInstallCancelled()

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

    sendProgress(
      buildProgress(pluginId, entry, 'model', 100, entry.model.sizeBytes, entry.model.sizeBytes),
      true,
    )

    return { ok: true }
  } catch (err) {
    if (installCancelled || err instanceof DownloadCancelledError) {
      cleanupPartialPluginDir(pluginDir)
      return { ok: false, error: INSTALL_CANCELLED_ERROR, cancelled: true }
    }
    const message = err instanceof Error ? err.message : String(err)
    return { ok: false, error: message }
  } finally {
    installInProgress = false
    installCancelled = false
    resetDownloadCancelState()
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
