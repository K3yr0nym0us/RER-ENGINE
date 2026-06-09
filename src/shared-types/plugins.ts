export type PluginId = 'local-ai-assistant'

export interface PluginModelManifest {
  repo: string
  repoUrl: string
  filename: string
  downloadUrl: string
  sizeBytes: number
  license: string
}

export interface PluginLlamaServerManifest {
  downloadUrl: string
  archiveInnerPath: string
  executableName: string
  sizeBytes: number
}

export interface PluginCatalogEntry {
  id: PluginId
  name: string
  description: string
  version: string
  downloadSizeLabel: string
  model: PluginModelManifest
  llamaServer: PluginLlamaServerManifest
}

export interface PluginInstalledRecord {
  version: string
  modelPath?: string
  llamaServerPath?: string
  installedAt: string
}

export interface PluginsState {
  installed: Partial<Record<PluginId, PluginInstalledRecord>>
  enabled: PluginId[]
}

export type PluginLlmStatus =
  | 'idle'
  | 'downloading'
  | 'ready'
  | 'starting'
  | 'running'
  | 'error'

export interface PluginDownloadProgress {
  pluginId: PluginId
  phase: 'llama-server' | 'model'
  percent: number
  bytesReceived: number
  bytesTotal: number
}

export interface PluginInstallResult {
  ok: boolean
  error?: string
}

export interface AssistantChatMessage {
  role: 'user' | 'assistant' | 'system'
  content: string
}

export interface AssistantChatRequest {
  messages: AssistantChatMessage[]
  /** Idioma del editor (en | es); las respuestas del modelo deben coincidir. */
  locale?: 'en' | 'es'
}

export interface AssistantChatDebugInfo {
  httpStatus?: number
  contentLength: number
  reasoningLength: number
  rawLength: number
  cleanedLength: number
  contentPreview: string
  reasoningPreview: string
  rawPreview: string
  cleanedPreview: string
  messageKeys: string[]
  logFile: string
}

export interface AssistantChatResponse {
  ok: boolean
  content?: string
  error?: string
  debug?: AssistantChatDebugInfo
}

export type PluginUiAction =
  | { type: 'open_sidebar_accordion'; accordionKey: string }
  | { type: 'highlight_ui_target'; targetId: string }
