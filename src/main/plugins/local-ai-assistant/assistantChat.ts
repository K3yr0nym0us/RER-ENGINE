import type {
  AssistantChatDebugInfo,
  AssistantChatDelta,
  AssistantChatMessage,
  AssistantChatRequest,
  AssistantChatResponse,
  PluginUiAction,
} from '../../../shared-types/plugins'
import {
  ASSISTANT_PLUGIN_ID,
  getUiActionsLogPath,
  getWorkspaceDir,
} from './openCodePaths'
import { getAssistantPreferences } from '../pluginState'
import { polishAssistantReply } from './assistantReplyFormat'
import { aiLog, getAiAssistantLogFilePath } from './assistantChatLog'
import {
  getOpenCodeEndpoint,
  getOpenCodeServerStatus,
  isOpenCodeServerRunning,
} from './openCodeRuntime'
import {
  createSession,
  interruptSession,
  isOpenCodeGoConnected,
  listModels,
  listSessionMessages,
  promptSession,
  switchSessionModel,
  type ApiSessionMessage,
} from './openCodeClient'

export type UiActionEmitter = (action: PluginUiAction) => void
export type ChatDeltaEmitter = (delta: AssistantChatDelta) => void

export interface RunAssistantChatContext {
  emitUiAction?: UiActionEmitter
  emitDelta?: ChatDeltaEmitter
}

const POLL_INTERVAL_MS = 800
const STABILITY_MS = 45_000
const MAX_TURN_MS = 240_000
const MODEL_CACHE_TTL_MS = 60_000
const MAX_SESSIONS = 8

const AGENTS_BY_LOCALE: Record<'en' | 'es', string> = {
  es: 'rer-assistant-es',
  en: 'rer-assistant-en',
}

interface SessionEntry {
  sessionID: string
  agent: string
  modelRef: string
}

const sessions = new Map<string, SessionEntry>()
const cancelledConversations = new Set<string>()
let modelCache: { at: number; models: Awaited<ReturnType<typeof listModels>> } | null = null

export function cancelAssistantChat(conversationId: string): void {
  cancelledConversations.add(conversationId)
}

export function resetAssistantSessions(): void {
  sessions.clear()
}

// ---------------------------------------------------------------------------
// Fallback: etiquetas OPEN_ACCORDION / HIGHLIGHT en el texto del modelo
// ---------------------------------------------------------------------------

const THINK_CLOSE = '</' + 'think' + '>'

export function stripThinkingFromResponse(text: string): string {
  let s = text.trim()
  if (!s) return ''

  s = s.replace(/胺[\s\S]*?<\/redacted_thinking>\s*/gi, '')
  s = s.replace(/<redacted_thinking>[\s\S]*?<\/redacted_thinking>\s*/gi, '')
  s = s.replace(/<think(?:ing)?>[\s\S]*?<\/think(?:ing)?>\s*/gi, '')
  s = s.replace(/<\/redacted_thinking>\s*/gi, '')
  s = s.replace(/<\/think(?:ing)?>\s*/gi, '')

  const thinkClose = s.lastIndexOf(THINK_CLOSE)
  if (thinkClose !== -1) {
    s = s.slice(thinkClose + THINK_CLOSE.length)
  }

  return s.trim()
}

const KNOWN_ACCORDION_KEYS = [
  'scenes',
  'mundo',
  'camera',
  'resources',
  'entities',
  'ui',
  'herramientas',
  'tools',
  'controles',
  'controls',
] as const

const ANY_OPEN_ACCORDION_RE = /OPEN_ACCORDION\s*[:=]\s*(\S+)/gi
const ANY_HIGHLIGHT_RE = /HIGHLIGHT\s*[:=]\s*(\S+)/gi

const SIDEBAR_ACCORDION_EVENT_KEYS = [
  'scenes',
  'mundo',
  'camera',
  'resources',
  'entities',
  'ui',
  'herramientas',
  'controles',
] as const

