import { useState, useEffect, useRef } from 'react'
import { Accordion } from 'react-bootstrap'
import { Plus, Trash, ArrowLeft, PlayFill, PauseFill, Crosshair, Pencil, MusicNoteBeamed } from 'react-bootstrap-icons'

import { useContextEngine } from '../../../context/useContextEngine'

interface AnimationFrame {
  path:    string
  /** Punto ancla en píxeles dentro del frame (0,0 = esquina superior-izquierda). */
  pivot_x: number
  pivot_y: number
}

interface Animation {
  name:       string
  fps:        number
  loop:       boolean
  /** Bounding box lógico fijo (píxeles). Define el tamaño referencia de la entidad. */
  logical_w:  number
  logical_h:  number
  /** Ruta del archivo de audio que se reproduce con la animación (wav/ogg/mp3). Opcional. */
  audio_path?: string
  frames:     AnimationFrame[]
}

export function AnimationsPanel() {
  const { selectedEntity: entity, send, updateEntityAnimations, registerPivotEditListener, unregisterPivotEditListener } = useContextEngine()
  const [animations, setAnimations] = useState<Animation[]>([])
  const [newAnimName, setNewAnimName] = useState('')
  const [playingAnim, setPlayingAnim] = useState<number | null>(null)
  const [editingPivot, setEditingPivot] = useState<{ animIdx: number; frameIdx: number } | null>(null)
  // Índice de la animación cuyo área lógica se muestra en el viewport
  const [editingLogicalArea, setEditingLogicalArea] = useState<number | null>(null)
  // Ref al interval activo y al id de la entidad que está animando
  const animationRef = useRef<{ interval: ReturnType<typeof setInterval>; entityId: number } | null>(null)
  // Siempre apunta a send actual para usarlo dentro de closures del interval
  const sendRef = useRef(send)
  sendRef.current = send
  // Mirror de editingPivot para cleanup en closures
  const editingPivotRef = useRef(editingPivot)
  editingPivotRef.current = editingPivot

  // Al cambiar entidad, detener cualquier animación en curso y cargar las de la nueva entidad
  useEffect(() => {
    if (animationRef.current) {
      clearInterval(animationRef.current.interval)
      animationRef.current = null
    }
    setPlayingAnim(null)
    // Cancelar modo pivot si estaba activo
    if (editingPivotRef.current !== null) {
      sendRef.current({ cmd: 'cancel_pivot_edit_mode' })
      unregisterPivotEditListener()
      setEditingPivot(null)
    }
    // Cancelar área lógica si estaba activa
    if (editingLogicalArea !== null) {
      sendRef.current({ cmd: 'cancel_logical_area_mode' })
      setEditingLogicalArea(null)
    }
    setAnimations(entity?.animations ?? [])
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entity?.id])

  const stopAnimation = (entityId: number) => {
    if (animationRef.current) {
      clearInterval(animationRef.current.interval)
      animationRef.current = null
    }
    setPlayingAnim(null)
    sendRef.current({ cmd: 'restore_animation_frame', id: entityId })
    sendRef.current({ cmd: 'stop_audio' })
  }

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
    // Actualizar overlay de área lógica en tiempo real si está activo para esta animación
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
    if (playingAnim === index && entity?.id) stopAnimation(entity.id)
    const next = animations.filter((_, i) => i !== index)
    setAnimations(next)
    if (entity?.id) updateEntityAnimations?.(entity.id, next)
  }

  const playAnimation = (animIdx: number) => {
    if (!entity) return
    const anim = animations[animIdx]
    if (!anim || anim.frames.length === 0) return

    // Detener si ya estaba reproduciendo esta misma
    if (playingAnim === animIdx) {
      stopAnimation(entity.id)
      return
    }

    // Detener cualquier otra animación en curso
    if (animationRef.current) {
      clearInterval(animationRef.current.interval)
      animationRef.current = null
    }

    // Capturar valores estables para el closure del interval
    const entityId  = entity.id
    const fps       = Math.max(1, anim.fps)
    const isLoop    = anim.loop
    const frames    = anim.frames
    const logicalW  = anim.logical_w
    const logicalH  = anim.logical_h
    let currentFrame = 0

    const sendFrame = (frameIdx: number) => {
      const f = frames[frameIdx]
      sendRef.current({
        cmd:       'play_animation_frame',
        id:        entityId,
        path:      f.path,
        pivot_x:   f.pivot_x   ?? 0,
        pivot_y:   f.pivot_y   ?? 0,
        logical_w: logicalW    || 0,
        logical_h: logicalH    || 0,
      })
    }

    // Enviar el primer frame inmediatamente
    sendFrame(0)
    setPlayingAnim(animIdx)

    // Reproducir audio en el motor si la animación tiene uno asignado
    if (anim.audio_path) {
      sendRef.current({ cmd: 'play_audio', path: anim.audio_path, loop_: isLoop })
    }

    const interval = setInterval(() => {
      currentFrame = (currentFrame + 1) % frames.length

      if (!isLoop && currentFrame === 0) {
        // Animación terminó: limpiar y restaurar sprite original
        clearInterval(interval)
        animationRef.current = null
        setPlayingAnim(null)
        sendRef.current({ cmd: 'restore_animation_frame', id: entityId })
        return
      }

      sendFrame(currentFrame)
    }, 1000 / fps)

    animationRef.current = { interval, entityId }
  }

  const startPivotEdit = (animIdx: number, frameIdx: number) => {
    if (!entity) return
    const frame = animations[animIdx]?.frames[frameIdx]
    if (!frame) return

    // Toggle: si ya se estaba editando este mismo frame, cancelar
    if (editingPivot?.animIdx === animIdx && editingPivot?.frameIdx === frameIdx) {
      send({ cmd: 'cancel_pivot_edit_mode' })
      unregisterPivotEditListener()
      setEditingPivot(null)
      return
    }

    // Si había otro frame activo, cancelar primero
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
      // El log de confirmación viene del motor, aquí solo limpiamos
      void framePath // supress unused warning
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

        {animations.length === 0 ? (
          <p className="text-secondary fst-italic small mb-0">Sin animaciones</p>
        ) : (
          <Accordion>
            {animations.map((anim, idx) => (
              <Accordion.Item key={idx} eventKey={`anim-${idx}`}>
                <Accordion.Header>
                  <span
                    role="button"
                    tabIndex={0}
                    className={`me-2 cursor-pointer ${playingAnim === idx ? 'text-warning' : 'text-info'}`}
                    style={{ lineHeight: 1 }}
                    title={playingAnim === idx ? 'Detener' : 'Reproducir'}
                    onClick={(e) => { e.stopPropagation(); playAnimation(idx) }}
                    onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.stopPropagation(); playAnimation(idx) } }}
                  >
                    {playingAnim === idx ? <PauseFill size={12} /> : <PlayFill size={12} />}
                  </span>
                  <span className="me-auto">{anim.name}</span>
                  <span
                    role="button"
                    tabIndex={0}
                    className="ms-2 text-danger cursor-pointer"
                    style={{ lineHeight: 1 }}
                    title="Eliminar animación"
                    onClick={(e) => { e.stopPropagation(); removeAnimation(idx) }}
                    onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.stopPropagation(); removeAnimation(idx) } }}
                  >
                    <Trash size={12} />
                  </span>
                </Accordion.Header>

                <Accordion.Body className="py-2 px-2">
                  {/* FPS + Loop */}
                  <div className="d-flex align-items-center gap-2 mb-2">
                    <label className="form-label small mb-0 text-secondary">FPS</label>
                    <input
                      type="number"
                      min={1} max={120}
                      value={anim.fps}
                      onChange={(e) => updateAnimation(idx, { fps: Math.max(1, Number(e.target.value)) })}
                      className="form-control form-control-sm bg-dark text-light border-secondary"
                      style={{ width: 60 }}
                    />
                    <div className="form-check mb-0 ms-2">
                      <input
                        type="checkbox"
                        id={`loop-${idx}`}
                        checked={anim.loop}
                        onChange={(e) => updateAnimation(idx, { loop: e.target.checked })}
                        className="form-check-input"
                      />
                      <label htmlFor={`loop-${idx}`} className="form-check-label small">Loop</label>
                    </div>
                  </div>

                  {/* Area Lógica */}
                  <div className="mb-2">
                    <div className="small text-secondary mb-1" title="Bounding box de referencia para la animación (píxeles)">Area Lógica</div>
                    <div className="d-flex align-items-center gap-2">
                      <input
                        type="number" min={1}
                        value={anim.logical_w ?? 64}
                        disabled={editingLogicalArea !== idx}
                        onChange={(e) => updateAnimation(idx, { logical_w: Math.max(1, Number(e.target.value)) })}
                        className="form-control form-control-sm bg-dark text-light border-secondary"
                        style={{ width: 60, opacity: editingLogicalArea !== idx ? 0.45 : 1 }}
                        title="Ancho lógico (px)"
                      />
                      <span className="text-secondary small">×</span>
                      <input
                        type="number" min={1}
                        value={anim.logical_h ?? 64}
                        disabled={editingLogicalArea !== idx}
                        onChange={(e) => updateAnimation(idx, { logical_h: Math.max(1, Number(e.target.value)) })}
                        className="form-control form-control-sm bg-dark text-light border-secondary"
                        style={{ width: 60, opacity: editingLogicalArea !== idx ? 0.45 : 1 }}
                        title="Alto lógico (px)"
                      />
                      <span className="text-secondary small">px</span>
                      <button
                        className={`btn btn-sm p-1 ms-auto ${editingLogicalArea === idx ? 'btn-warning' : 'btn-outline-secondary'}`}
                        title={editingLogicalArea === idx ? 'Ocultar área lógica' : 'Habilitar edición'}
                        onClick={() => toggleLogicalArea(idx)}
                      >
                        <Pencil size={11} />
                      </button>
                    </div>
                  </div>

                  {/* Audio */}
                  <div className="mb-2">
                    <div className="small text-secondary mb-1">Audio</div>
                    <div className="d-flex align-items-center gap-1">
                      <button
                        className="btn btn-sm btn-outline-secondary p-1"
                        title="Seleccionar audio (wav, ogg, mp3)"
                        onClick={() => addAudio(idx)}
                      >
                        <MusicNoteBeamed size={12} />
                      </button>
                      {anim.audio_path ? (
                        <>
                          <span className="small text-truncate flex-fill text-light" title={anim.audio_path} style={{ maxWidth: 120 }}>
                            {anim.audio_path.split('/').pop()}
                          </span>
                          <button
                            className="btn btn-sm btn-outline-danger p-1"
                            title="Quitar audio"
                            onClick={() => clearAudio(idx)}
                          >
                            <Trash size={11} />
                          </button>
                        </>
                      ) : (
                        <span className="small text-secondary fst-italic">Sin audio</span>
                      )}
                    </div>
                  </div>

                  {/* Lista de frames */}
                  <Accordion className="mb-2">
                    <Accordion.Item eventKey={`frames-${idx}`}>
                      <Accordion.Header>
                        <span className="me-auto">Frames ({anim.frames.length})</span>
                      </Accordion.Header>
                      <Accordion.Body className="py-2 px-2">
                        {anim.frames.length === 0 ? (
                          <p className="text-secondary fst-italic small mb-0">Sin frames</p>
                        ) : (
                          <ul className="list-unstyled mb-0">
                            {anim.frames.map((frame, frameIdx) => (
                              <li key={frameIdx} className="mb-2">
                                  <div
                                  className="p-1 rounded"
                                  style={{ background: 'var(--bs-dark)', border: '1px solid var(--bs-secondary)' }}
                                >
                                  {/* Fila 1: nombre del archivo */}
                                  <div className="text-truncate small text-light mb-1" title={frame.path}>
                                    {frame.path.split('/').pop()}
                                  </div>
                                  {/* Fila 2: controles ↑ ↓ 🗑 + editar pivot */}
                                  <div className="d-flex align-items-center gap-1 mb-1">
                                    {editingPivot === null && (
                                      <>
                                        <button
                                          className="btn btn-sm btn-outline-secondary p-1"
                                          title="Mover arriba"
                                          disabled={frameIdx === 0}
                                          onClick={() => moveFrame(idx, frameIdx, -1)}
                                        >
                                          <ArrowLeft size={12} style={{ transform: 'rotate(90deg)' }} />
                                        </button>
                                        <button
                                          className="btn btn-sm btn-outline-secondary p-1"
                                          title="Mover abajo"
                                          disabled={frameIdx === anim.frames.length - 1}
                                          onClick={() => moveFrame(idx, frameIdx, 1)}
                                        >
                                          <ArrowLeft size={12} style={{ transform: 'rotate(-90deg)' }} />
                                        </button>
                                      </>
                                    )}
                                    <button
                                      className="btn btn-sm btn-outline-danger p-1"
                                      title="Eliminar frame"
                                      onClick={() => removeFrame(idx, frameIdx)}
                                    >
                                      <Trash size={12} />
                                    </button>
                                    <button
                                      className={`btn btn-sm p-1 ms-auto ${editingPivot?.animIdx === idx && editingPivot?.frameIdx === frameIdx ? 'btn-warning' : 'btn-outline-info'}`}
                                      title={editingPivot?.animIdx === idx && editingPivot?.frameIdx === frameIdx ? 'Cancelar edición de pivot' : 'Editar pivot (click en viewport)'}
                                      onClick={() => startPivotEdit(idx, frameIdx)}
                                    >
                                      <Crosshair size={12} />
                                    </button>
                                  </div>
                                  {/* Fila 3: Pivot label */}
                                  <div className="small text-secondary mb-1" title="Punto ancla en píxeles dentro del frame">Pivot</div>
                                  {/* Fila 4: X/Y inputs */}
                                  <div className="d-flex align-items-center gap-1">
                                    <span className="small text-secondary">X</span>
                                    <input
                                      type="number" min={0}
                                      value={frame.pivot_x ?? 0}
                                      onChange={(e) => updateFramePivot(idx, frameIdx, 'pivot_x', Number(e.target.value))}
                                      className="form-control form-control-sm bg-dark text-light border-secondary"
                                      style={{ width: 56 }}
                                    />
                                    <span className="small text-secondary">Y</span>
                                    <input
                                      type="number" min={0}
                                      value={frame.pivot_y ?? 0}
                                      onChange={(e) => updateFramePivot(idx, frameIdx, 'pivot_y', Number(e.target.value))}
                                      className="form-control form-control-sm bg-dark text-light border-secondary"
                                      style={{ width: 56 }}
                                    />
                                    <span className="small text-secondary">px</span>
                                  </div>
                                </div>
                              </li>
                            ))}
                          </ul>
                        )}
                        <button
                          className="btn btn-sm btn-outline-info w-100 mt-2"
                          onClick={() => addFrame(idx)}
                        >
                          <Plus size={12} /> Agregar Frame
                        </button>
                      </Accordion.Body>
                    </Accordion.Item>
                  </Accordion>
                </Accordion.Body>
              </Accordion.Item>
            ))}
          </Accordion>
        )}
      </Accordion.Body>
    </Accordion.Item>
  )
}

export default AnimationsPanel