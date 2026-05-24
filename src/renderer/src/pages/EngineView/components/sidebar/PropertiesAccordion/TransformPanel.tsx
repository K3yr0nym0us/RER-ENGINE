import { useEffect, useState, useCallback, useRef, ReactNode } from 'react';

import { Lock, Unlock } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import { useTraslate } from '@hooks';
import {
  eulerYxzDegreesToQuat,
  quatRotateLocalAxis,
  quatToEulerYxzDegrees,
  shortestDegDelta,
} from '../../../../../utils/eulerYxz';

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
  isPlayCharacter?: boolean
  /** Cámara orbital del editor (solo posición = blanco de órbita). */
  isEditorCamera?: boolean
  onSend: (cmd: object) => void
}

export function TransformPanel({
  entity,
  is2D,
  isPlayCharacter = false,
  isEditorCamera = false,
  onSend,
}: Props) {
  const { t } = useTraslate()
  const [transform, setTransform] = useState<Transform>({
    pos: ['0', '0', '0'],
    rot: ['0', '0', '0', '1'],
    scl: ['1', '1', '1'],
  })
  const [lockProportions, setLockProportions] = useState(true)
  /** Quaternion acumulado mientras se arrastra (evita gimbal al recomponer Euler). */
  const rotQuatRef = useRef<[number, number, number, number]>([0, 0, 0, 1])
  const rotDraggingRef = useRef(false)
  const rotEditingRef = useRef(false)
  /** Grados al enfocar un input numérico (base para delta al confirmar). */
  const rotFocusDegRef = useRef<[number, number, number]>([0, 0, 0])

  useEffect(() => {
    if (!entity) return
    if (!is2D && (rotDraggingRef.current || rotEditingRef.current)) return
    const rot = is2D
      ? entity.rotation.map((n) => n.toFixed(1)) as [string, string, string, string]
      : (() => {
        const [rx, ry, rz] = quatToEulerYxzDegrees(entity.rotation);
        return [
          rx.toFixed(1),
          ry.toFixed(1),
          rz.toFixed(1),
          '',
        ] as [string, string, string, string];
      })();
    rotQuatRef.current = [...entity.rotation] as [number, number, number, number]
    setTransform({
      pos: entity.position.map((n, i) =>
        (is2D && i === 2) ? String(Math.round(n)) : n.toFixed(1)
      ) as [string, string, string],
      rot,
      scl: entity.scale.map((n) => n.toFixed(1)) as [string, string, string],
    })
  }, [entity, is2D])

  const commit = useCallback((override: Partial<Transform> & {
    rotQuat?: [number, number, number, number]
    positionAxis?: { axis: number; value: number }
    scaleAxis?: { axis: number; value: number }
    rotationEulerDelta?: { axis: number; degrees: number }
    rotationEulerDegrees?: [number, number, number]
  }) => {
    if (!entity) return
    const cmd: {
      cmd: 'set_transform'
      id: number
      position?: [number, number, number]
      position_axis?: { axis: number; value: number }
      rotation?: [number, number, number, number]
      scale?: [number, number, number]
      scale_axis?: { axis: number; value: number }
      body_rotation_only?: boolean
      rotation_euler_delta?: { axis: number; degrees: number }
      rotation_euler_degrees?: [number, number, number]
    } = { cmd: 'set_transform', id: entity.id }

    if (override.positionAxis !== undefined) {
      cmd.position_axis = override.positionAxis
    } else if (override.pos !== undefined) {
      cmd.position = override.pos.map(Number) as [number, number, number]
    }
    if (!is2D && isPlayCharacter) {
      cmd.body_rotation_only = true
      if (override.rotationEulerDelta !== undefined) {
        cmd.rotation_euler_delta = override.rotationEulerDelta
      } else if (override.rotationEulerDegrees !== undefined) {
        cmd.rotation_euler_degrees = override.rotationEulerDegrees
      }
    } else if (override.rotQuat !== undefined) {
      cmd.rotation = override.rotQuat
    } else if (override.rot !== undefined) {
      cmd.rotation = is2D
        ? override.rot.map(Number) as [number, number, number, number]
        : eulerYxzDegreesToQuat(
          Number(override.rot[0]),
          Number(override.rot[1]),
          Number(override.rot[2]),
        );
    }
    if (override.scaleAxis !== undefined) {
      cmd.scale_axis = override.scaleAxis
    } else if (override.scl !== undefined) {
      cmd.scale = override.scl.map(Number) as [number, number, number]
    }
    onSend(cmd)
  }, [entity, onSend, is2D, isPlayCharacter])

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
                    const raw = e.target.value
                    let next = [...vals] as [string, string, string]
                    next[i] = raw
                    // extraOnChange (proporciones bloqueadas) modifica varios ejes a la vez.
                    const couplesAxes = !!extraOnChange
                    if (extraOnChange) next = extraOnChange(i, next)
                    const updated = { ...transform, [key]: next }
                    setTransform(updated)
                    const parsed = Number(raw)
                    if (!couplesAxes && Number.isFinite(parsed)) {
                      const axisKey = key === 'pos' ? 'positionAxis' : 'scaleAxis'
                      commit({ [axisKey]: { axis: i, value: parsed } })
                    } else {
                      commit({ [key]: next })
                    }
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
    const prev = transform.scl.map(parseFloat) as [number, number, number]
    const refVal = parseFloat(next[i])
    const oldRef = prev[i]
    if (!Number.isFinite(refVal) || Math.abs(oldRef) < 1e-6) return next

    if (!is2D && isPlayCharacter) {
      const ratio = refVal / oldRef
      return prev.map((v) => (v * ratio).toFixed(3)) as [string, string, string]
    }

    if (is2D) {
      const xVal = prev[0]
      const yVal = prev[1]
      if (i === 0 && xVal !== 0) {
        next[1] = (refVal * yVal / xVal).toFixed(3)
      } else if (i === 1 && yVal !== 0) {
        next[0] = (refVal * xVal / yVal).toFixed(3)
      }
      return next
    }

    const ratio = refVal / oldRef
    for (let j = 0; j < 3; j++) {
      if (j !== i) next[j] = (prev[j] * ratio).toFixed(3)
    }
    return next
  }

  const lockBtn = (
    <AppTooltip content={lockProportions ? t('Lock proportions') : t('Keep proportions')} place="top">
      <button
        type="button"
        className={`btn btn-sm ${lockProportions ? 'btn-info' : 'btn-outline-secondary'}`}
        onClick={() => setLockProportions((v) => !v)}
      >
        {lockProportions ? <Lock size={13} /> : <Unlock size={13} />}
      </button>
    </AppTooltip>
  )

  const rotMin = is2D ? -1 : -180
  const rotMax = is2D ? 1 : 180
  const rotStep = is2D ? 0.01 : 1

  const formatRotDisplay = (axisIndex: number, deg: number) =>
    (is2D ? deg.toFixed(2) : deg.toFixed(1))

  const commitRotAxis3d = (axisIndex: 0 | 1 | 2, newDeg: number, prevDeg: number) => {
    if (!entity) return
    const roundedNew = Math.round(newDeg)
    const roundedPrev = Math.round(prevDeg)
    const deltaDeg = shortestDegDelta(roundedPrev, roundedNew, 360)
    if (Math.abs(deltaDeg) < 1e-6) return
    setTransform((prev) => {
      const nextRot = [...prev.rot] as [string, string, string, string]
      nextRot[axisIndex] = formatRotDisplay(axisIndex, roundedNew)
      return { ...prev, rot: nextRot }
    })
    if (isPlayCharacter) {
      commit({ rotationEulerDelta: { axis: axisIndex, degrees: deltaDeg } })
      return
    }
    const deltaRad = deltaDeg * (Math.PI / 180)
    const newQ = quatRotateLocalAxis(rotQuatRef.current, axisIndex, deltaRad)
    rotQuatRef.current = newQ
    commit({ rotQuat: newQ })
  }

  const commitRotationNumberInput = (axisIndex: number) => {
    const newDeg = Math.round(parseFloat(transform.rot[axisIndex]))
    if (!Number.isFinite(newDeg)) return
    const baseDeg =
      rotFocusDegRef.current[axisIndex as 0 | 1 | 2]
      ?? Math.round(parseFloat(transform.rot[axisIndex]) || 0)
    if (is2D) {
      commit({ rot: transform.rot })
      return
    }
    commitRotAxis3d(axisIndex as 0 | 1 | 2, newDeg, baseDeg)
    rotFocusDegRef.current[axisIndex as 0 | 1 | 2] = newDeg
  }

  const applyRotationNumberInput = (axisIndex: number, raw: string) => {
    const next = [...transform.rot] as [string, string, string, string]
    next[axisIndex] = raw
    setTransform((prev) => ({ ...prev, rot: next }))
    if (is2D && Number.isFinite(parseFloat(raw))) {
      commit({ rot: next })
    }
  }

  const formatRotationOnBlur = (axisIndex: number) => {
    const n = parseFloat(transform.rot[axisIndex])
    if (!Number.isFinite(n)) return
    const formatted = formatRotDisplay(axisIndex, n)
    if (formatted === transform.rot[axisIndex]) return
    setTransform((prev) => {
      const nextRot = [...prev.rot] as [string, string, string, string]
      nextRot[axisIndex] = formatted
      return { ...prev, rot: nextRot }
    })
  }

  return (
    <>
      {isEditorCamera && (
        <p className="text-secondary small mb-2">{t('Transform editor camera orbit hint')}</p>
      )}
      {makeVec3Row(t('Position'), transform.pos, 'pos', is2D ? ['0.1', '0.1', '1'] : '0.1')}
      {!isEditorCamera && makeVec3Row(t('Scale'), transform.scl, 'scl', '0.1', {
        hiddenAxes:    is2D ? [2] : [],
        labelAction:   lockBtn,
        extraOnChange: lockProportions ? proportionOnChange : undefined,
      })}
      {!isEditorCamera && (
      <div className="mb-2">
        <p className="prop-label">{is2D ? t('Rotation (xyzw)') : t('Rotation (degrees)')}</p>
        {!is2D && isPlayCharacter && (
          <p className="text-secondary small mb-1">{t('Transform rotation player hint')}</p>
        )}
        <div className="d-flex flex-column gap-2 mt-1">
          {rotationAxes.map((ax, i) => (
            <div key={ax} className="flex-fill">
              <div className="d-flex align-items-center justify-content-between gap-1 mb-0">
                <span
                  className={`prop-axis small mb-0 ${i < 3 ? axisColors[i] : ''}`}
                  style={i === 3 ? { color: '#a78bfa' } : undefined}
                >
                  {ax}
                </span>
                <input
                  id={`transform-rot-num-${ax}`}
                  type="number"
                  step={is2D ? 0.01 : 0.1}
                  min={rotMin}
                  max={rotMax}
                  value={transform.rot[i]}
                  aria-label={`${is2D ? t('Rotation (xyzw)') : t('Rotation (degrees)')} ${ax}`}
                  className="form-control form-control-sm text-center bg-dark text-info border-secondary prop-input"
                  style={{ width: '4.25rem', flex: '0 0 auto' }}
                  onFocus={() => {
                    if (is2D || i > 2) return
                    rotEditingRef.current = true
                    rotFocusDegRef.current[i as 0 | 1 | 2] = Math.round(parseFloat(transform.rot[i]) || 0)
                  }}
                  onChange={(e) => applyRotationNumberInput(i, e.target.value)}
                  onBlur={() => {
                    if (!is2D && i <= 2) {
                      commitRotationNumberInput(i)
                      rotEditingRef.current = false
                      return
                    }
                    formatRotationOnBlur(i)
                  }}
                  onMouseUp={() => {
                    if (is2D || i > 2) return
                    commitRotationNumberInput(i)
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      (e.target as HTMLInputElement).blur()
                      return
                    }
                    if (is2D || i > 2) return
                    if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return
                    e.preventDefault()
                    const prevDeg = Math.round(parseFloat(transform.rot[i]) || 0)
                    const step = typeof rotStep === 'number' ? rotStep : 1
                    const nextDeg = prevDeg + (e.key === 'ArrowUp' ? step : -step)
                    commitRotAxis3d(i as 0 | 1 | 2, nextDeg, prevDeg)
                    rotFocusDegRef.current[i as 0 | 1 | 2] = nextDeg
                  }}
                />
              </div>
              <input
                id={`transform-rot-range-${ax}`}
                type="range"
                className="form-range mb-0"
                min={rotMin}
                max={rotMax}
                step={rotStep}
                value={Number(transform.rot[i]) || 0}
                aria-label={`${is2D ? t('Rotation (xyzw)') : t('Rotation (degrees)')} ${ax}`}
                onPointerDown={() => { rotDraggingRef.current = true }}
                onPointerUp={() => { rotDraggingRef.current = false }}
                onPointerCancel={() => { rotDraggingRef.current = false }}
                onChange={(e) => {
                  if (is2D) {
                    const next = [...transform.rot] as [string, string, string, string]
                    next[i] = parseFloat(e.target.value).toFixed(2)
                    setTransform((prev) => ({ ...prev, rot: next }))
                    commit({ rot: next })
                    return
                  }
                  const newDeg = Math.round(parseFloat(e.target.value))
                  const prevDeg = Math.round(parseFloat(transform.rot[i]) || 0)
                  commitRotAxis3d(i as 0 | 1 | 2, newDeg, prevDeg)
                }}
              />
            </div>
          ))}
        </div>
      </div>
      )}
    </>
  )
}

export default TransformPanel;
