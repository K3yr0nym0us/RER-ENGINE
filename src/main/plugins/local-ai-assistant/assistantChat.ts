import http from 'http'

import type {
  AssistantChatDebugInfo,
  AssistantChatMessage,
  AssistantChatResponse,
  PluginUiAction,
} from '../../../shared-types/plugins'
import { buildSystemContext } from './editorDocsIndex'
import { polishAssistantReply } from './assistantReplyFormat'
import { aiLog, getAiAssistantLogFilePath } from './assistantChatLog'
import { getLlamaServerPort, isLlamaServerRunning } from './llamaServerProcess'

export interface AssistantToolResult {
  toolName: string
  result: string
}

export type UiActionEmitter = (action: PluginUiAction) => void

const THINK_CLOSE = '</' + 'think' + '>'

/** Quita bloques de razonamiento visibles; conserva la respuesta final. */
export function stripThinkingFromResponse(text: string): string {
  let s = text.trim()
  if (!s) return ''

  s = s.replace(/<think>[\s\S]*?<\/redacted_thinking>\s*/gi, '')
  s = s.replace(/<think(?:ing)?>[\s\S]*?<\/think(?:ing)?>\s*/gi, '')

  s = s.replace(/<think>[\s\S]*?(?=\n\s*\d+\.\s)/i, '')

  s = s.replace(/<\/redacted_thinking>\s*/gi, '')
  s = s.replace(/<\/think(?:ing)?>\s*/gi, '')

  const thinkClose = s.lastIndexOf(THINK_CLOSE)
  if (thinkClose !== -1) {
    s = s.slice(thinkClose + THINK_CLOSE.length)
  }

  return s.trim()
}

const KNOWN_ACCORDION_KEYS_LIST = [
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

const KNOWN_ACCORDION_KEYS = KNOWN_ACCORDION_KEYS_LIST.join('|')

/** Cualquier valor tras OPEN_ACCORDION:/HIGHLIGHT: (el modelo inventa formatos). */
const ANY_OPEN_ACCORDION_RE = /OPEN_ACCORDION\s*[:=]\s*(\S+)/gi
const ANY_HIGHLIGHT_RE = /HIGHLIGHT\s*[:=]\s*(\S+)/gi

/** Claves reales de `eventKey` en el sidebar (react-bootstrap Accordion). */
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
      continue
    }

    // El modelo a veces pone ids HIGHLIGHT (accordion-scenes) en OPEN_ACCORDION
    push({ type: 'highlight_ui_target', targetId: raw })
  }

  for (const m of content.matchAll(ANY_HIGHLIGHT_RE)) {
    const targetId = m[1]?.trim()
    if (targetId) push({ type: 'highlight_ui_target', targetId })
  }

  return actions
}

/** Quita etiquetas OPEN_ACCORDION/HIGHLIGHT antes de mostrar texto al usuario. */
function stripUiControlTags(text: string): string {
  let s = text
    .replace(/^\s*OPEN_ACCORDION\s*[:=]\s*\S+\s*$/gim, '')
    .replace(/^\s*HIGHLIGHT\s*[:=]\s*\S+\s*$/gim, '')
    .replace(/OPEN_ACCORDION\s*[:=]\s*\S+/gi, '')
    .replace(/HIGHLIGHT\s*[:=]\s*\S+/gi, '')
    .replace(/\|\s*HIGHLIGHT\s*[:=]\s*\S*/gi, '')

  // Restos del legend del prompt: "scenes |", "-scenes |", "scenes=Escenas |"
  s = s.replace(
    new RegExp(
      `(?:^|\\n)\\s*-?\\s*(?:${KNOWN_ACCORDION_KEYS})(?:\\s*=\\s*[^\\n|]+)?\\s*\\|?\\s*(?=\\n|$)`,
      'gim',
    ),
    '',
  )
  s = s.replace(
    new RegExp(`\\s+-?\\s*(?:${KNOWN_ACCORDION_KEYS})\\s*\\|?\\s*$`, 'im'),
    '',
  )
  s = s.replace(/^\s*\|\s*$/gm, '')
  s = s.replace(/\s+\|\s*$/gm, '')

  return s.trim()
}

