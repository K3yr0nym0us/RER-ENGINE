import { useState, useEffect, useRef } from 'react';

import { Accordion } from 'react-bootstrap';
import { Plus } from 'react-bootstrap-icons';
import { AnimationAccordion } from './AnimationAccordion';

import { useContextEngine } from '../../../context/useContextEngine';

interface AnimationFrame {
  path:    string
  pivot_x: number
  pivot_y: number
}

interface Animation {
  name:       string
  fps:        number
  loop:       boolean
  logical_w:  number
  logical_h:  number
  audio_path?: string
  frames:     AnimationFrame[]
}

export function AnimationsPanel() {
  const { selectedEntity: entity, send, sendAsync, setAnimationPlaying, updateEntityAnimations, registerPivotEditListener, unregisterPivotEditListener, animationPlaying } = useContextEngine()
  const [animations, setAnimations] = useState<Animation[]>([])
  const [newAnimName, setNewAnimName] = useState('')
  const [editingPivot, setEditingPivot] = useState<{ animIdx: number; frameIdx: number } | null>(null)
  const [editingLogicalArea, setEditingLogicalArea] = useState<number | null>(null)

  useEffect(() => {
    // Limpiar modos de edición al cambiar de entidad
    if (editingPivot !== null) {
      send({ cmd: 'cancel_pivot_edit_mode' })
      unregisterPivotEditListener()
      setEditingPivot(null)
    }
    if (editingLogicalArea !== null) {
      send({ cmd: 'cancel_logical_area_mode' })
      setEditingLogicalArea(null)
    }
    setAnimations(entity?.animations ?? [])
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entity?.id])

  const addAnimation = () => {
    if (!newAnimName.trim()) return
    const next: Animation[] = [
      ...animations,
      { name: newAnimName.trim(), fps: 12, loop: true, logical_w: 64, logical_h: 64, frames: [] },
    ]
    setAnimations(next)
    setNewAnimName('')
    if (entity?.id) updateEntityAnimations?.(entity.id, next)
  }

  const addFrame = async (animIdx: number) => {
    const path = await window.electronAPI.openScenarioDialog()
    if (!path) return
    const next = animations.map((a, i) =>
      i !== animIdx ? a : {
        ...a,
        frames: [
          ...a.frames,
          { path, pivot_x: Math.round(a.logical_w / 2), pivot_y: a.logical_h },
        ],
      }
    )
    setAnimations(next)
    if (entity?.id) updateEntityAnimations?.(entity.id, next)
  }

  const removeFrame = (animIdx: number, frameIdx: number) => {
    const next = animations.map((a, i) =>
      i !== animIdx ? a : { ...a, frames: a.frames.filter((_, fi) => fi !== frameIdx) }
    )
    setAnimations(next)
    if (entity?.id) updateEntityAnimations?.(entity.id, next)
  }

  const moveFrame = (animIdx: number, frameIdx: number, direction: -1 | 1) => {
    const anim   = animations[animIdx]
    const newIdx = frameIdx + direction
    if (newIdx < 0 || newIdx >= anim.frames.length) return
    const frames = [...anim.frames]
    const temp = frames[frameIdx]; frames[frameIdx] = frames[newIdx]; frames[newIdx] = temp
    const next = animations.map((a, i) => i !== animIdx ? a : { ...a, frames })
    setAnimations(next)
    if (entity?.id) updateEntityAnimations?.(entity.id, next)
  }

  const updateFramePivot = (animIdx: number, frameIdx: number, field: 'pivot_x' | 'pivot_y', value: number) => {
    const next = animations.map((a, i) =>
      i !== animIdx ? a : {
        ...a,
        frames: a.frames.map((f, fi) => fi !== frameIdx ? f : { ...f, [field]: value }),
      }
    )
    setAnimations(next)
    if (entity?.id) updateEntityAnimations?.(entity.id, next)
  }

  const updateAnimation = (index: number, updates: Partial<Animation>) => {
    const next = animations.map((a, i) => i !== index ? a : { ...a, ...updates })
    setAnimations(next)
    if (entity?.id) updateEntityAnimations?.(entity.id, next)
    if (editingLogicalArea === index && entity?.id) {
      const updated = next[index]
      send({ cmd: 'set_logical_area_mode', id: entity.id, w: updated.logical_w ?? 64, h: updated.logical_h ?? 64 })
    }
  }

  const addAudio = async (animIdx: number) => {
    const path = await window.electronAPI.openAudioDialog()
    if (!path) return
    updateAnimation(animIdx, { audio_path: path })
  }

  const clearAudio = (animIdx: number) => {
    updateAnimation(animIdx, { audio_path: undefined })
  }

  const toggleLogicalArea = (animIdx: number) => {
    if (!entity) return
    if (editingLogicalArea === animIdx) {
      send({ cmd: 'cancel_logical_area_mode' })
      setEditingLogicalArea(null)
    } else {
      if (editingLogicalArea !== null) {
        send({ cmd: 'cancel_logical_area_mode' })
      }
      const anim = animations[animIdx]
      send({ cmd: 'set_logical_area_mode', id: entity.id, w: anim.logical_w ?? 64, h: anim.logical_h ?? 64 })
      setEditingLogicalArea(animIdx)
    }
  }

const removeAnimation = (index: number) => {
    // Stop animation if this one is playing
    if (animationPlaying.get(entity?.id ?? 0)) {
      send({ cmd: 'stop_animation', id: entity?.id })
    }
    const next = animations.filter((_, i) => i !== index)
    setAnimations(next)
    if (entity?.id) updateEntityAnimations?.(entity.id, next)
  }

  const playAnimation = async (animIdx: number) => {
    if (!entity) return
    const anim = animations[animIdx]
    if (!anim || anim.frames.length === 0) return

    // El motor ya tiene los datos sincronizados vía updateEntityAnimations.
    // Solo enviar el comando de reproducción por nombre.
    if (anim.loop) {
      send({ cmd: 'play_animation', id: entity.id, name: anim.name })
    } else {
      await sendAsync(
        { cmd: 'play_animation', id: entity.id, name: anim.name },
        'animation_finished',
        () => setAnimationPlaying(entity.id, true)
      )
    }
  }

  const startPivotEdit = (animIdx: number, frameIdx: number) => {
    if (!entity) return
    const frame = animations[animIdx]?.frames[frameIdx]
    if (!frame) return

    if (editingPivot?.animIdx === animIdx && editingPivot?.frameIdx === frameIdx) {
      send({ cmd: 'cancel_pivot_edit_mode' })
      unregisterPivotEditListener()
      setEditingPivot(null)
      return
    }

    if (editingPivot !== null) {
      send({ cmd: 'cancel_pivot_edit_mode' })
      unregisterPivotEditListener()
    }

    send({ cmd: 'set_pivot_edit_mode', id: entity.id, frame_path: frame.path, pivot_x: frame.pivot_x ?? 0, pivot_y: frame.pivot_y ?? 0 })

    registerPivotEditListener((framePath: string, px: number, py: number) => {
      setAnimations(prev => {
        const next = prev.map((a, ai) =>
          ai !== animIdx ? a : {
            ...a,
            frames: a.frames.map((f, fi) =>
              fi !== frameIdx ? f : { ...f, pivot_x: Math.round(px), pivot_y: Math.round(py) }
            ),
          }
        )
        if (entity?.id) updateEntityAnimations?.(entity.id, next)
        return next
      })
      unregisterPivotEditListener()
      setEditingPivot(null)
      void framePath
    })

    setEditingPivot({ animIdx, frameIdx })
  }

  return (
    <Accordion.Item eventKey="animaciones">
      <Accordion.Header>Animaciones</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <div className="d-flex gap-2 mb-2">
          <input
            type="text"
            placeholder="Nueva animación..."
            value={newAnimName}
            onChange={(e) => setNewAnimName(e.target.value)}
            className="form-control form-control-sm bg-dark text-light border-secondary"
            onKeyDown={(e) => { if (e.key === 'Enter') addAnimation() }}
          />
          <button className="btn btn-sm btn-outline-info" title="Nueva animación" onClick={addAnimation}>
            <Plus size={14} />
          </button>
        </div>

        {animations.length === 0 && (
          <div className="alert alert-secondary py-1 text-center" role="alert">
            Sin animaciones. Agrega una nueva para empezar.
          </div>
        )}

        {animations.length > 0 && (
          <Accordion>
            {animations.map((anim, idx) => (
              <AnimationAccordion
                key={idx}
                anim={anim}
                animIdx={idx}
                editingPivot={editingPivot}
                editingLogicalArea={editingLogicalArea}
                onPlay={() => playAnimation(idx)}
                onRemove={() => removeAnimation(idx)}
                onUpdateFps={(fps) => updateAnimation(idx, { fps })}
                onUpdateLoop={(loop) => updateAnimation(idx, { loop })}
                onUpdateLogicalW={(w) => updateAnimation(idx, { logical_w: w })}
                onUpdateLogicalH={(h) => updateAnimation(idx, { logical_h: h })}
                onToggleLogicalArea={() => toggleLogicalArea(idx)}
                onAddAudio={() => addAudio(idx)}
                onClearAudio={() => clearAudio(idx)}
                onAddFrame={() => addFrame(idx)}
                onRemoveFrame={(frameIdx) => removeFrame(idx, frameIdx)}
                onMoveFrame={(frameIdx, dir) => moveFrame(idx, frameIdx, dir)}
                onUpdateFramePivot={(frameIdx, field, val) => updateFramePivot(idx, frameIdx, field, val)}
                onStartPivotEdit={(frameIdx) => startPivotEdit(idx, frameIdx)}
              />
            ))}
          </Accordion>
        )}
      </Accordion.Body>
    </Accordion.Item>
  )
}

export default AnimationsPanel