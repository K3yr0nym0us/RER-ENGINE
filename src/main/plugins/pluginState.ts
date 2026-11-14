import { app } from 'electron'
import fs from 'fs'
import path from 'path'

import type { PluginId, PluginsState } from '../../shared-types/plugins'

const EMPTY_STATE: PluginsState = { installed: {}, enabled: [] }

function stateFilePath(): string {
  return path.join(app.getPath('userData'), 'plugins', 'state.json')
}

export function getPluginsRootDir(): string {
  return path.join(app.getPath('userData'), 'plugins')
}

export function getPluginDir(pluginId: PluginId): string {
  return path.join(getPluginsRootDir(), pluginId)
}

export function readPluginsState(): PluginsState {
  const file = stateFilePath()
  try {
    if (!fs.existsSync(file)) return { ...EMPTY_STATE }
    const raw = fs.readFileSync(file, 'utf8')
    const parsed = JSON.parse(raw) as PluginsState
    return {
      installed: parsed.installed ?? {},
      enabled: Array.isArray(parsed.enabled) ? parsed.enabled : [],
      preferences: parsed.preferences ?? {},
    }
  } catch {
    return { ...EMPTY_STATE }
  }
}

export function writePluginsState(state: PluginsState): void {
  const dir = path.dirname(stateFilePath())
  fs.mkdirSync(dir, { recursive: true })
  fs.writeFileSync(stateFilePath(), JSON.stringify(state, null, 2), 'utf8')
}

export function isPluginInstalled(pluginId: PluginId): boolean {
  const state = readPluginsState()
  const record = state.installed[pluginId]
  if (pluginId === 'world-gen') {
    // Plugin nativo: vale la DLL instalada en su carpeta.
    return Boolean(record?.libraryPath && fs.existsSync(record.libraryPath))
  }
  // RER-AI: valen el runtime OpenCode y su sandbox.
  if (!record?.opencodePath || !record.workspaceDir) return false
  return fs.existsSync(record.opencodePath) && fs.existsSync(record.workspaceDir)
}

export function isPluginEnabled(pluginId: PluginId): boolean {
  const state = readPluginsState()
  return state.enabled.includes(pluginId) && isPluginInstalled(pluginId)
}

export function getAssistantPreferences(pluginId: PluginId) {
  const state = readPluginsState()
  return state.preferences?.[pluginId] ?? {}
}

export function updateAssistantPreferences(
  pluginId: PluginId,
  patch: { selectedModel?: string | null; goConnected?: boolean },
): void {
  const state = readPluginsState()
  const current = state.preferences?.[pluginId] ?? {}
  const next = { ...current }
  if (patch.selectedModel !== undefined) {
    next.selectedModel = patch.selectedModel ?? undefined
  }
  if (patch.goConnected !== undefined) {
    next.goConnected = patch.goConnected
  }
  state.preferences = { ...state.preferences, [pluginId]: next }
  writePluginsState(state)
}
