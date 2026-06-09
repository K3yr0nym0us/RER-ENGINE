import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'

import rerAiIcon from '../../../resources/RER-AI.png'
import rerAiPointingIcon from '../../../resources/RER-AI-POINTING.png'
import rerAiThinkingIcon from '../../../resources/RER-AI-THINKING.png'
import { LanguageProvider, useLanguage } from '../context/LanguageContext'
import { useTraslate } from '@hooks'

interface ChatLine {
  role: 'user' | 'assistant'
  content: string
}

type AssistantPhase = 'idle' | 'input' | 'thinking' | 'answer'

const CLICK_MAX_MOVE_PX = 8
const DRAG_ARM_MOVE_PX = 4
const DRAG_HOLD_MS = 160

type SpeechBubbleVariant = 'above' | 'below' | 'side'

interface FabPointerSession {
  pointerId: number
  startX: number
  startY: number
  armed: boolean
  target: HTMLElement
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

function SpeechBubble({
  variant,
  children,
}: {
  variant: SpeechBubbleVariant
  children: ReactNode
}) {
  return (
    <div className={`ai-assistant-speech-bubble ai-assistant-speech-bubble--${variant}`}>
      {children}
    </div>
  )
}

function AiAssistantOverlayInner() {
  const { t } = useTraslate()
  const { locale } = useLanguage()
  const [phase, setPhase] = useState<AssistantPhase>('idle')
  const [showIntro, setShowIntro] = useState(true)
  const [input, setInput] = useState('')
  const [lines, setLines] = useState<ChatLine[]>([])
  const [error, setError] = useState<string | null>(null)
  const [llmStatus, setLlmStatus] = useState('idle')
  const inputRef = useRef<HTMLInputElement>(null)

  const refreshLlmStatus = useCallback(async () => {
    const status = await window.electronAPI.pluginsGetLlmStatus()
    setLlmStatus(status.status)
    if (status.status === 'running' || status.status === 'starting') {
      setError(null)
    } else if (status.error) {
      setError(status.error)
    }
  }, [])

  useEffect(() => {
    void refreshLlmStatus()
    const id = window.setInterval(() => void refreshLlmStatus(), 2_000)
    return () => window.clearInterval(id)
  }, [refreshLlmStatus])

  const lastAssistantLine = [...lines].reverse().find((l) => l.role === 'assistant')

  const overlayLayout: 'intro' | 'thinking' | 'input' | 'answer' =
    phase === 'input'
      ? 'input'
      : phase === 'thinking'
        ? 'thinking'
        : phase === 'answer'
          ? 'answer'
          : 'intro'

  useEffect(() => {
    window.electronAPI.aiAssistantSetLayout(overlayLayout)
  }, [overlayLayout])

  useEffect(() => {
    if (phase === 'input' && inputRef.current) {
      inputRef.current.focus()
    }
  }, [phase])

  const send = useCallback(async () => {
    const text = input.trim()
    if (!text || phase !== 'input') return

    setInput('')
    setError(null)
    const nextLines: ChatLine[] = [...lines, { role: 'user', content: text }]
    setLines(nextLines)
    setPhase('thinking')

    try {
      const messages = nextLines.map((l) => ({
        role: l.role as 'user' | 'assistant',
        content: l.content,
      }))
      const result = await window.electronAPI.pluginsChat({ messages, locale })
      if (result.debug) {
        console.log('[ai-assistant overlay] chat debug', result.debug)
      }
      console.log('[ai-assistant overlay] chat result', {
        ok: result.ok,
        contentLength: result.content?.length ?? 0,
        error: result.error,
      })
      if (!result.ok) {
        const detail = result.debug?.logFile ? ` (${result.debug.logFile})` : ''
        setError((result.error ?? t('Chat failed')) + detail)
        setPhase('input')
      } else if (result.content?.trim()) {
        setLines((prev) => [...prev, { role: 'assistant', content: result.content!.trim() }])
        setPhase('answer')
      } else {
        const detail = result.debug?.logFile ? ` ${result.debug.logFile}` : ''
        setError(t('The model returned an empty response. Try again.') + detail)
        setPhase('input')
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      setPhase('input')
    } finally {
      void refreshLlmStatus()
    }
  }, [input, phase, lines, locale, t, refreshLlmStatus])

  const handleFabClick = useCallback(() => {
    if (phase === 'thinking' || phase === 'answer') return
    if (phase === 'input') {
      setPhase('idle')
      setError(null)
      return
    }
    setShowIntro(false)
    setPhase('input')
  }, [phase])

  const dismissAnswer = useCallback(() => {
    setPhase('idle')
    setLines([])
    setInput('')
    setError(null)
  }, [])

  const { isDragging, onFabPointerDown } = useFabPressHandlers({
    onShortClick: handleFabClick,
  })

  const canSend = llmStatus === 'running' || llmStatus === 'starting'
  const fabIcon =
    phase === 'answer'
      ? rerAiPointingIcon
      : phase === 'thinking' || (phase === 'input' && input.length > 0)
        ? rerAiThinkingIcon
        : rerAiIcon

  const showInputRow = phase === 'input'
  const showAnswerBubble = phase === 'answer' && Boolean(lastAssistantLine)

  const fab = (
    <div
      className={`ai-assistant-fab-wrap${isDragging ? ' ai-assistant-fab-wrap--dragging' : ''}`}
      data-plugin-target="ai-assistant-fab"
      role="button"
      tabIndex={0}
      aria-label={t('Open AI assistant')}
      onPointerDown={onFabPointerDown}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault()
          handleFabClick()
        }
      }}
    >
      <div className="ai-assistant-fab" aria-hidden>
        <img src={fabIcon} alt="" className="ai-assistant-fab-img" draggable={false} />
      </div>
    </div>
  )

  const inputBubble = (
    <SpeechBubble variant="side">
      <div className="ai-assistant-input-bubble">
        {llmStatus === 'starting' && (
          <p className="ai-assistant-status small mb-2">{t('AI server is starting…')}</p>
        )}
        {error && <p className="ai-assistant-error small mb-2">{error}</p>}
        <div className="ai-assistant-input-row">
          <input
            ref={inputRef}
            type="text"
            className="ai-assistant-input-field"
            placeholder={t('Ask the assistant…')}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void send()
            }}
            disabled={!canSend}
          />
          <button
            type="button"
            className="ai-assistant-send-btn"
            onClick={() => void send()}
            disabled={!input.trim() || !canSend}
          >
            {t('Send')}
          </button>
        </div>
      </div>
    </SpeechBubble>
  )

  const stackClass = showInputRow ? 'chat' : 'column-above'

  return (
    <div className="ai-assistant-overlay-root">
      <div className={`ai-assistant-stack ai-assistant-stack--${stackClass}`}>
        {showIntro && phase === 'idle' && (
          <SpeechBubble variant="above">
            <p className="mb-0 ai-assistant-speech-text">
              {t("Hi! I'm RER-AI, your AI assistant. If you need anything, don't hesitate to ask me.")}
            </p>
          </SpeechBubble>
        )}

        {phase === 'thinking' && (
          <SpeechBubble variant="above">
            <div className="ai-assistant-input-bubble">
              {llmStatus === 'starting' && (
                <p className="ai-assistant-status small mb-0">{t('AI server is starting…')}</p>
              )}
              {llmStatus !== 'starting' && (
                <p className="ai-assistant-status small mb-0">{t('Thinking…')}</p>
              )}
              {error && <p className="ai-assistant-error small mb-0 mt-2">{error}</p>}
            </div>
          </SpeechBubble>
        )}

        {showInputRow ? (
          <div className="ai-assistant-main-row">
            {inputBubble}
            {fab}
          </div>
        ) : (
          fab
        )}

        {showAnswerBubble && lastAssistantLine && (
          <SpeechBubble variant="below">
            <div className="ai-assistant-answer-bubble">
              <button
                type="button"
                className="ai-assistant-bubble-close"
                aria-label={t('Close')}
                onClick={dismissAnswer}
              >
                ×
              </button>
              <p className="ai-assistant-reply small mb-0">{lastAssistantLine.content}</p>
            </div>
          </SpeechBubble>
        )}
      </div>
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
