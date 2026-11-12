import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'

import type {
  AssistantChatDelta,
  AssistantModelOption,
  AssistantSettings,
} from '../../../shared-types/plugins'
import rerAiIcon from '../../../resources/RER-AI.png'
import rerAiPointingIcon from '../../../resources/RER-AI-POINTING.png'
import rerAiThinkingIcon from '../../../resources/RER-AI-THINKING.png'
import { LanguageProvider, useLanguage } from '../context/LanguageContext'
import { useTraslate } from '@hooks'
import { AssistantReplyText } from './AssistantReplyText'

interface ChatLine {
  id: string
  role: 'user' | 'assistant'
  content: string
}

interface ToolChip {
  id: string
  label: string
}

const CLICK_MAX_MOVE_PX = 8
const DRAG_ARM_MOVE_PX = 4
const DRAG_HOLD_MS = 160

type FabPointerSession = {
  pointerId: number
  startX: number
  startY: number
  armed: boolean
  target: HTMLElement
}

function newConversationId(): string {
  try {
    if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
      return crypto.randomUUID()
    }
  } catch {
    // fallback below
  }
  return `conv-${Date.now()}-${Math.floor(Math.random() * 1e6)}`
}

function newLineId(): string {
  return `line-${Date.now()}-${Math.floor(Math.random() * 1e6)}`
}

function describeTool(label: string, input: Record<string, unknown>): string {
  const key = typeof input.accordionKey === 'string' ? input.accordionKey : ''
  const target = typeof input.targetId === 'string' ? input.targetId : ''
  if (label.includes('accordion') && key) return `accordion:${key}`
  if (label.includes('highlight') && target) return `highlight:${target}`
  return label
}

function useFabPressHandlers(options: { onShortClick: () => void }) {
  const optionsRef = useRef(options)
  optionsRef.current = options
  const sessionRef = useRef<FabPointerSession | null>(null)
  const holdTimerRef = useRef<number | null>(null)
  const [isDragging, setIsDragging] = useState(false)

  const clearHoldTimer = useCallback(() => {
    if (holdTimerRef.current != null) {
      window.clearTimeout(holdTimerRef.current)
      holdTimerRef.current = null
    }
  }, [])

  const finishSession = useCallback((armed: boolean) => {
    clearHoldTimer()
    const session = sessionRef.current
    if (session) {
      try {
        session.target.releasePointerCapture(session.pointerId)
      } catch {
        // ignore
      }
    }
    sessionRef.current = null
    setIsDragging(false)
    if (armed) {
      window.electronAPI.aiAssistantFabDragEnd()
    }
  }, [clearHoldTimer])

  const armDrag = useCallback((session: FabPointerSession) => {
    if (session.armed) return
    clearHoldTimer()
    session.armed = true
    setIsDragging(true)
    window.electronAPI.aiAssistantFabDragStart()
  }, [clearHoldTimer])

  const onWindowPointerMove = useCallback((e: PointerEvent) => {
    const session = sessionRef.current
    if (!session || e.pointerId !== session.pointerId || session.armed) return

    const dist = Math.hypot(e.screenX - session.startX, e.screenY - session.startY)
    if (dist >= DRAG_ARM_MOVE_PX) {
      armDrag(session)
    }
  }, [armDrag])

  const onWindowPointerEnd = useCallback((e: PointerEvent) => {
    const session = sessionRef.current
    if (!session || e.pointerId !== session.pointerId) return

    window.removeEventListener('pointermove', onWindowPointerMove)
    window.removeEventListener('pointerup', onWindowPointerEnd)
    window.removeEventListener('pointercancel', onWindowPointerEnd)

    const dist = Math.hypot(e.screenX - session.startX, e.screenY - session.startY)
    const armed = session.armed

    if (!armed && dist <= CLICK_MAX_MOVE_PX) {
      optionsRef.current.onShortClick()
    }

    finishSession(armed)
  }, [finishSession, onWindowPointerMove])

  const onFabPointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return
    e.preventDefault()

    const target = e.currentTarget
    target.setPointerCapture(e.pointerId)

    sessionRef.current = {
      pointerId: e.pointerId,
      startX: e.screenX,
      startY: e.screenY,
      armed: false,
      target,
    }

    window.addEventListener('pointermove', onWindowPointerMove)
    window.addEventListener('pointerup', onWindowPointerEnd)
    window.addEventListener('pointercancel', onWindowPointerEnd)

    holdTimerRef.current = window.setTimeout(() => {
      const active = sessionRef.current
      if (!active || active.pointerId !== e.pointerId) return
      armDrag(active)
    }, DRAG_HOLD_MS)
  }, [armDrag, onWindowPointerEnd, onWindowPointerMove])

  useEffect(() => () => {
    clearHoldTimer()
    window.removeEventListener('pointermove', onWindowPointerMove)
    window.removeEventListener('pointerup', onWindowPointerEnd)
    window.removeEventListener('pointercancel', onWindowPointerEnd)
    if (sessionRef.current?.armed) {
      window.electronAPI.aiAssistantFabDragEnd()
    }
  }, [clearHoldTimer, onWindowPointerEnd, onWindowPointerMove])

  return { isDragging, onFabPointerDown }
}

