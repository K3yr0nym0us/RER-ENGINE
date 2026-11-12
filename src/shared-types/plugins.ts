export type PluginId = 'local-ai-assistant' | 'world-gen'

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

/** Artefacto binario de un plugin nativo (DLL) publicado en GitHub Releases. */
export interface PluginArtifactManifest {
  /** Nombre del archivo dentro del ZIP del release. */
  archiveInnerPath: string
  /** Nombre del binario una vez instalado (p. ej. `rer_worldgen.dll`). */
  executableName: string
  downloadUrl: string
  sizeBytes: number
  /** SHA-256 hexadecimal del ZIP (opcional: si está, se valida al instalar). */
  sha256?: string
}

export interface PluginCatalogEntry {
  id: PluginId
  name: string
  description: string
  version: string
  downloadSizeLabel: string
  /** `download`: se descarga de internet al instalar. `bundled`: viene con la app. */
  kind: 'download' | 'bundled'
  model?: PluginModelManifest
  llamaServer?: PluginLlamaServerManifest
  artifact?: PluginArtifactManifest
}

export interface PluginInstalledRecord {
  version: string
  modelPath?: string
  llamaServerPath?: string
  /** Ruta del binario nativo instalado (plugins con `artifact`). */
  libraryPath?: string
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

export type PluginDownloadPhase = 'msvc-redist' | 'llama-server' | 'extracting' | 'model' | 'artifact'

export interface PluginDownloadProgress {
  pluginId: PluginId
  phase: PluginDownloadPhase
  step: number
  stepsTotal: number
  percent: number
  overallPercent: number
  bytesReceived: number
  bytesTotal: number
  bytesOverallReceived: number
  bytesOverallTotal: number
}

export interface PluginInstallResult {
  ok: boolean
  error?: string
  cancelled?: boolean
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
