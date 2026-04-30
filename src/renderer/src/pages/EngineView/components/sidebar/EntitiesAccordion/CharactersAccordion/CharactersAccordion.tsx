import { Files, Trash } from 'react-bootstrap-icons';

import { useContextEngine } from '../../../../../../context/useContextEngine';

export interface AssetGroupConfig {
  openDialog:  () => Promise<string | null>
  loadCmd:     string
  dupCmd:      string
  addBtnLabel: string
  emptyText:   string
}

interface Props {
  config: AssetGroupConfig
}

export function CharactersAccordion({ config }: Props) {
  const { engineReady, send, characterEntities: entries, removeCharacter: onRemove, duplicateCharacter: onDuplicate } = useContextEngine()
  const hoveredEntityId = useContextEngine().hoveredEntityId

  const handleLoad = () => {
    config.openDialog().then((p: string | null) => {
      if (!p) return
      send({ cmd: config.loadCmd, path: p })
    })
  }

  const entryLabel = (path: string) => path.split('/').pop() ?? path

  return (
    <>
      {entries.length === 0 ? (
        <p className="text-secondary fst-italic small mb-0 px-1">{config.emptyText}</p>
      ) : (
        <ul className="list-unstyled mb-0">
          {entries.map(({ id, path }) => {
            const isHighlighted = id === hoveredEntityId
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

export default CharactersAccordion