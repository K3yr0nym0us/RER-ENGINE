import fs from 'fs'
import path from 'path'
import { app } from 'electron'

import type { PluginId } from '../../../shared-types/plugins'
import { getPluginDir } from '../pluginState'

export const ASSISTANT_PLUGIN_ID: PluginId = 'local-ai-assistant'

export function getAssistantPluginDir(): string {
  return getPluginDir(ASSISTANT_PLUGIN_ID)
}

export function getOpenCodeBinDir(): string {
  return path.join(getAssistantPluginDir(), 'bin')
}

export function getOpenCodeExecutablePath(): string {
  return path.join(getOpenCodeBinDir(), 'opencode.exe')
}

export function getOpenCodeServeLogPath(): string {
  return path.join(getAssistantPluginDir(), 'logs', 'opencode-serve.log')
}

export function getWorkspaceDir(): string {
  return path.join(getAssistantPluginDir(), 'workspace')
}

export function getXdgDir(name: 'config' | 'data' | 'cache' | 'state'): string {
  return path.join(getAssistantPluginDir(), 'xdg', name)
}

export function getOpenCodeGlobalConfigPath(): string {
  return path.join(getXdgDir('config'), 'opencode', 'opencode.json')
}

export function getServiceJsonPath(): string {
  return path.join(getXdgDir('state'), 'opencode', 'service.json')
}

/** Ruta dentro del sandbox donde el plugin rer-ui registra las acciones de UI. */
export function getUiActionsLogPath(): string {
  return path.join(getWorkspaceDir(), 'ui-actions.log')
}

export function getWorkspaceStampPath(): string {
  return path.join(getWorkspaceDir(), '.rer-stamp.json')
}

/** Directorio con los docs empaquetados (extraResources docs/ai) o del repo en dev. */
export function resolveDocsAiSourceDir(): string | null {
  const candidates = [
    typeof process.resourcesPath === 'string'
      ? path.join(process.resourcesPath, 'docs', 'ai')
      : null,
    path.join(process.cwd(), 'docs', 'ai'),
    path.join(app.getAppPath(), '..', '..', 'docs', 'ai'),
    path.join(app.getAppPath(), 'docs', 'ai'),
  ]
  for (const dir of candidates) {
    if (dir && fs.existsSync(dir)) return dir
  }
  return null
}
