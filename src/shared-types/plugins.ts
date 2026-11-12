export type PluginId = 'local-ai-assistant'

export interface PluginOpenCodeManifest {
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
  openCode: PluginOpenCodeManifest
}

export interface PluginInstalledRecord {
  version: string
  opencodePath?: string
  workspaceDir?: string
  installedAt: string
}

export interface PluginAssistantPreferences {
  /** Modelo elegido por el usuario, en formato `provider/model`. */
  selectedModel?: string
  /** True cuando hay un token de OpenCode Go conectado en el perfil aislado. */
  goConnected?: boolean
}

export interface PluginsState {
  installed: Partial<Record<PluginId, PluginInstalledRecord>>
  enabled: PluginId[]
  preferences?: Partial<Record<PluginId, PluginAssistantPreferences>>
}

export type PluginLlmStatus =
  | 'idle'
  | 'downloading'
  | 'ready'
  | 'starting'
  | 'running'
  | 'error'

export type PluginDownloadPhase = 'opencode' | 'extracting' | 'preparing'

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
  /** Id de conversación del overlay para reusar la sesión de OpenCode. */
  conversationId?: string
  /** Modelo explícito `provider/model`; si falta se usa la preferencia guardada. */
  model?: string
}

export interface AssistantChatDebugInfo {
  logFile: string
  sessionID?: string
  model?: string
  agent?: string
  steps: number
  toolCalls: number
}

export interface AssistantChatResponse {
  ok: boolean
  content?: string
  error?: string
  debug?: AssistantChatDebugInfo
}

export interface AssistantModelOption {
  /** `provider/model`, p. ej. `opencode/deepseek-v4.1-flash-free`. */
  ref: string
  providerID: string
  modelID: string
  name: string
  contextTokens: number
  free: boolean
}

export interface AssistantModelsResult {
  ok: boolean
  models?: AssistantModelOption[]
  error?: string
}

export interface AssistantSettings {
  selectedModel: string | null
  goConnected: boolean
  /** True cuando la cuenta de OpenCode (fallback free) está conectada. */
  opencodeConnected?: boolean
}

export interface AssistantGoConnectResult {
  ok: boolean
  error?: string
}

export type PluginUiAction =
  | { type: 'open_sidebar_accordion'; accordionKey: string }
  | { type: 'highlight_ui_target'; targetId: string }

/** Evento de streaming del chat (main → overlays). */
export interface AssistantChatDelta {
  conversationId: string
  kind: 'text' | 'tool' | 'done' | 'error'
  /** Fragmento de texto incremental (kind: text). */
  delta?: string
  /** Tool ejecutada (kind: tool). */
  toolName?: string
  toolInput?: Record<string, unknown>
  /** Mensaje final (kind: done | error). */
  message?: string
}
