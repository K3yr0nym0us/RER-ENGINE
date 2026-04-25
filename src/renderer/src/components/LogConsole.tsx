import React, { useEffect, useRef } from 'react'
import { LogEntry } from '../context/useContextEngine'

export function LogConsole({ log }: { log: LogEntry[] }) {
  const logRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight
  }, [log])

  return (
    <div
      ref={logRef}
      className="console-panel overflow-auto font-monospace px-3 py-2 small"
    >
      {log.length === 0
        ? <span className="text-secondary">Sin eventos aún…</span>
        : log.map((entry) => (
          <div key={entry.id} className={entry.isError ? 'text-danger' : 'text-success'}>
            {entry.text}
          </div>
        ))
      }
    </div>
  )
}

export default LogConsole;
