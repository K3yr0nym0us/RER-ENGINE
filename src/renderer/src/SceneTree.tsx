import type { Entity } from './useEngine'

interface Props {
  entities: Entity[]
  selectedId?: number | null
  onSelect?: (id: number) => void
}

export function SceneTree({ entities, selectedId, onSelect }: Props) {
  if (entities.length === 0) {
    return <p className="text-white-50 fst-italic small mb-0">Sin entidades</p>
  }

  return (
    <div className="list-group list-group-flush rounded overflow-hidden">
      {entities.map((e) => (
        <button
          key={e.id}
          className={`list-group-item list-group-item-action text-start fw-semibold ${e.id === selectedId ? 'active' : ''}`}
          onClick={() => onSelect?.(e.id)}
        >
          <span className="me-2 opacity-75">▸</span>
          Entity #{e.id}
        </button>
      ))}
    </div>
  )
}