const ACCORDION_KEY_ALIASES: Record<string, (typeof SIDEBAR_ACCORDION_EVENT_KEYS)[number]> = {
  tools: 'herramientas',
  controls: 'controles',
}

function canonicalAccordionKey(raw: string): (typeof SIDEBAR_ACCORDION_EVENT_KEYS)[number] | null {
  const value = raw.trim().toLowerCase()
  const aliased = ACCORDION_KEY_ALIASES[value] ?? value
  if ((SIDEBAR_ACCORDION_EVENT_KEYS as readonly string[]).includes(aliased)) {
    return aliased as (typeof SIDEBAR_ACCORDION_EVENT_KEYS)[number]
  }
  const fromHighlightId = aliased.match(/^accordion-(.+)$/)
  if (fromHighlightId) {
    return canonicalAccordionKey(fromHighlightId[1])
  }
  return null
}

function parseToolCalls(content: string): PluginUiAction[] {
  const actions: PluginUiAction[] = []
  const seen = new Set<string>()

  const push = (action: PluginUiAction) => {
    const key =
      action.type === 'open_sidebar_accordion'
        ? `accordion:${action.accordionKey}`
        : `highlight:${action.targetId}`
    if (seen.has(key)) return
    seen.add(key)
    actions.push(action)
  }

  for (const m of content.matchAll(ANY_OPEN_ACCORDION_RE)) {
    const raw = m[1]?.trim()
    if (!raw) continue
    const accordionKey = canonicalAccordionKey(raw)
    if (accordionKey) {
      push({ type: 'open_sidebar_accordion', accordionKey })
    } else {
      push({ type: 'highlight_ui_target', targetId: raw })
    }
  }

  for (const m of content.matchAll(ANY_HIGHLIGHT_RE)) {
    const targetId = m[1]?.trim()
    if (targetId) push({ type: 'highlight_ui_target', targetId })
  }

  return actions
}

function stripUiControlTags(text: string): string {
  let s = text
    .replace(/^\s*OPEN_ACCORDION\s*[:=]\s*\S+\s*$/gim, '')
    .replace(/^\s*HIGHLIGHT\s*[:=]\s*\S+\s*$/gim, '')
    .replace(/OPEN_ACCORDION\s*[:=]\s*\S+/gi, '')
    .replace(/HIGHLIGHT\s*[:=]\s*\S+/gi, '')
    .replace(/\|\s*HIGHLIGHT\s*[:=]\s*\S*/gi, '')

  s = s.replace(
    new RegExp(`(?:^|\\n)\\s*-?\\s*(?:${KNOWN_ACCORDION_KEYS.join('|')})(?:\\s*=\\s*[^\\n|]+)?\\s*\\|?\\s*(?=\\n|$)`, 'gim'),
    '',
  )
  s = s.replace(/^\s*\|\s*$/gm, '')
  s = s.replace(/\s+\|\s*$/gm, '')

  return s.trim()
}

// ---------------------------------------------------------------------------
// Resolución de modelo y sesión
// ---------------------------------------------------------------------------

function parseModelRef(ref: string): { providerID: string; id: string } | null {
  const slash = ref.indexOf('/')
  if (slash <= 0 || slash >= ref.length - 1) return null
  return { providerID: ref.slice(0, slash), id: ref.slice(slash + 1) }
}

async function listModelsCached(getEndpoint: () => ReturnType<typeof getOpenCodeEndpoint>) {
  const now = Date.now()
  if (modelCache && now - modelCache.at < MODEL_CACHE_TTL_MS) return modelCache.models
  const endpoint = getEndpoint()
  if (!endpoint) return { ok: false as const, error: 'OpenCode server unavailable' }
  const result = await listModels(endpoint)
  if (result.ok) modelCache = { at: now, models: result }
  return result
}

function invalidateModelCache(): void {
  modelCache = null
}

