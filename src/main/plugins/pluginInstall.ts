import type { PluginId, PluginInstallResult } from '../../shared-types/plugins'
import {
  cancelPluginInstall as cancelLocalAiInstall,
  installPlugin as installLocalAiAssistant,
  isPluginInstallInProgress as isLocalAiInstallInProgress,
  uninstallPlugin as uninstallLocalAiAssistant,
} from './local-ai-assistant/install'
import { stopOpenCodeServer } from './local-ai-assistant/openCodeRuntime'

export function isPluginInstallInProgress(): boolean {
  return isLocalAiInstallInProgress()
}

export function cancelPluginInstall(): void {
  cancelLocalAiInstall()
}

export async function installPlugin(pluginId: PluginId): Promise<PluginInstallResult> {
  if (pluginId === 'local-ai-assistant') {
    return installLocalAiAssistant(pluginId)
  }
  return { ok: false, error: 'Unknown plugin' }
}

export async function uninstallPlugin(pluginId: PluginId): Promise<PluginInstallResult> {
  if (pluginId === 'local-ai-assistant') {
    await stopOpenCodeServer()
    return uninstallLocalAiAssistant(pluginId)
  }
  return { ok: false, error: 'Unknown plugin' }
}