interface ChatCompletionMessage {
  content?: string | null
  reasoning_content?: string | null
  [key: string]: unknown
}

interface ChatCompletionResult {
  rawAnswer: string
  contentField: string
  reasoningField: string
  httpStatus: number
  messageKeys: string[]
}

function preview(text: string, max = 280): string {
  const t = text.trim()
  if (!t) return '(empty)'
  if (t.length <= max) return t
  return `${t.slice(0, max)}…`
}

function extractAnswerFromCompletion(msg: ChatCompletionMessage | undefined): ChatCompletionResult {
  const contentField = (msg?.content ?? '').trim()
  const reasoningField = (msg?.reasoning_content ?? '').trim()
  const messageKeys = msg ? Object.keys(msg) : []

  aiLog('completion.message keys', { messageKeys, contentLength: contentField.length, reasoningLength: reasoningField.length })

  if (contentField) {
    return {
      rawAnswer: contentField,
      contentField,
      reasoningField,
      httpStatus: 200,
      messageKeys,
    }
  }

  if (reasoningField) {
    aiLog('completion WARN: content empty but reasoning_content present')
    const stripped = stripThinkingFromResponse(reasoningField)
    if (stripped.length > 0) {
      aiLog('completion fallback: using stripped reasoning_content', {
        strippedLength: stripped.length,
        strippedPreview: preview(stripped),
      })
      return {
        rawAnswer: stripped,
        contentField,
        reasoningField,
        httpStatus: 200,
        messageKeys,
      }
    }
  }

  return {
    rawAnswer: '',
    contentField,
    reasoningField,
    httpStatus: 200,
    messageKeys,
  }
}

function postChatCompletion(messages: AssistantChatMessage[]): Promise<ChatCompletionResult> {
  const port = getLlamaServerPort()
  const body = JSON.stringify({
    model: 'qwen3',
    messages,
    temperature: 0.5,
    max_tokens: 480,
  })

  aiLog('chat.request', {
    port,
    messageCount: messages.length,
    lastUserPreview: preview([...messages].reverse().find((m) => m.role === 'user')?.content ?? ''),
  })

  return new Promise((resolve, reject) => {
    const req = http.request(
      {
        hostname: '127.0.0.1',
        port,
        path: '/v1/chat/completions',
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Content-Length': Buffer.byteLength(body),
        },
        timeout: 120_000,
      },
      (res) => {
        let data = ''
        res.on('data', (chunk) => {
          data += chunk
        })
        res.on('end', () => {
          try {
            aiLog('chat.http', { status: res.statusCode, bodyLength: data.length, bodyPreview: preview(data, 400) })

            const parsed = JSON.parse(data) as {
              choices?: Array<{ message?: ChatCompletionMessage; finish_reason?: string }>
              error?: { message?: string }
            }

            if (parsed.error?.message) {
              reject(new Error(parsed.error.message))
              return
            }

            const choice = parsed.choices?.[0]
            aiLog('chat.choice', {
              finishReason: choice?.finish_reason,
              hasMessage: Boolean(choice?.message),
            })

            const extracted = extractAnswerFromCompletion(choice?.message)
            extracted.httpStatus = res.statusCode ?? 200
            resolve(extracted)
          } catch (err) {
            aiLog('chat.parse-error', { error: err instanceof Error ? err.message : String(err), bodyPreview: preview(data, 400) })
            reject(err)
          }
        })
      },
    )

    req.on('error', (err) => {
      aiLog('chat.http-error', { error: err.message })
      reject(err)
    })
    req.on('timeout', () => {
      req.destroy()
      aiLog('chat.timeout')
      reject(new Error('Chat request timed out'))
    })
    req.write(body)
    req.end()
  })
}