async function resolveModel(
  requestedRef: string | undefined,
): Promise<{ providerID: string; id: string } | { error: string }> {
  const prefs = getAssistantPreferences(ASSISTANT_PLUGIN_ID)
  const candidate = requestedRef ?? prefs.selectedModel

  const catalog = await listModelsCached(getOpenCodeEndpoint)
  const models = catalog.ok ? catalog.models ?? [] : []

  if (candidate) {
    const parsed = parseModelRef(candidate)
    if (parsed && models.some((model) => model.ref === candidate)) return parsed
    if (parsed && !catalog.ok) {
      // Sin catálogo no podemos verificar; confiamos en la preferencia.
      return parsed
    }
  }

  const firstFree = models.find((model) => model.free)
  if (firstFree) {
    const parsed = parseModelRef(firstFree.ref)
    if (parsed) return parsed
  }

  if (catalog.ok && models.length === 0) {
    return { error: 'No assistant models available. Connect OpenCode Go or try later.' }
  }
  if (!catalog.ok) {
    return { error: catalog.error ?? 'Model catalog unavailable' }
  }
  return { error: 'No assistant model available' }
}

async function acquireSession(
  conversationId: string,
  agent: string,
  modelRef: string,
): Promise<{ ok: true; sessionID: string } | { ok: false; error: string }> {
  const endpoint = getOpenCodeEndpoint()
  if (!endpoint) return { ok: false, error: 'OpenCode server unavailable' }

  const existing = sessions.get(conversationId)
  if (existing && existing.agent === agent && existing.modelRef === modelRef) {
    // Reordena para LRU.
    sessions.delete(conversationId)
    sessions.set(conversationId, existing)
    return { ok: true, sessionID: existing.sessionID }
  }

  const model = parseModelRef(modelRef)
  if (!model) return { ok: false, error: `Invalid model reference: ${modelRef}` }

  // Cambio de modelo o de idioma dentro de una conversación viva: cambiar el
  // modelo mantiene el contexto; cambiar de agente exige sesión nueva.
  if (existing && existing.agent === agent) {
    const switched = await switchSessionModel(endpoint, existing.sessionID, model)
    if (switched.ok) {
      sessions.delete(conversationId)
      sessions.set(conversationId, { ...existing, modelRef })
      return { ok: true, sessionID: existing.sessionID }
    }
  }

  const created = await createSession(endpoint, {
    directory: getWorkspaceDir(),
    agent,
    model: { providerID: model.providerID, id: model.id },
  })
  if (!created.ok || !created.session) {
    return { ok: false, error: created.error ?? 'Failed to create assistant session' }
  }

  sessions.set(conversationId, { sessionID: created.session.id, agent, modelRef })
  while (sessions.size > MAX_SESSIONS) {
    const oldest = sessions.keys().next().value
    if (oldest === undefined) break
    sessions.delete(oldest)
  }

  aiLog('chat.session-created', { conversationId, sessionID: created.session.id, agent, modelRef })
  return { ok: true, sessionID: created.session.id }
}

// ---------------------------------------------------------------------------
// Ejecución del turno (polling de mensajes con streaming incremental)
// ---------------------------------------------------------------------------

interface TurnObservation {
  steps: number
  toolCalls: number
  lastText: string
}

function messageText(message: ApiSessionMessage): string {
  return (message.content ?? [])
    .filter((part) => part.type === 'text' && typeof part.text === 'string')
    .map((part) => part.text ?? '')
    .join('\n')
}

function actionsFromToolCall(name: string, input: unknown): PluginUiAction[] {
  if (name === 'rer_ui_open_accordion') {
    const accordionKey =
      typeof input === 'object' && input !== null && 'accordionKey' in input
        ? String((input as { accordionKey: unknown }).accordionKey)
        : ''
    if (accordionKey) return [{ type: 'open_sidebar_accordion', accordionKey }]
    return []
  }
  if (name === 'rer_ui_highlight') {
    const targetId =
      typeof input === 'object' && input !== null && 'targetId' in input
        ? String((input as { targetId: unknown }).targetId)
        : ''
    if (targetId) return [{ type: 'highlight_ui_target', targetId }]
    return []
  }
  return []
}