function Fab({
  variant,
  isDragging,
  onPointerDown,
  onKeyDown,
  label,
}: {
  variant: 'idle' | 'thinking' | 'pointing'
  isDragging: boolean
  onPointerDown: (e: React.PointerEvent<HTMLDivElement>) => void
  onKeyDown: (e: React.KeyboardEvent<HTMLDivElement>) => void
  label: string
}) {
  return (
    <div
      className={`ai-assistant-fab-wrap${isDragging ? ' ai-assistant-fab-wrap--dragging' : ''}`}
      data-plugin-target="ai-assistant-fab"
      role="button"
      tabIndex={0}
      aria-label={label}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
    >
      <div className="ai-assistant-fab" aria-hidden>
        <img
          src={rerAiIcon}
          alt=""
          className={`ai-assistant-fab-img${variant === 'idle' ? ' ai-assistant-fab-img--active' : ''}`}
          draggable={false}
        />
        <img
          src={rerAiThinkingIcon}
          alt=""
          className={`ai-assistant-fab-img${variant === 'thinking' ? ' ai-assistant-fab-img--active' : ''}`}
          draggable={false}
        />
        <img
          src={rerAiPointingIcon}
          alt=""
          className={`ai-assistant-fab-img${variant === 'pointing' ? ' ai-assistant-fab-img--active' : ''}`}
          draggable={false}
        />
      </div>
    </div>
  )
}

function StatusDot({ status }: { status: string }) {
  const className =
    status === 'running'
      ? 'ai-assistant-dot ai-assistant-dot--ok'
      : status === 'starting' || status === 'downloading'
        ? 'ai-assistant-dot ai-assistant-dot--busy'
        : status === 'error'
          ? 'ai-assistant-dot ai-assistant-dot--error'
          : 'ai-assistant-dot'
  return <span className={className} aria-hidden />
}

function ModelSelect({
  models,
  value,
  goConnected,
  onChange,
}: {
  models: AssistantModelOption[]
  value: string
  goConnected: boolean
  onChange: (next: string) => void
}) {
  const { t } = useTraslate()
  const free = models.filter((model) => model.free)
  const go = models.filter((model) => !model.free)

  return (
    <select
      className="ai-assistant-panel__model-select"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      title={t('Model')}
      aria-label={t('Model')}
    >
      <option value="">{t('Automatic')}</option>
      {free.length > 0 && (
        <optgroup label={t('Free models')}>
          {free.map((model) => (
            <option key={model.ref} value={model.ref}>{model.name}</option>
          ))}
        </optgroup>
      )}
      {go.length > 0 && (
        <optgroup label={t('OpenCode Go models')}>
          {go.map((model) => (
            <option key={model.ref} value={model.ref} disabled={!goConnected}>
              {model.name}
            </option>
          ))}
        </optgroup>
      )}
    </select>
  )
}

