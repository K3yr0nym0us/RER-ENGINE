import { Trash } from 'react-bootstrap-icons'
import type { ScenarioEntry } from '../../context/useContextEngine'

export interface ColliderPanelConfig {
  addBtnLabel: string
  emptyText:   string
}

interface Props {
  entries:  ScenarioEntry[]
  onRemove: (id: number) => void
  config:  ColliderPanelConfig
  highlightId: number | null
}

export function ColliderPanel({ entries, onRemove, config, highlightId }: Props) {
  return (
    <>
      {entries.length === 0 ? (
        <p className="text-secondary fst-italic small mb-0 px-1">{config.emptyText}</p>
      ) : (
        <ul className="list-unstyled mb-0">
          {entries.map(({ id, path }) => {
            const isHighlighted = id === highlightId
            return (
              <li key={id} className="mb-1">
                <div className="d-flex align-items-center gap-1">
                  <div
                    className="btn btn-sm flex-fill text-start text-truncate"
                    style={isHighlighted 
                      ? { background: '#1e2a4a', borderRadius: '4px 0 0 4px', outline: '1px solid #38bdf855', color: '#7dd3fc', fontWeight: 700 }
                      : { background: 'var(--bs-dark)', border: '1px solid var(--bs-secondary)', borderRadius: '4px 0 0 4px', color: 'var(--bs-light)' }
                    }
                    title={`Colisionador #${id}`}
                  >
                    {isHighlighted ? '▶ ' : ''}#{id}
                  </div>
                  <button
                    className="btn btn-sm btn-outline-danger py-1"
                    title="Eliminar colisionador"
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