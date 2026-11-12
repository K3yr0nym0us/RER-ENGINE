import fs from 'fs'
import path from 'path'
import AdmZip from 'adm-zip'

import type {
  PluginCatalogEntry,
  PluginDownloadPhase,
  PluginDownloadProgress,
  PluginId,
  PluginInstallResult,
} from '../../../shared-types/plugins'
import { cancelActiveDownload, DownloadCancelledError, downloadFile, resetDownloadCancelState } from '../fileDownload'
import { getPluginCatalogEntry } from '../pluginCatalog'
import {
  getAssistantPreferences,
  getPluginDir,
  readPluginsState,
  writePluginsState,
} from '../pluginState'
import { broadcastPluginsProgress, resetProgressBroadcastThrottle } from '../pluginProgressBroadcast'
import { stopOpenCodeServer } from './openCodeRuntime'
import {
  applyOpenCodeGlobalConfig,
  applyWorkspacePlan,
  buildWorkspacePlan,
  writeWorkspaceStamp,
  workspaceStampMatches,
} from './openCodeSandbox'
import {
  ASSISTANT_PLUGIN_ID,
  getOpenCodeGlobalConfigPath,
  getWorkspaceDir,
  getUiActionsLogPath,
  resolveDocsAiSourceDir,
} from './openCodePaths'
import { resetAssistantSessions } from './assistantChat'

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
  const bytesOverallTotal = entry.openCode.sizeBytes
  let bytesOverallReceived: number
  let step: number
  const stepsTotal = 3

  if (phase === 'opencode') {
    bytesOverallReceived = bytesReceived
    step = 0
  } else if (phase === 'extracting') {
    bytesOverallReceived = entry.openCode.sizeBytes
    step = 1
    percent = 100
  } else {
    bytesOverallReceived = entry.openCode.sizeBytes
    step = 2
    percent = 100
  }

  const overallPercent = Math.min(100, Math.round((bytesOverallReceived / bytesOverallTotal) * 100))

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
  void stopOpenCodeServer()
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

/** Prepara/renueva el sandbox del plugin ya instalado (docs/ai + config + agentes). */
export async function prepareWorkspaceForInstalledPlugin(): Promise<{ ok: boolean; error?: string }> {
  const state = readPluginsState()
  const record = state.installed[ASSISTANT_PLUGIN_ID]
  const entry = getPluginCatalogEntry(ASSISTANT_PLUGIN_ID)
  if (!record || !entry) return { ok: false, error: 'Plugin not installed' }

  const workspaceDir = record.workspaceDir ?? getWorkspaceDir()
  if (workspaceStampMatches(workspaceDir, record.version)) return { ok: true }

  const docsSourceDir = resolveDocsAiSourceDir()
  if (!docsSourceDir) return { ok: false, error: 'docs/ai bundle not found' }

  const plan = buildWorkspacePlan({
    workspaceDir,
    docsSourceDir,
    uiActionsLogPath: getUiActionsLogPath(),
  })
  applyWorkspacePlan(plan, getUiActionsLogPath())
  applyOpenCodeGlobalConfig(getOpenCodeGlobalConfigPath())
  writeWorkspaceStamp(workspaceDir, record.version)
  return { ok: true }
}

export async function installPlugin(pluginId: PluginId): Promise<PluginInstallResult> {
  if (installInProgress) {
    return { ok: false, error: 'Another plugin installation is in progress' }
  }

  if (process.platform !== 'win32') {
    return { ok: false, error: 'Assistant plugin install is only supported on Windows in v1' }
  }

  const entry = getPluginCatalogEntry(pluginId)
  if (!entry) {
    return { ok: false, error: 'Unknown plugin' }
  }

  const docsSourceDir = resolveDocsAiSourceDir()
  if (!docsSourceDir) {
    return { ok: false, error: 'docs/ai bundle not found; cannot prepare assistant workspace' }
  }

  installInProgress = true
  installCancelled = false
  resetDownloadCancelState()
  resetProgressBroadcastThrottle()

  const pluginDir = getPluginDir(pluginId)

  try {
    const binDir = path.join(pluginDir, 'bin')
    fs.mkdirSync(binDir, { recursive: true })

    const exePath = path.join(binDir, entry.openCode.executableName)
    const zipPath = path.join(binDir, 'opencode.zip')

    if (!fs.existsSync(exePath)) {
      sendProgress(buildProgress(pluginId, entry, 'opencode', 0, 0, entry.openCode.sizeBytes), true)
      await downloadFile(
        entry.openCode.downloadUrl,
        zipPath,
        (p) => {
          throwIfInstallCancelled()
          sendProgress(
            buildProgress(pluginId, entry, 'opencode', p.percent, p.bytesReceived, p.bytesTotal || entry.openCode.sizeBytes),
          )
        },
        entry.openCode.sizeBytes * 0.9,
        entry.openCode.sizeBytes,
      )
      throwIfInstallCancelled()

      sendProgress(
        buildProgress(pluginId, entry, 'extracting', 100, entry.openCode.sizeBytes, entry.openCode.sizeBytes),
        true,
      )
      const zip = new AdmZip(zipPath)
      zip.extractAllTo(binDir, true)
      try {
        fs.unlinkSync(zipPath)
      } catch {
        // ignore
      }
      if (!fs.existsSync(exePath)) {
        throw new Error(`${entry.openCode.executableName} was not extracted from runtime archive`)
      }
      throwIfInstallCancelled()
    }

    sendProgress(
      buildProgress(pluginId, entry, 'preparing', 100, entry.openCode.sizeBytes, entry.openCode.sizeBytes),
      true,
    )

    const workspaceDir = getWorkspaceDir()
    const plan = buildWorkspacePlan({
      workspaceDir,
      docsSourceDir,
      uiActionsLogPath: getUiActionsLogPath(),
    })
    applyWorkspacePlan(plan, getUiActionsLogPath())
    applyOpenCodeGlobalConfig(getOpenCodeGlobalConfigPath())
    writeWorkspaceStamp(workspaceDir, entry.version)
    throwIfInstallCancelled()

    // Limpia preferencias obsoletas de instalaciones anteriores (modelos no-OpenCode).
    const prefs = getAssistantPreferences(pluginId)
    if (prefs.selectedModel && !prefs.selectedModel.startsWith('opencode')) {
      const state = readPluginsState()
      state.preferences = { ...state.preferences, [pluginId]: {} }
      writePluginsState(state)
    }

    const state = readPluginsState()
    state.installed[pluginId] = {
      version: entry.version,
      opencodePath: exePath,
      workspaceDir,
      installedAt: new Date().toISOString(),
    }
    if (!state.enabled.includes(pluginId)) {
      state.enabled.push(pluginId)
    }
    writePluginsState(state)

    sendProgress(
      buildProgress(pluginId, entry, 'preparing', 100, entry.openCode.sizeBytes, entry.openCode.sizeBytes),
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
  if (state.preferences) {
    delete state.preferences[pluginId]
    if (Object.keys(state.preferences).length === 0) delete state.preferences
  }
  writePluginsState(state)

  resetAssistantSessions()

  try {
    await stopOpenCodeServer()
    if (fs.existsSync(pluginDir)) {
      fs.rmSync(pluginDir, { recursive: true, force: true })
    }
    return { ok: true }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    return { ok: false, error: message }
  }
}
