import { Files, Trash } from 'react-bootstrap-icons'
import type { ScenarioEntry } from '../../hooks/useEngine'

export interface AssetGroupConfig {
  openDialog:  () => Promise<string | null>
  loadCmd:     string
  dupCmd:      string
  addBtnLabel: string
  emptyText:   string
}

interface Props {
  engineReady: boolean
  send:        (cmd: object) => void
  entries:     ScenarioEntry[]
  onRemove:    (id: number) => void
  onDuplicate: (id: number) => void
  config:      AssetGroupConfig
  highlightId: number | null
}

export function AssetGroupPanel({ engineReady, send, entries, onRemove, onDuplicate, config, highlightId }: Props) {
  const handleLoad = () => {
    config.openDialog().then((p: string | null) => {
      if (!p) return
      send({ cmd: config.loadCmd, path: p })
    })
  }

  const entryLabel = (path: string) => path.split('/').pop() ?? path

  return (
    <>
      <button
        className="btn btn-outline-info btn-sm w-100 fw-bold mb-2"
        disabled={!engineReady}
        onClick={handleLoad}
      >
        {config.addBtnLabel}
      </button>

      {entries.length === 0 ? (
        <p className="text-secondary fst-italic small mb-0 px-1">{config.emptyText}</p>
      ) : (
        <ul className="list-unstyled mb-0">
          {entries.map(({ id, path }) => {
            const isHighlighted = id === highlightId
            return (
            <li key={id} className="mb-1">
              <div
                className="d-flex align-items-center gap-1"
                style={isHighlighted ? { background: '#1e2a4a', borderRadius: 4, outline: '1px solid #38bdf855' } : undefined}
              >
                <button
                  className="btn btn-sm flex-fill text-start text-truncate"
                  style={{ color: isHighlighted ? '#7dd3fc' : undefined, fontWeight: isHighlighted ? 700 : undefined }}
                  title={path}
                  disabled
                >
                  {isHighlighted ? '▶ ' : ''}{entryLabel(path)}
                </button>
                <button
                  className="btn btn-sm btn-outline-secondary"
                  title="Duplicar"
                  onClick={() => onDuplicate(id)}
                ><Files /></button>
                <button
                  className="btn btn-sm btn-outline-danger"
                  title="Quitar"
                  onClick={() => onRemove(id)}
                ><Trash /></button>
              </div>
            </li>
            )
          })}
        </ul>
      )}
    </>
  )
}
