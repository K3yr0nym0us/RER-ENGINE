import type { ReactNode } from 'react'

function parseBoldLine(line: string): ReactNode[] {
  const parts = line.split(/(\*\*[^*]+\*\*)/g)
  return parts.map((part, index) => {
    const match = part.match(/^\*\*([^*]+)\*\*$/)
    if (match) {
      return <strong key={index}>{match[1]}</strong>
    }
    return <span key={index}>{part}</span>
  })
}

/** Renderiza respuestas del asistente: saltos de línea + **negrita** simple. */
export function AssistantReplyText({ text }: { text: string }) {
  const lines = text.split('\n')
  return (
    <>
      {lines.map((line, lineIndex) => (
        <span key={lineIndex}>
          {lineIndex > 0 && <br />}
          {parseBoldLine(line)}
        </span>
      ))}
    </>
  )
}
