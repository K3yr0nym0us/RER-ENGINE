import { useEffect, useState, useCallback } from 'react'
import type { SelectedEntity } from '../hooks/useEngine'

interface Transform {
  pos: [string, string, string]
  rot: [string, string, string, string]
  scl: [string, string, string]
}

interface Props {
  entity: SelectedEntity | null
  onSend: (cmd: object) => void
}

export function PropertiesPanel({ entity, onSend }: Props) {
  const [transform, setTransform] = useState<Transform>({
    pos: ['0', '0', '0'],
    rot: ['0', '0', '0', '1'],
    scl: ['1', '1', '1'],
  })

  useEffect(() => {
    if (!entity) return
    setTransform({
      pos: entity.position.map((n) => n.toFixed(1)) as [string, string, string],
      rot: entity.rotation.map((n) => n.toFixed(1)) as [string, string, string, string],
      scl: entity.scale.map((n) => n.toFixed(1)) as [string, string, string],
    })
  }, [entity?.id, entity])

  const commit = useCallback((override: Partial<Transform>) => {
    if (!entity) return
    const merged = { ...transform, ...override }
    onSend({
      cmd:      'set_transform',
      id:       entity.id,
      position: merged.pos.map(Number) as [number, number, number],
      rotation: merged.rot.map(Number) as [number, number, number, number],
      scale:    merged.scl.map(Number) as [number, number, number],
    })
  }, [entity, transform, onSend])

  if (!entity) {
    return <p className="text-secondary fst-italic small mb-0 px-1">Haz click en un objeto para verlo</p>
  }

  const axisColors = ['text-danger', 'text-success', 'text-info']

  const makeVec3Row = (
    label: string,
    vals: [string, string, string],
    key: 'pos' | 'scl',
    step = '0.1',
  ) => (
    <div className="mb-2">
      <p className="prop-label">{label}</p>
      <div className="d-flex gap-1 mt-1">
        {(['X', 'Y', 'Z'] as const).map((ax, i) => (
          <div key={ax} className="flex-fill">
            <div className={`prop-axis ${axisColors[i]}`}>{ax}</div>
            <input
              type="number"
              step={step}
              value={vals[i]}
              aria-label={`${label} ${ax}`}
              className="form-control form-control-sm text-center bg-dark text-light border-secondary prop-input"
              onChange={(e) => {
                const next = [...vals] as [string, string, string]
                next[i] = e.target.value
                const updated = { ...transform, [key]: next }
                setTransform(updated)
                commit({ [key]: next })
              }}
            />
          </div>
        ))}
      </div>
    </div>
  )

  return (
    <div className="px-1">
      <div className="mb-2">
        <p className="prop-label">Nombre</p>
        <div className="form-control form-control-sm bg-dark text-info border-secondary mt-1 prop-input">
          {entity.name}
        </div>
      </div>
      {makeVec3Row('Posición', transform.pos, 'pos')}
      {makeVec3Row('Escala',   transform.scl, 'scl')}
      <div className="mb-2">
        <p className="prop-label">Rotación (xyzw)</p>
        <div className="d-flex gap-1 mt-1">
          {(['X', 'Y', 'Z', 'W'] as const).map((ax, i) => (
            <div key={ax} className="flex-fill">
              <div
                className={`prop-axis ${i < 3 ? axisColors[i] : ''}`}
                style={i === 3 ? { color: '#a78bfa' } : undefined}
              >
                {ax}
              </div>
              <input
                type="number"
                step="0.01"
                value={transform.rot[i]}
                aria-label={`Rotación ${ax}`}
                className="form-control form-control-sm text-center bg-dark text-light border-secondary prop-input"
                onChange={(e) => {
                  const next = [...transform.rot] as [string, string, string, string]
                  next[i] = e.target.value
                  setTransform((prev) => ({ ...prev, rot: next }))
                  commit({ rot: next })
                }}
              />
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