const ACCORDION_FALLBACK_HINTS: Record<string, string> = {
  scenes:
    'Abre el acordeón Scenes (Escenas) en el panel izquierdo. Ahí puedes crear, renombrar y cambiar de escena activa.',
  resources:
    'Abre el acordeón Resources (Recursos) para importar modelos, sprites, fuentes, sonidos e imágenes HUD.',
  entities:
    'Abre el acordeón Create entity (Crear entidad) para añadir personajes, objetos y otros elementos a la escena.',
  ui: 'Abre el acordeón User interface (Interfaz) para editar pantallas HUD del jugador.',
  herramientas:
    'Abre el acordeón Herramientas y pulsa Construcción rápida; en el modal Construcción elige la blueprint y luego coloca en el viewport.',
  controles: 'Abre el acordeón Controls (Controles) para configurar el mapeo de teclas y gamepad.',
  mundo: 'Abre el acordeón World (Mundo) para ajustar iluminación, cielo y propiedades del entorno.',
  camera: 'Abre el acordeón Camera (Cámara) para configurar la vista y la cámara del editor.',
}

function buildFallbackFromActions(actions: PluginUiAction[]): string {
  const parts: string[] = []
  for (const action of actions) {
    if (action.type === 'open_sidebar_accordion') {
      const key = action.accordionKey.toLowerCase()
      parts.push(
        ACCORDION_FALLBACK_HINTS[key]
        ?? `Abre el acordeón "${action.accordionKey}" en el panel izquierdo del editor.`,
      )
    } else if (action.type === 'highlight_ui_target') {
      parts.push(`Te resalto el control "${action.targetId}" en el editor.`)
    }
  }
  return parts.join(' ') || 'Revisa el panel izquierdo del editor para esa opción.'
}

/** Si el modelo no emite OPEN_ACCORDION, inferir acordeón desde la pregunta del usuario. */
function inferAccordionFromUserQuery(query: string): (typeof SIDEBAR_ACCORDION_EVENT_KEYS)[number] | null {
  const q = query.toLowerCase().normalize('NFC')

  if (
    /renombrar|mover|rotar|f[ií]sica|colisi[oó]n|reemplazar modelo|eliminar|animaci[oó]n|script rhai|l[oó]gica visual|transform|resize|rename|move|rotate|physics|collision|replace model|delete/.test(
      q,
    )
    && !/crear (personaje|entidad|character|entity)|create (character|entity)/.test(q)
  ) {
    return null
  }

  if (/blueprint|plantilla/.test(q)) {
    if (/crear|convertir|create|convert/.test(q) && !/usar|uso|use|colocar|construcci/.test(q)) {
      return null
    }
    return 'herramientas'
  }

  if (/control|teclado|gamepad|mando|keyboard/.test(q)) return 'controles'
  if (/programar escena|scene program|scene script|scene logic/.test(q)) return 'scenes'
  if (/\bescena|\bscene/.test(q)) return 'scenes'
  if (/modelo|model|recurso|resource|sprite|\.glb|\.gltf|importar|load.*model/.test(q)) {
    return 'resources'
  }
  if (/personaje|character|entidad|entity|objeto|object/.test(q) && /crear|create|a[nñ]adir|add/.test(q)) {
    return 'entities'
  }
  if (/interfaz|hud|\bui\b|user interface/.test(q)) return 'ui'
  if (/herramienta|tool|construcci[oó]n r[aá]pida|quick build/.test(q)) return 'herramientas'
  if (/mundo|world|iluminaci[oó]n|lighting|cielo|sky/.test(q)) return 'mundo'
  if (/c[aá]mara|camera/.test(q)) return 'camera'

  return null
}

function mergeUiActions(
  fromModel: PluginUiAction[],
  userQuery: string,
): PluginUiAction[] {
  if (fromModel.length > 0) return fromModel

  const inferredKey = inferAccordionFromUserQuery(userQuery)
  if (!inferredKey) return fromModel

  aiLog('chat.inferred-accordion', {
    userQuery: preview(userQuery, 120),
    accordionKey: inferredKey,
  })
  return [{ type: 'open_sidebar_accordion', accordionKey: inferredKey }]
}