async function runTurn(
  conversationId: string,
  sessionID: string,
  userText: string,
  context: RunAssistantChatContext,
): Promise<{ ok: boolean; observation?: TurnObservation; error?: string }> {
  const endpoint = getOpenCodeEndpoint()
  if (!endpoint) return { ok: false, error: 'OpenCode server unavailable' }

  const before = await listSessionMessages(endpoint, sessionID)
  const baselineAssistantCount = before.ok
    ? (before.messages ?? []).filter((message) => message.type === 'assistant').length
    : 0

  const sent = await promptSession(endpoint, sessionID, userText)
  if (!sent.ok) return { ok: false, error: sent.error ?? 'Failed to send prompt' }

  const emittedTextLengths = new Map<string, number>()
  const emittedActions = new Set<string>()
  const observation: TurnObservation = { steps: 0, toolCalls: 0, lastText: '' }
  let lastGrowthAt = Date.now()
  const startedAt = Date.now()
  let lastCount = baselineAssistantCount

  while (Date.now() - startedAt < MAX_TURN_MS) {
    if (cancelledConversations.has(conversationId)) {
      await interruptSession(endpoint, sessionID)
      cancelledConversations.delete(conversationId)
      return { ok: false, error: 'Chat cancelled' }
    }

    const listing = await listSessionMessages(endpoint, sessionID)
    if (listing.ok) {
      const messages = listing.messages ?? []
      const assistant = messages.filter((message) => message.type === 'assistant')
      const turn = assistant.slice(baselineAssistantCount)
      observation.steps = turn.length

      if (turn.length !== lastCount) {
        lastCount = turn.length
        lastGrowthAt = Date.now()
      }

      for (const message of turn) {
        // Tool calls nuevas → acción de UI + chip.
        for (const part of message.content ?? []) {
          if (part.type !== 'tool' || !part.name) continue
          if (!part.name.startsWith('rer_ui_')) continue
          const actionKey = `${message.id}:${part.name}:${JSON.stringify(part.state?.input ?? {})}`
          if (emittedActions.has(actionKey)) continue
          emittedActions.add(actionKey)
          observation.toolCalls += 1
          const actions = actionsFromToolCall(part.name, part.state?.input)
          for (const action of actions) {
            context.emitUiAction?.(action)
            context.emitDelta?.({
              conversationId,
              kind: 'tool',
              toolName: part.name,
              toolInput: (part.state?.input ?? {}) as Record<string, unknown>,
            })
          }
        }

        // Texto incremental.
        const fullText = messageText(message)
        const emitted = emittedTextLengths.get(message.id) ?? 0
        if (fullText.length > emitted) {
          const delta = fullText.slice(emitted)
          emittedTextLengths.set(message.id, fullText.length)
          if (delta.trim()) context.emitDelta?.({ conversationId, kind: 'text', delta })
        }
      }

      const last = turn[turn.length - 1]
      if (last) {
        const turnText = turn
          .map((message) => messageText(message))
          .filter(Boolean)
          .join('\n\n')
        observation.lastText = turnText
        if (last.finish === 'stop') return { ok: true, observation }
      }

      const noGrowthMs = Date.now() - lastGrowthAt
      if (noGrowthMs > STABILITY_MS && turn.length > 0) {
        aiLog('chat.stability-finalize', { conversationId, noGrowthMs, steps: turn.length })
        return { ok: true, observation }
      }
    }

    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS))
  }

  aiLog('chat.max-duration-finalize', { conversationId, startedAt, now: Date.now() })
  if (observation.lastText) return { ok: true, observation }
  return { ok: false, error: 'Assistant response timed out' }
}

// ---------------------------------------------------------------------------
// API pública
// ---------------------------------------------------------------------------

function buildDebugInfo(input: {
  sessionID?: string
  modelRef?: string
  agent?: string
  steps: number
  toolCalls: number
}): AssistantChatDebugInfo {
  return {
    logFile: getAiAssistantLogFilePath(),
    sessionID: input.sessionID,
    model: input.modelRef,
    agent: input.agent,
    steps: input.steps,
    toolCalls: input.toolCalls,
  }
}

