import { useState, useCallback } from 'react';
import { SelectionMode, type ScriptEntry } from './';
import { Link, Unlock, MusicNoteBeamed, Trash, FileEarmarkCode, PencilSquare } from 'react-bootstrap-icons';
import AppTooltip from '../../../../../../../components/AppTooltip';

interface SpritePreviewLeftPanelProps {
  selectionMode: SelectionMode;
  setSelectionMode: (mode: SelectionMode) => void;
  gridSize: number;
  setGridSize: (size: number) => void;
  cellOffsetX: number;
  setCellOffsetX: (offset: number) => void;
  cellOffsetY: number;
  setCellOffsetY: (offset: number) => void;
  CANVAS_SIZE: number;
  onBoxChange: (box: { x: number; y: number; width: number; height: number }) => void;
  onAddBox: () => void;
  audioPath?: string;
  onAddAudio: () => void;
  onClearAudio: () => void;
  scripts: ScriptEntry[];
  onAddScript: () => void;
  onEditScript: (name: string) => void;
  onRemoveScript: (name: string) => void;
}

const DEFAULT_BOX = { x: 0, y: 0, width: 64, height: 64 };

export function SpritePreviewLeftPanel({
  selectionMode,
  setSelectionMode,
  gridSize,
  setGridSize,
  cellOffsetX,
  setCellOffsetX,
  cellOffsetY,
  setCellOffsetY,
  CANVAS_SIZE,
  onBoxChange,
  onAddBox,
  audioPath,
  onAddAudio,
  onClearAudio,
  scripts,
  onAddScript,
  onEditScript,
  onRemoveScript,
}: SpritePreviewLeftPanelProps) {
  const [box, setBox] = useState(DEFAULT_BOX);
  const [keepAspect, setKeepAspect] = useState(true);

  const handleBoxWidthChange = useCallback((width: number) => {
    const newHeight = keepAspect ? width : box.height;
    const updated = { ...box, width, height: newHeight };
    setBox(updated);
    onBoxChange(updated);
  }, [keepAspect, onBoxChange, box]);

  const handleBoxHeightChange = useCallback((height: number) => {
    const newWidth = keepAspect ? height : box.width;
    const updated = { ...box, width: newWidth, height };
    setBox(updated);
    onBoxChange(updated);
  }, [keepAspect, onBoxChange, box]);

  return (
    <div className="bg-dark text-light border border-secondary rounded p-3 h-100">
      <h5 className="text-light text-center mb-3">Propiedades</h5>
      <hr className="border-secondary opacity-50 mb-3" />

      <div className="mb-3">
        <label className="text-light fw-bold d-block mb-2" id="label-modo-seleccion" htmlFor="mode-cell">Modo de selección</label>
        <div className="d-flex gap-4 justify-content-center">
          <div className="form-check d-flex align-items-center gap-1">
            <input
              className="form-check-input"
              type="radio"
              checked={selectionMode === 'cell'}
              onChange={() => setSelectionMode('cell')}
              id="mode-cell"
              aria-labelledby="label-modo-seleccion mode-cell-label"
            />
            <label className="form-check-label" htmlFor="mode-cell" id="mode-cell-label">
              Celdas
            </label>
          </div>
          <div className="form-check d-flex align-items-center gap-1">
            <input
              className="form-check-input"
              type="radio"
              checked={selectionMode === 'box'}
              onChange={() => setSelectionMode('box')}
              id="mode-box"
              aria-labelledby="label-modo-seleccion mode-box-label"
            />
            <label className="form-check-label" htmlFor="mode-box" id="mode-box-label">
              Recuadro
            </label>
          </div>
        </div>
      </div>

      {selectionMode === 'cell' && (
        <div className="mb-3">
            <label className="text-light fw-bold d-block mb-2" htmlFor="grid-size">Tamaño de celda</label>
          <div className="d-flex align-items-center gap-2 mb-3">
            <input
              id="grid-size"
              type="range"
              className="form-range flex-fill"
              min={8}
              max={CANVAS_SIZE}
              step={1}
              value={gridSize}
              onChange={e => setGridSize(Number(e.target.value))}
            />
            <input
              id="grid-size-number"
              type="number"
              className="form-control form-control-sm bg-dark text-light border-secondary"
              style={{ width: 70 }}
              min={8}
              max={CANVAS_SIZE}
              step={1}
              value={gridSize}
              onChange={e => setGridSize(Number(e.target.value))}
            />
            <span className="text-secondary small">px</span>
          </div>

          <label className="text-light fw-bold d-block mb-2" htmlFor="offset-x-range">Desplazar cuadrícula</label>

          <div className="mb-2">
            <div className="d-flex align-items-center gap-2">
              <span className="text-secondary small fw-bold" style={{ width: 20 }}>X</span>
              <input
                id="offset-x-range"
                type="range"
                className="form-range flex-fill"
                min={-CANVAS_SIZE}
                max={CANVAS_SIZE}
                step={1}
                value={cellOffsetX}
                onChange={e => setCellOffsetX(Number(e.target.value))}
                aria-label="Desplazar cuadrícula X"
              />
              <input
                id="offset-x-number"
                type="number"
                className="form-control form-control-sm bg-dark text-light border-secondary"
                style={{ width: 70 }}
                min={-CANVAS_SIZE}
                max={CANVAS_SIZE}
                step={1}
                value={cellOffsetX}
                onChange={e => setCellOffsetX(Number(e.target.value))}
                aria-label="Desplazar cuadrícula X (número)"
              />
            </div>
          </div>

          <div className="mb-2">
            <div className="d-flex align-items-center gap-2">
              <span className="text-secondary small fw-bold" style={{ width: 20 }}>Y</span>
              <input
                type="range"
                className="form-range flex-fill"
                min={-CANVAS_SIZE}
                max={CANVAS_SIZE}
                step={1}
                value={cellOffsetY}
                onChange={e => setCellOffsetY(Number(e.target.value))}
              />
              <input
                type="number"
                className="form-control form-control-sm bg-dark text-light border-secondary"
                style={{ width: 70 }}
                min={-CANVAS_SIZE}
                max={CANVAS_SIZE}
                step={1}
                value={cellOffsetY}
                onChange={e => setCellOffsetY(Number(e.target.value))}
              />
            </div>
          </div>
        </div>
      )}

      {selectionMode === 'box' && (
        <div className="mb-3">
          <label className="text-light fw-bold d-block mb-2" htmlFor="box-width-range">Tamaño recuadro</label>

          <div className="mb-2">
            <div className="d-flex align-items-center gap-2">
              <span className="text-secondary small fw-bold" style={{ width: 20 }}>W</span>
              <input
                id="box-width-range"
                type="range"
                className="form-range flex-fill"
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={box.width}
                onChange={e => handleBoxWidthChange(Number(e.target.value))}
                aria-label="Tamaño recuadro ancho"
              />
              <input
                id="box-width-number"
                type="number"
                className="form-control form-control-sm bg-dark text-light border-secondary"
                style={{ width: 70 }}
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={box.width}
                onChange={e => handleBoxWidthChange(Number(e.target.value))}
                aria-label="Tamaño recuadro ancho (número)"
              />
              <span className="text-secondary small">px</span>
            </div>
          </div>

          <div className="mb-2">
            <div className="d-flex align-items-center gap-2">
              <span className="text-secondary small fw-bold" style={{ width: 20 }}>H</span>
              <input
                type="range"
                className="form-range flex-fill"
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={box.height}
                onChange={e => handleBoxHeightChange(Number(e.target.value))}
                disabled={keepAspect}
              />
              <input
                type="number"
                className="form-control form-control-sm bg-dark text-light border-secondary"
                style={{ width: 70 }}
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={box.height}
                onChange={e => handleBoxHeightChange(Number(e.target.value))}
                disabled={keepAspect}
              />
              <span className="text-secondary small">px</span>
            </div>
          </div>

          <button
            onClick={() => setKeepAspect(!keepAspect)}
            className={`btn w-100 ${keepAspect ? 'btn-primary' : 'btn-outline-secondary'}`}
            type="button"
          >
            {keepAspect ? <><Link className="me-1" /> Proporción fija</> : <><Unlock className="me-1" /> Proporción libre</>}
          </button>
        </div>
      )}

      <hr className="border-secondary opacity-50 mb-3" />

      <div className="mb-3">
        <label className="text-light fw-bold d-block mb-2">Audio de animación</label>
        {audioPath ? (
          <div className="d-flex align-items-center gap-1">
            <AppTooltip content={audioPath} place="top">
              <span className="text-success small text-truncate flex-fill" style={{ maxWidth: 120 }}>
                <MusicNoteBeamed className="me-1" />
                {audioPath.split(/[\\/]/).pop()}
              </span>
            </AppTooltip>
            <AppTooltip content="Quitar audio" place="top">
              <button className="btn btn-sm btn-outline-danger p-0 px-1" type="button" onClick={onClearAudio}>
                <Trash />
              </button>
            </AppTooltip>
          </div>
        ) : (
          <AppTooltip content="Cargar archivo de audio (wav/ogg/mp3)" place="top">
            <button
              className="btn btn-outline-secondary btn-sm w-100"
              type="button"
              onClick={onAddAudio}
            >
              <MusicNoteBeamed className="me-1" /> Agregar audio
            </button>
          </AppTooltip>
        )}
      </div>

      <hr className="border-secondary opacity-50 mb-3" />

      <div className="mb-3">
        <label className="text-light fw-bold d-block mb-2">Scripts Lua</label>

        {scripts.length > 0 && (
          <div className="d-flex flex-column gap-1 mb-2">
            {scripts.map((s) => (
              <div key={s.name} className="d-flex align-items-center gap-1">
                <AppTooltip content={s.name} place="top">
                  <span className="text-info small text-truncate flex-fill" style={{ maxWidth: 110 }}>
                    <FileEarmarkCode className="me-1" />{s.name}
                  </span>
                </AppTooltip>
                <AppTooltip content="Editar script" place="top">
                  <button className="btn btn-sm p-0 px-1 btn-outline-warning" type="button" onClick={() => onEditScript(s.name)}>
                    <PencilSquare size={12} />
                  </button>
                </AppTooltip>
                <AppTooltip content="Eliminar script" place="top">
                  <button className="btn btn-sm p-0 px-1 btn-outline-danger" type="button" onClick={() => onRemoveScript(s.name)}>
                    <Trash size={12} />
                  </button>
                </AppTooltip>
              </div>
            ))}
          </div>
        )}

        <AppTooltip content="Agregar script Lua a esta animación" place="top">
          <button
            className="btn btn-outline-secondary btn-sm w-100"
            type="button"
            onClick={onAddScript}
          >
            <FileEarmarkCode className="me-1" /> Agregar script
          </button>
        </AppTooltip>
      </div>

    </div>
  );
}