function prepareAssistantOutput(
  raw: string,
  locale: 'en' | 'es',
  userQuery = '',
): { displayText: string; uiActions: PluginUiAction[] } {
  const stripped = stripThinkingFromResponse(raw)
  const uiActions = mergeUiActions(parseToolCalls(stripped), userQuery)

  let displayText = stripUiControlTags(stripped)
    .replace(/\n{3,}/g, '\n\n')
    .trim()

  if (!displayText && uiActions.length > 0) {
    displayText = buildFallbackFromActions(uiActions)
    aiLog('chat.fallback-text', { reason: 'only-ui-tags', uiActions, displayText: preview(displayText) })
  }

  if (displayText) {
    displayText = polishAssistantReply(displayText, locale, userQuery)
  }

  return { displayText, uiActions }
}

function buildDebugInfo(
  completion: ChatCompletionResult,
  raw: string,
  cleaned: string,
): AssistantChatDebugInfo {
  return {
    httpStatus: completion.httpStatus,
    contentLength: completion.contentField.length,
    reasoningLength: completion.reasoningField.length,
    rawLength: raw.length,
    cleanedLength: cleaned.length,
    contentPreview: preview(completion.contentField),
    reasoningPreview: preview(completion.reasoningField),
    rawPreview: preview(raw),
    cleanedPreview: preview(cleaned),
    messageKeys: completion.messageKeys,
    logFile: getAiAssistantLogFilePath(),
  }
}

function resolveAssistantPreamble(locale: 'en' | 'es'): string {
  if (locale === 'es') {
    return 'Eres RER-AI, el asistente del editor RER-ENGINE.'
  }
  return 'You are RER-AI, the RER-ENGINE editor assistant.'
}

function resolveLocaleInstruction(locale: 'en' | 'es'): string {
  if (locale === 'es') {
    return 'Responde SIEMPRE en español. Lista pasos completos con nombres exactos de la UI.'
  }
  return 'Always reply in English. List complete steps with exact UI names.'
}

export async function runAssistantChat(
  userMessages: AssistantChatMessage[],
  emitUiAction?: UiActionEmitter,
  locale: 'en' | 'es' = 'en',
): Promise<AssistantChatResponse & { uiActions?: PluginUiAction[]; debug?: AssistantChatDebugInfo }> {
  const serverUp = await isLlamaServerRunning()
  if (!serverUp) {
    aiLog('chat.aborted', { reason: 'llama-server not running' })
    return { ok: false, error: 'AI server is not running. Wait for startup or re-enable the plugin.' }
  }

  const lastUser = [...userMessages].reverse().find((m) => m.role === 'user')
  const systemContent = buildSystemContext(lastUser?.content ?? '', locale)

  const messages: AssistantChatMessage[] = [
    {
      role: 'system',
      content: [
        resolveAssistantPreamble(locale),
        resolveLocaleInstruction(locale),
        systemContent,
      ].join('\n\n'),
    },
    ...userMessages.filter((m) => m.role !== 'system'),
  ]

  try {
    const completion = await postChatCompletion(messages)
    const raw = completion.rawAnswer
    const userQuery = lastUser?.content ?? ''
    const { displayText, uiActions } = prepareAssistantOutput(raw, locale, userQuery)
    const debug = buildDebugInfo(completion, raw, displayText)

    aiLog('chat.result', { ...debug, uiActionCount: uiActions.length })

    if (!displayText) {
      return {
        ok: false,
        error: `Empty response (see log: ${debug.logFile})`,
        debug,
      }
    }

    if (emitUiAction) {
      for (const action of uiActions) {
        emitUiAction(action)
      }
    }

    return { ok: true, content: displayText, uiActions, debug }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    aiLog('chat.failed', { error: message })
    return { ok: false, error: message }
  }
}
