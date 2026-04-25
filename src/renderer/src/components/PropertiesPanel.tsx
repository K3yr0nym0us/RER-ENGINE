import { useEffect, useState, useCallback } from 'react'
import { Lock, Unlock } from 'react-bootstrap-icons'
import type { SelectedEntity } from '../context/useContextEngine'

interface Transform {
  pos: [string, string, string]
  rot: [string, string, string, string]
  scl: [string, string, string]
}

interface Props {
  entity:      SelectedEntity | null
  onSend:      (cmd: object) => void
  projectType?: string
  isScenario?:  boolean
}

export function PropertiesPanel({ entity, onSend, projectType, isScenario = false }: Props) {
  const is2D = projectType === '2D'

  const [transform, setTransform] = useState<Transform>({
    pos: ['0', '0', '0'],
    rot: ['0', '0', '0', '1'],
    scl: ['1', '1', '1'],
  })
  const [lockProportions, setLockProportions] = useState(false)
  const [physicsEnabled, setPhysicsEnabled] = useState(false)
  const [physicsType,    setPhysicsType]    = useState('dynamic')

  useEffect(() => {
    if (!entity) return
    setPhysicsEnabled(entity.physicsEnabled)
    setPhysicsType(entity.physicsType || 'dynamic')
  }, [entity?.id])

  useEffect(() => {
    if (!entity) return
    setTransform({
      pos: entity.position.map((n, i) =>
        (is2D && i === 2) ? String(Math.round(n)) : n.toFixed(1)
      ) as [string, string, string],
      rot: entity.rotation.map((n) => n.toFixed(1)) as [string, string, string, string],
      scl: entity.scale.map((n) => n.toFixed(1)) as [string, string, string],
    })
  }, [entity?.id, entity, is2D])

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
    step: string | [string, string, string] = '0.1',
    options: {
      hiddenAxes?:    number[]
      labelAction?:   React.ReactNode
      extraOnChange?: (i: number, next: [string, string, string]) => [string, string, string]
    } = {},
  ) => {
    const { hiddenAxes = [], labelAction, extraOnChange } = options
    return (
      <div className="mb-2">
        <p className="prop-label">{label}</p>
        <div className="d-flex gap-1 mt-1 align-items-end">
          {(['X', 'Y', 'Z'] as const).map((ax, i) => {
            if (hiddenAxes.includes(i)) return null
            return (
              <div key={ax} className="flex-fill">
                <div className={`prop-axis ${axisColors[i]}`}>{ax}</div>
                <input
                  type="number"
                  step={Array.isArray(step) ? step[i] : step}
                  value={vals[i]}
                  aria-label={`${label} ${ax}`}
                  className="form-control form-control-sm text-center bg-dark text-light border-secondary prop-input"
                  onChange={(e) => {
                    let next = [...vals] as [string, string, string]
                    next[i] = e.target.value
                    if (extraOnChange) next = extraOnChange(i, next)
                    const updated = { ...transform, [key]: next }
                    setTransform(updated)
                    commit({ [key]: next })
                  }}
                />
              </div>
            )
          })}
          {labelAction && (
            <div className="d-flex flex-column align-items-center">
              <div className="prop-axis" style={{ visibility: 'hidden' }}>·</div>
              {labelAction}
            </div>
          )}
        </div>
      </div>
    )
  }

  const proportionOnChange = (i: number, next: [string, string, string]): [string, string, string] => {
    const xVal = parseFloat(transform.scl[0])
    const yVal = parseFloat(transform.scl[1])
    if (i === 0 && xVal !== 0) {
      next[1] = (parseFloat(next[0]) * yVal / xVal).toFixed(3)
    } else if (i === 1 && yVal !== 0) {
      next[0] = (parseFloat(next[1]) * xVal / yVal).toFixed(3)
    }
    return next
  }

  const lockBtn = is2D ? (
    <button
      className={`btn btn-sm ${lockProportions ? 'btn-info' : 'btn-outline-secondary'}`}
      title={lockProportions ? 'Proporciones bloqueadas' : 'Mantener proporciones'}
      onClick={() => setLockProportions((v) => !v)}
    >
      {lockProportions ? <Lock size={13} /> : <Unlock size={13} />}
    </button>
  ) : undefined

  return (
    <div className="px-1">
      <div className="mb-2">
        <p className="prop-label">Nombre</p>
        <div className="form-control form-control-sm bg-dark text-info border-secondary mt-1 prop-input">
          {entity.name}
        </div>
      </div>
      {makeVec3Row('Posición', transform.pos, 'pos', is2D ? ['0.1', '0.1', '1'] : '0.1')}
      {makeVec3Row('Escala', transform.scl, 'scl', '0.1', {
        hiddenAxes:    is2D ? [2] : [],
        labelAction:   lockBtn,
        extraOnChange: is2D && lockProportions ? proportionOnChange : undefined,
      })}
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
      {isScenario ? (
        <div className="mb-2">
          <p className="prop-label">Colisión</p>
          <div className="d-flex align-items-center gap-2 mt-1">
            <input
              type="checkbox"
              id="scenario-collision"
              className="form-check-input"
              checked={physicsEnabled}
              onChange={(e) => {
                const next = e.target.checked
                setPhysicsEnabled(next)
                onSend({ cmd: 'set_physics', id: entity.id, enabled: next, body_type: 'static' })
              }}
            />
            <label htmlFor="scenario-collision" className="form-check-label text-light small mb-0">
              Con colisión
            </label>
          </div>
        </div>
      ) : (
        <div className="mb-2">
          <p className="prop-label">Física</p>
          <div className="d-flex align-items-center gap-2 mt-1">
            <input
              type="checkbox"
              id="physics-enabled"
              className="form-check-input"
              checked={physicsEnabled}
              onChange={(e) => {
                const next = e.target.checked
                setPhysicsEnabled(next)
                onSend({ cmd: 'set_physics', id: entity.id, enabled: next, body_type: physicsType })
              }}
            />
            <label htmlFor="physics-enabled" className="form-check-label text-light small mb-0">
              Activar física
            </label>
          </div>
          {physicsEnabled && (
            <select
              value={physicsType}
              className="form-select form-select-sm bg-dark text-light border-secondary mt-1"
              onChange={(e) => {
                const next = e.target.value
                setPhysicsType(next)
                onSend({ cmd: 'set_physics', id: entity.id, enabled: true, body_type: next })
              }}
            >
              <option value="dynamic">Dinámico (gravedad)</option>
              <option value="static">Estático (no se mueve)</option>
              {!is2D && <option value="kinematic">Cinemático (por código)</option>}
            </select>
          )}
        </div>
      )}
    </div>
  )
}
