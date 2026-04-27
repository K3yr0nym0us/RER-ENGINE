import React, { useState } from 'react'
import { Accordion, Spinner } from 'react-bootstrap'
import { Plus, Trash, ArrowLeft, PlayFill, Crosshair, Pencil, MusicNoteBeamed } from 'react-bootstrap-icons'
import { useContextEngine } from '../../../context/useContextEngine'

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

interface AnimationAccordionProps {
  anim: Animation
  animIdx: number
  editingPivot: { animIdx: number; frameIdx: number } | null
  editingLogicalArea: number | null
  onPlay: () => void
  onRemove: () => void
  onUpdateFps: (fps: number) => void
  onUpdateLoop: (loop: boolean) => void
  onUpdateLogicalW: (w: number) => void
  onUpdateLogicalH: (h: number) => void
  onToggleLogicalArea: () => void
  onAddAudio: () => void
  onClearAudio: () => void
  onAddFrame: () => void
  onRemoveFrame: (frameIdx: number) => void
  onMoveFrame: (frameIdx: number, direction: -1 | 1) => void
  onUpdateFramePivot: (frameIdx: number, field: 'pivot_x' | 'pivot_y', value: number) => void
  onStartPivotEdit: (frameIdx: number) => void
}

export function AnimationAccordion({
  anim,
  animIdx,
  editingPivot,
  editingLogicalArea,
  onPlay,
  onRemove,
  onUpdateFps,
  onUpdateLoop,
  onUpdateLogicalW,
  onUpdateLogicalH,
  onToggleLogicalArea,
  onAddAudio,
  onClearAudio,
  onAddFrame,
  onRemoveFrame,
  onMoveFrame,
  onUpdateFramePivot,
  onStartPivotEdit,
}: AnimationAccordionProps) {
  const { selectedEntity: entity, animationPlaying } = useContextEngine()
  const isPlaying = entity ? animationPlaying.get(entity.id) ?? false : false

  const frameInputW = (
    <div className="d-flex align-items-center gap-2 mb-2">
      <label className="form-label small mb-0 text-secondary">W</label>
      <input
        type="number"
        className="form-control form-control-sm"
        style={{ width: 60 }}
        value={anim.logical_w ?? 64}
        disabled={editingLogicalArea !== animIdx}
        onChange={(e) => onUpdateLogicalW(parseInt(e.target.value) || 64)}
      />
      <label className="form-label small mb-0 text-secondary">H</label>
      <input
        type="number"
        className="form-form-control form-control-sm"
        style={{ width: 60 }}
        value={anim.logical_h ?? 64}
        disabled={editingLogicalArea !== animIdx}
        onChange={(e) => onUpdateLogicalH(parseInt(e.target.value) || 64)}
      />
      <button
        className={`btn btn-sm p-1 ms-auto ${editingLogicalArea === animIdx ? 'btn-warning' : 'btn-outline-secondary'}`}
        title={editingLogicalArea === animIdx ? 'Ocultar área lógica' : 'Habilitar edición'}
        onClick={onToggleLogicalArea}
      >
        <Crosshair size={12} />
      </button>
    </div>
  )

  return (
    <Accordion.Item eventKey={`anim-${animIdx}`}>
      <Accordion.Header>
        <span
          role="button"
          tabIndex={0}
          className={`me-2 cursor-pointer ${isPlaying ? 'text-warning' : 'text-info'}`}
          style={{ lineHeight: 1 }}
          title={isPlaying ? 'Reproduciendo...' : 'Reproducir'}
          onClick={(e) => { e.stopPropagation(); onPlay() }}
          onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.stopPropagation(); onPlay() } }}
        >
          {isPlaying ? <Spinner animation="border" size="sm" /> : <PlayFill size={12} />}
        </span>
        <span className="me-auto">{anim.name}</span>
        <span
          role="button"
          tabIndex={0}
          className="ms-2 text-danger cursor-pointer"
          style={{ lineHeight: 1 }}
          title="Eliminar animación"
          onClick={(e) => { e.stopPropagation(); onRemove() }}
          onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.stopPropagation(); onRemove() } }}
        >
          <Trash size={12} />
        </span>
      </Accordion.Header>

      <Accordion.Body className="py-2 px-2">
        <div className="d-flex align-items-center gap-2 mb-2">
          <label className="form-label small mb-0 text-secondary">FPS</label>
          <input
            type="number"
            className="form-control form-control-sm"
            style={{ width: 60 }}
            value={anim.fps}
            onChange={(e) => onUpdateFps(parseInt(e.target.value) || 12)}
          />
          <input
            type="checkbox"
            className="form-check-input"
            checked={anim.loop}
            onChange={(e) => onUpdateLoop(e.target.checked)}
          />
          <label className="form-check-label small">Loop</label>
          {anim.audio_path ? (
            <MusicNoteBeamed size={14} className="text-success ms-auto" title={anim.audio_path} />
          ) : (
            <button className="btn btn-sm p-0 ms-auto" title="Agregar audio" onClick={onAddAudio}>
              <MusicNoteBeamed size={14} />
            </button>
          )}
          {anim.audio_path && (
            <button className="btn btn-sm p-0 text-danger" title="Quitar audio" onClick={onClearAudio}>
              <Trash size={12} />
            </button>
          )}
        </div>

        {frameInputW}

        <div className="d-flex gap-1 mb-2">
          <button className="btn btn-outline-light btn-sm flex-grow-1" onClick={onAddFrame}>
            <Plus size={12} /> Frame
          </button>
        </div>

        {anim.frames.length > 0 && (
          <div className="d-flex flex-column gap-1">
            {editingPivot === null && (
              <div className="small text-secondary mb-1">Frames ({anim.frames.length})</div>
            )}
            {anim.frames.map((frame, frameIdx) => (
              <div key={frameIdx} className="d-flex align-items-center gap-1">
                <button
                  className="btn btn-sm p-1"
                  disabled={frameIdx === 0}
                  onClick={() => onMoveFrame(frameIdx, -1)}
                >
                  <ArrowLeft size={10} />
                </button>
                <span className="small text-truncate" style={{ maxWidth: 120 }} title={frame.path}>
                  {frame.path.split('/').pop()}
                </span>
                <span className="small text-muted">
                  ({frame.pivot_x},{frame.pivot_y})
                </span>
                <button
                  className={`btn btn-sm p-1 ms-auto ${editingPivot?.animIdx === animIdx && editingPivot?.frameIdx === frameIdx ? 'btn-warning' : 'btn-outline-info'}`}
                  title={editingPivot?.animIdx === animIdx && editingPivot?.frameIdx === frameIdx ? 'Cancelar edición de pivot' : 'Editar pivot (click en viewport)'}
                  onClick={() => onStartPivotEdit(frameIdx)}
                >
                  <Pencil size={10} />
                </button>
                <button
                  className="btn btn-sm p-1 text-danger"
                  onClick={() => onRemoveFrame(frameIdx)}
                >
                  <Trash size={10} />
                </button>
                <button
                  className="btn btn-sm p-1"
                  disabled={frameIdx === anim.frames.length - 1}
                  onClick={() => onMoveFrame(frameIdx, 1)}
                >
                  <ArrowLeft size={10} style={{ transform: 'scaleX(-1)' }} />
                </button>
              </div>
            ))}
          </div>
        )}
      </Accordion.Body>
    </Accordion.Item>
  )
}