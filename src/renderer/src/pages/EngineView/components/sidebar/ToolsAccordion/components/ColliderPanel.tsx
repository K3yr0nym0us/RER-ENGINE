import { Trash } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import type { ScenarioEntry } from '@engine';
import { useTraslate } from '@hooks';

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
  const { t } = useTraslate()
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
                  <AppTooltip content={`${t('Collider')} #${id}`} place="top">
                    <div
                      className="btn btn-sm flex-fill text-start text-truncate"
                      style={isHighlighted 
                        ? { background: '#1e2a4a', borderRadius: '4px 0 0 4px', outline: '1px solid #38bdf855', color: '#7dd3fc', fontWeight: 700 }
                        : { background: 'var(--bs-dark)', border: '1px solid var(--bs-secondary)', borderRadius: '4px 0 0 4px', color: 'var(--bs-light)' }
                      }
                    >
                      {isHighlighted ? '? ' : ''}#{id}
                    </div>
                  </AppTooltip>
                  <AppTooltip content={t('Delete collider')} place="top">
                    <button
                      className="btn btn-sm btn-outline-danger py-1"
                      onClick={() => onRemove(id)}
                    ><Trash /></button>
                  </AppTooltip>
                </div>
              </li>
            )
          })}
        </ul>
      )}
    </>
  )
}