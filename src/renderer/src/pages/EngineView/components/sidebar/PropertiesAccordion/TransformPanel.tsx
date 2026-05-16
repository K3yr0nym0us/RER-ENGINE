import { useEffect, useState, useCallback, ReactNode } from 'react';

import { Lock, Unlock } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import { useTraslate } from '@hooks';

interface Transform {
  pos: [string, string, string]
  rot: [string, string, string, string]
  scl: [string, string, string]
}

interface Props {
  entity: {
    id: number
    position: [number, number, number]
    rotation: [number, number, number, number]
    scale:    [number, number, number]
  } | null
  is2D: boolean
  onSend: (cmd: object) => void
}

const RAD_TO_DEG = 180 / Math.PI;
const DEG_TO_RAD = Math.PI / 180;

function quatToEulerDegrees(q: [number, number, number, number]): [number, number, number] {
  const [x, y, z, w] = q;
  const sinr = 2 * (w * x + y * z);
  const cosr = 1 - 2 * (x * x + y * y);
  const rx = Math.atan2(sinr, cosr);
  const sinp = 2 * (w * y - z * x);
  const ry = Math.abs(sinp) >= 1 ? Math.sign(sinp) * (Math.PI / 2) : Math.asin(sinp);
  const siny = 2 * (w * z + x * y);
  const cosy = 1 - 2 * (y * y + z * z);
  const rz = Math.atan2(siny, cosy);
  return [rx * RAD_TO_DEG, ry * RAD_TO_DEG, rz * RAD_TO_DEG];
}

function eulerDegreesToQuat(euler: [number, number, number]): [number, number, number, number] {
  const [ex, ey, ez] = euler.map((d) => d * DEG_TO_RAD);
  const cx = Math.cos(ex / 2), sx = Math.sin(ex / 2);
  const cy = Math.cos(ey / 2), sy = Math.sin(ey / 2);
  const cz = Math.cos(ez / 2), sz = Math.sin(ez / 2);
  return [
    sx * cy * cz - cx * sy * sz,
    cx * sy * cz + sx * cy * sz,
    cx * cy * sz - sx * sy * cz,
    cx * cy * cz + sx * sy * sz,
  ];
}

export function TransformPanel({ entity, is2D, onSend }: Props) {
  const { t } = useTraslate()
  const [transform, setTransform] = useState<Transform>({
    pos: ['0', '0', '0'],
    rot: ['0', '0', '0', '1'],
    scl: ['1', '1', '1'],
  })
  const [lockProportions, setLockProportions] = useState(false)

  useEffect(() => {
    if (!entity) return
    const rot = is2D
      ? entity.rotation.map((n) => n.toFixed(1)) as [string, string, string, string]
      : (() => {
        const [rx, ry, rz] = quatToEulerDegrees(entity.rotation);
        return [rx.toFixed(1), ry.toFixed(1), rz.toFixed(1), ''] as [string, string, string, string];
      })();
    setTransform({
      pos: entity.position.map((n, i) =>
        (is2D && i === 2) ? String(Math.round(n)) : n.toFixed(1)
      ) as [string, string, string],
      rot,
      scl: entity.scale.map((n) => n.toFixed(1)) as [string, string, string],
    })
  }, [entity, is2D])

  const commit = useCallback((override: Partial<Transform>) => {
    if (!entity) return
    const merged = { ...transform, ...override }
    const rotation: [number, number, number, number] = is2D
      ? merged.rot.map(Number) as [number, number, number, number]
      : eulerDegreesToQuat([
        Number(merged.rot[0]),
        Number(merged.rot[1]),
        Number(merged.rot[2]),
      ]);
    onSend({
      cmd:      'set_transform',
      id:       entity.id,
      position: merged.pos.map(Number) as [number, number, number],
      rotation,
      scale:    merged.scl.map(Number) as [number, number, number],
    })
  }, [entity, transform, onSend, is2D])

  const axisColors = ['text-danger', 'text-success', 'text-info']
  const rotationAxes = is2D ? (['X', 'Y', 'Z', 'W'] as const) : (['X', 'Y', 'Z'] as const)

  const makeVec3Row = (
    label: string,
    vals: [string, string, string],
    key: 'pos' | 'scl',
    step: string | [string, string, string] = '0.1',
    options: {
      hiddenAxes?:    number[]
      labelAction?:   ReactNode
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
      <AppTooltip content={lockProportions ? t('Lock proportions') : t('Keep proportions')} place="top">
      <button
        className={`btn btn-sm ${lockProportions ? 'btn-info' : 'btn-outline-secondary'}`}
        onClick={() => setLockProportions((v) => !v)}
      >
        {lockProportions ? <Lock size={13} /> : <Unlock size={13} />}
      </button>
    </AppTooltip>
  ) : undefined

  return (
    <>
      {makeVec3Row(t('Position'), transform.pos, 'pos', is2D ? ['0.1', '0.1', '1'] : '0.1')}
      {makeVec3Row(t('Scale'), transform.scl, 'scl', '0.1', {
        hiddenAxes:    is2D ? [2] : [],
        labelAction:   lockBtn,
        extraOnChange: is2D && lockProportions ? proportionOnChange : undefined,
      })}
      <div className="mb-2">
        <p className="prop-label">{is2D ? t('Rotation (xyzw)') : t('Rotation (degrees)')}</p>
        <div className="d-flex gap-1 mt-1">
          {rotationAxes.map((ax, i) => (
            <div key={ax} className="flex-fill">
              <div
                className={`prop-axis ${i < 3 ? axisColors[i] : ''}`}
                style={i === 3 ? { color: '#a78bfa' } : undefined}
              >
                {ax}
              </div>
              <input
                type="number"
                step={is2D ? '0.01' : '1'}
                value={transform.rot[i]}
                aria-label={`${is2D ? t('Rotation (xyzw)') : t('Rotation (degrees)')} ${ax}`}
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
    </>
  )
}

export default TransformPanel;