function AiAssistantOverlayInner() {
  const { t } = useTraslate()
  const { locale } = useLanguage()
  const [mode, setMode] = useState<'fab' | 'panel'>('fab')
  const [lines, setLines] = useState<ChatLine[]>([])
  const [streaming, setStreaming] = useState<string | null>(null)
  const [toolChips, setToolChips] = useState<ToolChip[]>([])
  const [input, setInput] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [llmStatus, setLlmStatus] = useState('idle')
  const [models, setModels] = useState<AssistantModelOption[]>([])
  const [settings, setSettings] = useState<AssistantSettings>({ selectedModel: null, goConnected: false })
  const conversationRef = useRef(newConversationId())
  const lastUserRef = useRef<string>('')
  const scrollRef = useRef<HTMLDivElement | null>(null)
  const busyRef = useRef(false)

  busyRef.current = busy

  const welcome = useMemo<ChatLine>(
    () => ({
      id: 'welcome',
      role: 'assistant',
      content: t("Hi! I'm RER-AI, your AI assistant. Ask me anything about the editor."),
    }),
    [t],
  )

  const refreshLlmStatus = useCallback(async () => {
    const status = await window.electronAPI.pluginsGetLlmStatus()
    setLlmStatus(status.status)
    if (status.status === 'running') {
      setError((prev) => (prev && prev.startsWith('[') ? null : prev))
    } else if (status.error) {
      setError(status.error)
    }
  }, [])

  const loadAssistantContext = useCallback(async () => {
    try {
      const [settingsResult, modelsResult] = await Promise.all([
        window.electronAPI.pluginsGetAssistantSettings(),
        window.electronAPI.pluginsListModels(),
      ])
      setSettings(settingsResult)
      if (modelsResult.ok) setModels(modelsResult.models ?? [])
    } catch {
      // sin estado aún (plugin no instalado)
    }
  }, [])

  useEffect(() => {
    void refreshLlmStatus()
    const id = window.setInterval(() => void refreshLlmStatus(), 2_000)
    return () => window.clearInterval(id)
  }, [refreshLlmStatus])

  useEffect(() => {
    if (llmStatus === 'running') void loadAssistantContext()
  }, [llmStatus, loadAssistantContext])

  useEffect(() => {
    const offDelta = window.electronAPI.onPluginsChatDelta((delta: AssistantChatDelta) => {
      if (delta.conversationId !== conversationRef.current) return
      if (delta.kind === 'text' && delta.delta) {
        setStreaming((prev) => (prev ?? '') + delta.delta)
      } else if (delta.kind === 'tool') {
        const label = describeTool(delta.toolName ?? '', delta.toolInput ?? {})
        setToolChips((prev) => {
          if (prev.some((chip) => chip.label === label)) return prev
          return [...prev, { id: `${label}-${prev.length}`, label }]
        })
      } else if (delta.kind === 'done') {
        setStreaming(null)
      } else if (delta.kind === 'error' && delta.message) {
        setStreaming(null)
        setError(delta.message)
      }
    })
    return offDelta
  }, [])

  useEffect(() => {
    window.electronAPI.aiAssistantSetLayout(mode === 'panel' ? 'panel' : 'fab')
  }, [mode])

  useEffect(() => {
    const node = scrollRef.current
    if (node) node.scrollTop = node.scrollHeight
  }, [lines, streaming, toolChips])

  const canSend = llmStatus === 'running' || llmStatus === 'starting'
  const open = mode === 'panel'

  const openPanel = useCallback(() => {
    setMode('panel')
    setLines((prev) => (prev.length === 0 ? [welcome] : prev))
    void refreshLlmStatus()
    void loadAssistantContext()
  }, [welcome, refreshLlmStatus, loadAssistantContext])

  const closePanel = useCallback(() => {
    setMode('fab')
  }, [])

  const startNewChat = useCallback(() => {
    conversationRef.current = newConversationId()
    setLines([welcome])
    setStreaming(null)
    setToolChips([])
    setError(null)
    setInput('')
  }, [welcome])

  const finishWithAssistantLine = useCallback((content: string) => {
    setStreaming(null)
    setLines((prev) => [...prev, { id: newLineId(), role: 'assistant', content }])
  }, [])

  const runChat = useCallback(
    async (userText: string) => {
      lastUserRef.current = userText
      setBusy(true)
      setStreaming('')
      setError(null)
      try {
        const history = [...lines, { id: newLineId(), role: 'user' as const, content: userText }]
        setLines(history)
        const result = await window.electronAPI.pluginsChat({
          messages: history
            .filter((line) => line.id !== 'welcome')
            .map((line) => ({ role: line.role, content: line.content })),
          locale,
          conversationId: conversationRef.current,
        })
        if (!result.ok) {
          setStreaming(null)
          setError(result.error ?? t('Chat failed'))
        } else {
          finishWithAssistantLine(result.content ?? '')
        }
      } catch (err) {
        setStreaming(null)
        setError(err instanceof Error ? err.message : String(err))
      } finally {
        setBusy(false)
      }
    },
    [lines, locale, t, finishWithAssistantLine],
  )

  const send = useCallback(() => {
    const text = input.trim()
    if (!text || busy || !open) return
    void runChat(text)
    setInput('')
  }, [input, busy, open, runChat])

  const retry = useCallback(() => {
    if (!lastUserRef.current || busy) return
    void runChat(lastUserRef.current)
  }, [busy, runChat])

  const stop = useCallback(() => {
    void window.electronAPI.pluginsChatCancel(conversationRef.current)
  }, [])

  const handleModelChange = useCallback(async (next: string) => {
    setSettings((prev) => ({ ...prev, selectedModel: next || null }))
    try {
      await window.electronAPI.pluginsSetModel(next || null)
    } catch {
      // ignore
    }
  }, [])

  const handleFabClick = useCallback(() => {
    if (open) closePanel()
    else openPanel()
  }, [open, openPanel, closePanel])

  const { isDragging, onFabPointerDown } = useFabPressHandlers({
    onShortClick: handleFabClick,
  })

  const fabVariant: 'idle' | 'thinking' | 'pointing' =
    busy || streaming != null
      ? 'thinking'
      : 'idle'

  const onKeyDownFab = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault()
        handleFabClick()
      }
    },
    [handleFabClick],
  )

  const modelLabel = (): string => {
    if (settings.selectedModel) {
      const found = models.find((model) => model.ref === settings.selectedModel)
      if (found) return found.name
      return settings.selectedModel
    }
    return t('Free')
  }

  const statusText = (): string => {
    if (llmStatus === 'running') return modelLabel()
    if (llmStatus === 'starting') return t('AI server is starting…')
    if (llmStatus === 'downloading') return t('Preparing download…')
    if (llmStatus === 'error') return t('OpenCode offline')
    return t('OpenCode idle')
  }

  const fab = (
    <Fab
      variant={fabVariant}
      isDragging={isDragging}
      onPointerDown={onFabPointerDown}
      onKeyDown={onKeyDownFab}
      label={t('Open AI assistant')}
    />
  )

  const panel = open ? (
    <div className="ai-assistant-panel">
      <div className="ai-assistant-panel__header">
        <img src={rerAiIcon} alt="" className="ai-assistant-panel__avatar" draggable={false} />
        <div className="ai-assistant-panel__heading">
          <span className="ai-assistant-panel__title">RER-AI</span>
          <span className="ai-assistant-panel__subtitle">
            <StatusDot status={llmStatus} />
            <span>{statusText()}</span>
          </span>
        </div>
        <ModelSelect
          models={models}
          value={settings.selectedModel ?? ''}
          goConnected={settings.goConnected}
          onChange={(next) => void handleModelChange(next)}
        />
        <button
          type="button"
          className="ai-assistant-panel__icon-btn"
          onClick={startNewChat}
          disabled={busy}
          title={t('New chat')}
          aria-label={t('New chat')}
        >
          ＋
        </button>
        <button
          type="button"
          className="ai-assistant-panel__icon-btn ai-assistant-panel__icon-btn--close"
          onClick={closePanel}
          title={t('Close')}
          aria-label={t('Close')}
        >
          ×
        </button>
      </div>

      <div className="ai-assistant-panel__messages" ref={scrollRef}>
        {lines.map((line) => (
          <div key={line.id} className={`ai-assistant-msg ai-assistant-msg--${line.role}`}>
            <div className="ai-assistant-msg__bubble">
              <p className="ai-assistant-reply small mb-0">
                <AssistantReplyText text={line.content} />
              </p>
            </div>
          </div>
        ))}

        {streaming != null && (
          <div className="ai-assistant-msg ai-assistant-msg--assistant">
            <div className="ai-assistant-msg__bubble">
              {streaming ? (
                <p className="ai-assistant-reply small mb-0">
                  <AssistantReplyText text={streaming} />
                  <span className="ai-assistant-thinking-dots" aria-hidden="true">
                    <span>.</span><span>.</span><span>.</span>
                  </span>
                </p>
              ) : (
                <p className="ai-assistant-status small mb-0">{t('Thinking')}</p>
              )}
            </div>
          </div>
        )}

        {toolChips.length > 0 && (
          <div className="ai-assistant-panel__chips">
            {toolChips.map((chip) => (
              <span key={chip.id} className="ai-assistant-chip">
                {chip.label}
              </span>
            ))}
          </div>
        )}

        {error && (
          <div className="ai-assistant-panel__error">
            <p className="small mb-1">{error}</p>
            <div className="d-flex gap-2">
              {lastUserRef.current && (
                <button type="button" className="btn btn-sm btn-outline-secondary" onClick={retry} disabled={busy}>
                  {t('Retry')}
                </button>
              )}
            </div>
          </div>
        )}
      </div>

      <div className="ai-assistant-panel__composer">
        <div className="ai-assistant-panel__composer-row">
          <textarea
            className="ai-assistant-panel__textarea"
            placeholder={t('Ask the assistant…')}
            value={input}
            rows={1}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault()
                send()
              }
            }}
            disabled={!canSend || busy}
          />
          {busy || streaming != null ? (
            <button
              type="button"
              className="ai-assistant-panel__send-btn ai-assistant-panel__send-btn--stop"
              onClick={stop}
              title={t('Stop')}
              aria-label={t('Stop')}
            >
              ■
            </button>
          ) : (
            <button
              type="button"
              className="ai-assistant-panel__send-btn"
              onClick={send}
              disabled={!input.trim() || !canSend}
              title={t('Send')}
              aria-label={t('Send')}
            >
              ➤
            </button>
          )}
        </div>
        <div className="ai-assistant-panel__footer">
          <span>{t('Reads only docs/ai')}</span>
          {llmStatus === 'running' && settings.selectedModel && !settings.goConnected && (
            <span className="text-secondary">· {t('OpenCode Go token required for paid models')}</span>
          )}
        </div>
      </div>
    </div>
  ) : null

  return (
    <div className="ai-assistant-overlay-root">
      <div className={`ai-assistant-stack ai-assistant-stack--${open ? 'panel' : 'column-above'}`}>
        {!open && (
          <SpeechBubble variant="above">
            <p className="mb-0 ai-assistant-speech-text">
              {t("Hi! I'm RER-AI, your AI assistant. If you need anything, don't hesitate to ask me.")}
            </p>
          </SpeechBubble>
        )}

        {open && panel}

        {fab}
      </div>
    </div>
  )
}

