import http from 'http'

import type {
  AssistantChatDebugInfo,
  AssistantChatMessage,
  AssistantChatResponse,
  PluginUiAction,
} from '../../shared-types/plugins'
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

function parseToolCalls(content: string): PluginUiAction[] {
  const actions: PluginUiAction[] = []
  const accordionMatch = content.match(/OPEN_ACCORDION:(\w+)/gi)
  if (accordionMatch) {
    for (const m of accordionMatch) {
      const key = m.split(':')[1]?.toLowerCase()
      if (key) actions.push({ type: 'open_sidebar_accordion', accordionKey: key })
    }
  }
  const highlightMatch = content.match(/HIGHLIGHT:(\S+)/gi)
  if (highlightMatch) {
    for (const m of highlightMatch) {
      const targetId = m.split(':')[1]
      if (targetId) actions.push({ type: 'highlight_ui_target', targetId })
    }
  }
  return actions
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
    'Abre el acordeón Herramientas y pulsa Construcción Rápida; en el modal Construcción elige la blueprint y luego coloca en el viewport.',
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

function prepareAssistantOutput(
  raw: string,
  locale: 'en' | 'es',
  userQuery = '',
): { displayText: string; uiActions: PluginUiAction[] } {
  const stripped = stripThinkingFromResponse(raw)
  const uiActions = parseToolCalls(stripped)

  let displayText = stripped
    .replace(/OPEN_ACCORDION:\w+/gi, '')
    .replace(/HIGHLIGHT:\S+/gi, '')
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

const REPLY_FORMAT_RULES = [
  'Answer as numbered steps (1. 2. 3. …) for workflows. One step per line.',
  'Leave a blank line between steps.',
  'Complete ALL steps of the workflow — never stop mid-answer.',
  'Use exact UI names from the guide (bold/emojis are applied automatically).',
  'No reasoning tags. OPEN_ACCORDION / HIGHLIGHT tags on a separate line after the answer.',
].join('\n')

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
        'You are RER-AI, the RER-ENGINE editor assistant.',
        resolveLocaleInstruction(locale),
        REPLY_FORMAT_RULES,
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