export async function runAssistantChat(
  request: AssistantChatRequest,
  context: RunAssistantChatContext = {},
  locale: 'en' | 'es' = 'en',
): Promise<AssistantChatResponse & { uiActions?: PluginUiAction[]; debug?: AssistantChatDebugInfo }> {
  const serverUp = await isOpenCodeServerRunning()
  const { status } = getOpenCodeServerStatus()
  if (!serverUp && status !== 'running') {
    aiLog('chat.aborted', { reason: 'opencode server not running', status })
    return { ok: false, error: 'AI assistant is not running. Wait for startup or re-enable the plugin.' }
  }

  const lastUser = [...request.messages].reverse().find((message) => message.role === 'user')
  const userText = lastUser?.content?.trim()
  if (!userText) {
    return { ok: false, error: 'Empty request' }
  }

  const conversationId = request.conversationId?.trim() || 'default'
  const agent = AGENTS_BY_LOCALE[locale] ?? AGENTS_BY_LOCALE.en

  try {
    const model = await resolveModel(request.model)
    if ('error' in model) {
      return { ok: false, error: model.error }
    }

    const session = await acquireSession(conversationId, agent, `${model.providerID}/${model.id}`)
    if (!session.ok) {
      return { ok: false, error: session.error }
    }

    invalidateModelCache()

    const result = await runTurn(conversationId, session.sessionID, userText, context)

    if (!result.ok || !result.observation) {
      const error = result.error ?? 'Assistant chat failed'
      aiLog('chat.failed', { conversationId, error })
      context.emitDelta?.({ conversationId, kind: 'error', message: error })
      return { ok: false, error }
    }

    const raw = result.observation.lastText
    const stripped = stripThinkingFromResponse(raw)
    const fromTags = parseToolCalls(stripped)
    if (context.emitUiAction) {
      for (const action of dedupeActions(fromTags)) context.emitUiAction(action)
    }

    let displayText = stripUiControlTags(stripped).replace(/\n{3,}/g, '\n\n').trim()
    if (displayText) {
      displayText = polishAssistantReply(displayText, locale, userText)
    }

    const debug = buildDebugInfo({
      sessionID: session.sessionID,
      modelRef: `${model.providerID}/${model.id}`,
      agent,
      steps: result.observation.steps,
      toolCalls: result.observation.toolCalls,
    })
    aiLog('chat.result', { ...debug, textLength: displayText.length })

    if (!displayText) {
      const error = `Empty response (see log: ${debug.logFile})`
      context.emitDelta?.({ conversationId, kind: 'error', message: error })
      return { ok: false, error, debug }
    }

    context.emitDelta?.({ conversationId, kind: 'done', message: displayText })
    return { ok: true, content: displayText, uiActions: fromTags, debug }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    aiLog('chat.exception', { conversationId, error: message })
    context.emitDelta?.({ conversationId, kind: 'error', message })
    return { ok: false, error: message }
  }
}

function dedupeActions(actions: PluginUiAction[]): PluginUiAction[] {
  const seen = new Set<string>()
  const out: PluginUiAction[] = []
  for (const action of actions) {
    const key =
      action.type === 'open_sidebar_accordion'
        ? `accordion:${action.accordionKey}`
        : `highlight:${action.targetId}`
    if (seen.has(key)) continue
    seen.add(key)
    out.push(action)
  }
  return out
}

/** Indica si hay un token de OpenCode Go activo (para estados de UI). */
export async function assistantGoConnected(): Promise<boolean> {
  const endpoint = getOpenCodeEndpoint()
  if (!endpoint) return false
  return isOpenCodeGoConnected(endpoint)
}

/** Ruta del log de acciones de UI (diagnóstico). */
export function getAssistantUiActionsLogPath(): string {
  return getUiActionsLogPath()
}

export type { AssistantChatMessage }