function SpeechBubble({
  variant,
  children,
}: {
  variant: 'above' | 'below' | 'side'
  children: ReactNode
}) {
  return (
    <div className={`ai-assistant-speech-bubble ai-assistant-speech-bubble--${variant}`}>
      {children}
    </div>
  )
}

export function AiAssistantOverlayApp() {
  const [locale, setLocale] = useState<'en' | 'es'>('en')
  const [active, setActive] = useState(false)

  useEffect(() => {
    document.documentElement.classList.add('ai-assistant-overlay')
    document.body.classList.add('ai-assistant-overlay')
    return () => {
      document.documentElement.classList.remove('ai-assistant-overlay')
      document.body.classList.remove('ai-assistant-overlay')
    }
  }, [])

  useEffect(() => {
    const removeConfig = window.electronAPI.onAiAssistantConfig((config) => {
      if (!config) {
        setActive(false)
        return
      }
      setLocale(config.locale === 'es' ? 'es' : 'en')
      setActive(true)
    })
    window.electronAPI.notifyAiAssistantReady()
    return removeConfig
  }, [])

  if (!active) {
    return <div className="ai-assistant-overlay-idle" />
  }

  return (
    <LanguageProvider initialLocale={locale}>
      <AiAssistantOverlayInner />
    </LanguageProvider>
  )
}
