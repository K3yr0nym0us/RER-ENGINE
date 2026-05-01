import { useState, useEffect, useRef } from 'react';

import { PlayFill, StopFill, XCircleFill } from 'react-bootstrap-icons';
import { SelectionMode } from './SpritePreviewModalBody';

interface SpritePreviewRightPanelProps {
  src: string;
  selectionMode: SelectionMode;
  selectedCells: { x: number; y: number }[];
  boxes: { x: number; y: number; width: number; height: number }[];
  gridSize: number;
  cellOffsetX: number;
  cellOffsetY: number;
  onRemoveBox?: (index: number) => void;
  animationName: string;
  onAnimationNameChange: (value: string) => void;
  fps: number;
  onFpsChange: (value: number) => void;
  isLooping: boolean;
  onLoopChange: (value: boolean) => void;
}

const CANVAS_SIZE = 500;

export function SpritePreviewRightPanel({
  src,
  selectionMode,
  selectedCells,
  boxes,
  gridSize,
  cellOffsetX,
  cellOffsetY,
  onRemoveBox,
  animationName,
  onAnimationNameChange,
  fps,
  onFpsChange,
  isLooping,
  onLoopChange,
}: SpritePreviewRightPanelProps) {
  const [selectedFrameIndex, setSelectedFrameIndex] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const animIntervalRef = useRef<number | null>(null);
  const previewCanvasRef = useRef<HTMLCanvasElement>(null);

  const frames = selectionMode === 'cell'
    ? selectedCells.map(cell => ({
        x: cell.x * gridSize - cellOffsetX,
        y: cell.y * gridSize - cellOffsetY,
        width: gridSize,
        height: gridSize
      }))
    : boxes;

  const hasFrames = frames.length > 0;
  const safeIndex = hasFrames ? Math.min(selectedFrameIndex, frames.length - 1) : 0;
  const currentFrame = hasFrames ? frames[safeIndex] : null;

  // Animation effect
  useEffect(() => {
    if (isPlaying && hasFrames) {
      const interval = setInterval(() => {
        setSelectedFrameIndex(prev => {
          const next = prev + 1;
          if (next >= frames.length) {
            if (isLooping) {
              return 0;
            } else {
              setIsPlaying(false);
              return frames.length - 1;
            }
          }
          return next;
        });
      }, 1000 / fps);

      animIntervalRef.current = interval;
      return () => clearInterval(interval);
    } else {
      if (animIntervalRef.current) {
        clearInterval(animIntervalRef.current);
        animIntervalRef.current = null;
      }
    }
  }, [isPlaying, isLooping, fps, hasFrames, frames.length]);

  // Reset animation when frames change
  useEffect(() => {
    setSelectedFrameIndex(0);
    setIsPlaying(false);
  }, [selectionMode, selectedCells, boxes]);

  // Draw current frame
  useEffect(() => {
    if (!currentFrame || !previewCanvasRef.current) return;

    const canvas = previewCanvasRef.current;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const img = new window.Image();
    img.onload = () => {
      const scale = Math.min(CANVAS_SIZE / img.width, CANVAS_SIZE / img.height);
      const drawWidth = img.width * scale;
      const drawHeight = img.height * scale;
      const offsetX = (CANVAS_SIZE - drawWidth) / 2;
      const offsetY = (CANVAS_SIZE - drawHeight) / 2;

      const origX = (currentFrame.x - offsetX) / scale;
      const origY = (currentFrame.y - offsetY) / scale;
      const origW = currentFrame.width / scale;
      const origH = currentFrame.height / scale;

      const size = 120;
      canvas.width = size;
      canvas.height = size;
      ctx.clearRect(0, 0, size, size);
      ctx.drawImage(img, origX, origY, origW, origH, 0, 0, size, size);
    };
    img.onerror = () => {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
    };
    img.src = src;
  }, [currentFrame, src]);

  const handlePlayStop = () => {
    if (isPlaying) {
      setIsPlaying(false);
    } else {
      if (!hasFrames) return;
      setSelectedFrameIndex(0);
      setIsPlaying(true);
    }
  };

  return (
    <div className="bg-dark text-light border border-secondary rounded p-3 h-100 d-flex flex-column" style={{ minWidth: 220 }}>
      <h5 className="text-light text-center mb-3">Previsualización</h5>
      <hr className="border-secondary mb-3" />

      <div className="mb-3">
        <label className="text-light text-center fw-bold d-block" htmlFor="preview-canvas" id="preview-canvas-label">Vista previa del frame</label>
        <div className="bg-dark d-flex align-items-center justify-content-center mt-3">
          {currentFrame && (
            <canvas 
              className="border border-primary"
              ref={previewCanvasRef}
              id="preview-canvas"
              aria-labelledby="preview-canvas-label"
            />
          )}
          {!currentFrame && (
            <p className="text-muted small mb-0 text-center">
              Para ver la vista previa primero debe crear un cuadro o seleccionar una celda
            </p>
          )}
        </div>
      </div>

      {hasFrames && (
        <div className="mb-3">
          <div className="d-flex align-items-center justify-content-between mb-2">
            <div className="form-check d-flex align-items-center gap-2 mb-0">
              <input
                className="form-check-input"
                type="checkbox"
                id="loop-check"
                checked={isLooping}
                onChange={e => onLoopChange(e.target.checked)}
              />
              <label className="form-check-label d-flex align-items-center" htmlFor="loop-check">
                Loop
              </label>
            </div>

            <button
              className={`btn btn-sm ${isPlaying ? 'btn-danger' : 'btn-success'}`}
              onClick={handlePlayStop}
              disabled={!hasFrames}
              title={isPlaying ? 'Detener' : 'Reproducir'}
            >
              {isPlaying ? <StopFill /> : <PlayFill />}
            </button>

            <div className="d-flex align-items-center gap-1">
              <label className="text-light small mb-0" htmlFor="preview-fps">FPS</label>
              <input
                id="preview-fps"
                type="number"
                className="form-control form-control-sm bg-dark text-light border-secondary"
                style={{ width: '4vw' }}
                min={1}
                max={60}
                value={fps}
                onChange={e => onFpsChange(Math.max(1, Math.min(60, Number(e.target.value) || 1)))}
              />
            </div>
          </div>
        </div>
      )}

      <hr className="mt-0" />

      {hasFrames && (
        <div className="mb-3">
          <label className="text-light fw-bold d-block mb-2" aria-hidden="true" role="presentation">Frames ({frames.length})</label>
          <nav>
            <ul className="pagination pagination-sm flex-wrap mb-0">
              {frames.map((frame, index) => (
                <li key={`${frame.x}-${frame.y}-${frame.width}-${frame.height}`} className={`page-item ${index === safeIndex ? 'active' : ''}`}>
                  <button
                    className="page-link d-flex align-items-center gap-1"
                    onClick={() => {
                      if (isPlaying) setIsPlaying(false);
                      setSelectedFrameIndex(index);
                    }}
                  >
                    {index + 1}
                    {selectionMode === 'box' && onRemoveBox && (
                      <XCircleFill
                        role="button"
                        className="text-danger"
                        style={{ fontSize: '0.8em' }}
                        onClick={(e) => {
                          e.stopPropagation();
                          onRemoveBox(index);
                        }}
                      />
                    )}
                  </button>
                </li>
              ))}
            </ul>
          </nav>
        </div>
      )}

      {hasFrames && (
        <div className="mt-auto pt-2 border-top border-secondary">
          <label className="text-light fw-bold d-block mb-1">Nombre de la animacion</label>
          <input
            className="form-control form-control-sm bg-dark text-light border-secondary"
            placeholder="Ej: run, idle, attack"
            value={animationName}
            onChange={(e) => onAnimationNameChange(e.target.value)}
          />
        </div>
      )}
    </div>
  );
}
