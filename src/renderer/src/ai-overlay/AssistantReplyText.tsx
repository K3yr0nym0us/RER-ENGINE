import type { ReactNode } from 'react'

const INLINE_RE = /(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\((https?:\/\/[^)\s]+)\))/g

function renderInlineSegment(text: string, keyPrefix: string): ReactNode[] {
  const parts = text.split(INLINE_RE)
  return parts.map((part, index) => {
    const key = `${keyPrefix}-${index}`
    if (part.startsWith('**') && part.endsWith('**') && part.length > 4) {
      return <strong key={key}>{part.slice(2, -2)}</strong>
    }
    if (part.startsWith('`') && part.endsWith('`') && part.length > 2) {
      return <code key={key} className="ai-assistant-md-code">{part.slice(1, -1)}</code>
    }
    const link = part.match(/^\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)$/)
    if (link) {
      return (
        <a key={key} href={link[2]} target="_blank" rel="noreferrer noopener">
          {link[1]}
        </a>
      )
    }
    return <span key={key}>{part}</span>
  })
}

function isListItem(line: string): boolean {
  return /^\d+\.\s/.test(line) || /^[-*]\s/.test(line)
}

/** Renderiza respuestas del asistente: markdown mínimo (negrita, código, enlaces, listas). */
export function AssistantReplyText({ text }: { text: string }) {
  const blocks = text.split(/```[a-zA-Z0-9_-]*\n?/)

  return (
    <>
      {blocks.map((block, blockIndex) => {
        if (blockIndex % 2 === 1) {
          return (
            <pre key={`code-${blockIndex}`} className="ai-assistant-md-pre">
              <code>{block.replace(/\n$/, '')}</code>
            </pre>
          )
        }
        const lines = block.split('\n')
        return (
          <span key={`text-${blockIndex}`}>
            {lines.map((line, lineIndex) => (
              <span
                key={lineIndex}
                className={
                  isListItem(line.trim())
                    ? 'ai-assistant-md-line ai-assistant-md-line--item'
                    : 'ai-assistant-md-line'
                }
              >
                {renderInlineSegment(line, `${blockIndex}-${lineIndex}`)}
              </span>
            ))}
          </span>
        )
      })}
    </>
  )
}
