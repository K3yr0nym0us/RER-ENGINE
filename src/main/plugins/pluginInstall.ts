import type { PluginId, PluginInstallResult } from '../../shared-types/plugins'
import {
  cancelPluginInstall as cancelLocalAiInstall,
  installPlugin as installLocalAiAssistant,
  isPluginInstallInProgress as isLocalAiInstallInProgress,
  uninstallPlugin as uninstallLocalAiAssistant,
} from './local-ai-assistant/install'
import { stopLlamaServer } from './local-ai-assistant/llamaServerProcess'

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
    await stopLlamaServer()
    return uninstallLocalAiAssistant(pluginId)
  }
  return { ok: false, error: 'Unknown plugin' }
}
