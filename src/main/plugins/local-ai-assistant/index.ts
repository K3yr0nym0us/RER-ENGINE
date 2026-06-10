export { runAssistantChat, stripThinkingFromResponse } from './assistantChat'
export type { AssistantToolResult, UiActionEmitter } from './assistantChat'
export { polishAssistantReply } from './assistantReplyFormat'
export { aiLog, getAiAssistantLogFilePath } from './assistantChatLog'
export { buildSystemContext, refreshAiEditorGuideCache } from './editorDocsIndex'
export {
  autoStartLocalAiAssistant,
  registerLocalAiAssistantIpc,
} from './ipc'
export {
  cancelPluginInstall as cancelLocalAiInstall,
  installPlugin as installLocalAiAssistant,
  isPluginInstallInProgress,
  uninstallPlugin as uninstallLocalAiAssistant,
} from './install'
export {
  getLlamaServerPort,
  getLlamaServerStatus,
  isLlamaServerRunning,
  probeLlamaServerHttp,
  refreshLlamaServerReachability,
  startLlamaServer,
  stopLlamaServer,
} from './llamaServerProcess'
export {
  downloadAndExtractLlamaRuntime,
  ensureLlamaRuntime,
  extractLlamaRuntimeZip,
  formatWindowsExitCode,
  getLlamaBinDir,
  isLlamaRuntimeComplete,
} from './llamaRuntime'
