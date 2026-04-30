import { useState, useCallback } from 'react';
import { SelectionMode } from './SpritePreviewModalBody';
import { Link, Unlock } from 'react-bootstrap-icons';

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
  onAddBox
}: SpritePreviewLeftPanelProps) {
  const [box, setBox] = useState(DEFAULT_BOX);
  const [keepAspect, setKeepAspect] = useState(true);

  const handleBoxWidthChange = useCallback((width: number) => {
    setBox(b => {
      const newHeight = keepAspect ? width : b.height;
      const updated = { ...b, width, height: newHeight };
      onBoxChange(updated);
      return updated;
    });
  }, [keepAspect, onBoxChange]);

  const handleBoxHeightChange = useCallback((height: number) => {
    setBox(b => {
      const newWidth = keepAspect ? height : b.width;
      const updated = { ...b, width: newWidth, height };
      onBoxChange(updated);
      return updated;
    });
  }, [keepAspect, onBoxChange]);

  return (
    <div className="bg-dark text-light border border-secondary rounded p-3 h-100">
      <h5 className="text-light text-center mb-3">Propiedades</h5>
      <hr className="border-secondary opacity-50 mb-3" />

      <div className="mb-3">
        <label className="text-light fw-bold d-block mb-2">Modo de selección</label>
        <div className="d-flex gap-4 justify-content-center">
          <div className="form-check d-flex align-items-center gap-1">
            <input
              className="form-check-input"
              type="radio"
              checked={selectionMode === 'cell'}
              onChange={() => setSelectionMode('cell')}
              id="mode-cell"
            />
            <label className="form-check-label" htmlFor="mode-cell">
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
            />
            <label className="form-check-label" htmlFor="mode-box">
              Recuadro
            </label>
          </div>
        </div>
      </div>

      {selectionMode === 'cell' && (
        <div className="mb-3">
          <label className="text-light fw-bold d-block mb-2">Tamaño de celda</label>
          <div className="d-flex align-items-center gap-2 mb-3">
            <input
              type="range"
              className="form-range flex-fill"
              min={8}
              max={CANVAS_SIZE}
              step={1}
              value={gridSize}
              onChange={e => setGridSize(Number(e.target.value))}
            />
            <input
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

          <label className="text-light fw-bold d-block mb-2">Desplazar cuadrícula</label>

          <div className="mb-2">
            <div className="d-flex align-items-center gap-2">
              <span className="text-secondary small fw-bold" style={{ width: 20 }}>X</span>
              <input
                type="range"
                className="form-range flex-fill"
                min={-CANVAS_SIZE}
                max={CANVAS_SIZE}
                step={1}
                value={cellOffsetX}
                onChange={e => setCellOffsetX(Number(e.target.value))}
              />
              <input
                type="number"
                className="form-control form-control-sm bg-dark text-light border-secondary"
                style={{ width: 70 }}
                min={-CANVAS_SIZE}
                max={CANVAS_SIZE}
                step={1}
                value={cellOffsetX}
                onChange={e => setCellOffsetX(Number(e.target.value))}
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
          <label className="text-light fw-bold d-block mb-2">Tamaño recuadro</label>

          <div className="mb-2">
            <div className="d-flex align-items-center gap-2">
              <span className="text-secondary small fw-bold" style={{ width: 20 }}>W</span>
              <input
                type="range"
                className="form-range flex-fill"
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={box.width}
                onChange={e => handleBoxWidthChange(Number(e.target.value))}
              />
              <input
                type="number"
                className="form-control form-control-sm bg-dark text-light border-secondary"
                style={{ width: 70 }}
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={box.width}
                onChange={e => handleBoxWidthChange(Number(e.target.value))}
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
    </div>
  );
}
