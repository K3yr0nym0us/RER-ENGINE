import { useEffect, useReducer, useRef } from 'react';

import { PlayFill, StopFill, XCircleFill } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import type { SelectionMode } from './';
import { useAudioPreview } from '@hooks';
import { useTraslate } from '@hooks';

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
  isDefaultAnimation: boolean;
  onDefaultAnimationChange: (value: boolean) => void;
  fps: number;
  onFpsChange: (value: number) => void;
  isLooping: boolean;
  onLoopChange: (value: boolean) => void;
  onSelectedFrameChange?: (index: number) => void;
  pivotsByFrameIndex?: Record<number, { x: number; y: number }>;
  onPivotChange?: (index: number, pivot: { x: number; y: number }) => void;
  audioPath?: string;
}

const CANVAS_SIZE = 500;

interface PlaybackState {
  selectedFrameIndex: number;
  isPlaying: boolean;
}

type PlaybackAction =
  | { type: 'start' }
  | { type: 'stop' }
  | { type: 'select_frame'; payload: number }
  | { type: 'tick'; payload: { frameCount: number; isLooping: boolean } };

const initialPlaybackState: PlaybackState = {
  selectedFrameIndex: 0,
  isPlaying: false,
};

function playbackReducer(state: PlaybackState, action: PlaybackAction): PlaybackState {
  switch (action.type) {
    case 'start':
      return { selectedFrameIndex: 0, isPlaying: true };
    case 'stop':
      return { ...state, isPlaying: false };
    case 'select_frame':
      return { selectedFrameIndex: action.payload, isPlaying: false };
    case 'tick': {
      const { frameCount, isLooping } = action.payload;
      if (frameCount === 0) return { ...state, isPlaying: false };

      const nextIndex = state.selectedFrameIndex + 1;
      if (nextIndex < frameCount) {
        return { ...state, selectedFrameIndex: nextIndex };
      }

      if (isLooping) {
        return { ...state, selectedFrameIndex: 0 };
      }

      return { selectedFrameIndex: frameCount - 1, isPlaying: false };
    }
    default:
      return state;
  }
}

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
  isDefaultAnimation,
  onDefaultAnimationChange,
  fps,
  onFpsChange,
  isLooping,
  onLoopChange,
  onSelectedFrameChange,
  pivotsByFrameIndex,
  onPivotChange,
  audioPath,
}: SpritePreviewRightPanelProps) {
  const [{ selectedFrameIndex, isPlaying }, dispatch] = useReducer(playbackReducer, initialPlaybackState);
  const { t } = useTraslate();
  useAudioPreview(audioPath, isPlaying, isLooping);
  const animIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
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
  const currentPivot = (pivotsByFrameIndex?.[safeIndex]) ?? { x: 0.5, y: 1.0 };

  useEffect(() => {
    if (!hasFrames) return;
    onSelectedFrameChange?.(safeIndex);
  }, [hasFrames, safeIndex, onSelectedFrameChange]);

  // Animation effect
  useEffect(() => {
    if (isPlaying && hasFrames) {
      const interval = setInterval(() => {
        dispatch({ type: 'tick', payload: { frameCount: frames.length, isLooping } });
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

  const drawSpaceW = Math.max(1, ...frames.map((f) => f.width));
  const drawSpaceH = Math.max(1, ...frames.map((f) => f.height));

  // Draw current frame dentro del espacio de dibujo (sin estirar a un cuadrado)
  useEffect(() => {
    if (!currentFrame || !previewCanvasRef.current) return;

    const canvas = previewCanvasRef.current;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const img = new window.Image();
    img.onload = () => {
      const sheetScale = Math.min(CANVAS_SIZE / img.width, CANVAS_SIZE / img.height);
      const sheetDrawW = img.width * sheetScale;
      const sheetDrawH = img.height * sheetScale;
      const sheetOffX = (CANVAS_SIZE - sheetDrawW) / 2;
      const sheetOffY = (CANVAS_SIZE - sheetDrawH) / 2;

      const frameLeft = currentFrame.x;
      const frameRight = currentFrame.x + currentFrame.width;
      const sampleLeft = frameLeft - sheetOffX;
      const sampleRight = frameRight - sheetOffX;
      const origX = sampleLeft / sheetScale;
      const origY = (currentFrame.y - sheetOffY) / sheetScale;
      const origW = (sampleRight - sampleLeft) / sheetScale;
      const origH = currentFrame.height / sheetScale;

      const previewSize = 120;
      const spaceScale = Math.min(previewSize / drawSpaceW, previewSize / drawSpaceH);
      const spaceDispW = drawSpaceW * spaceScale;
      const spaceDispH = drawSpaceH * spaceScale;
      const spaceOffX = (previewSize - spaceDispW) / 2;
      const spaceOffY = (previewSize - spaceDispH) / 2;
      const frameOffX = (drawSpaceW - currentFrame.width) / 2;
      const frameOffY = drawSpaceH - currentFrame.height;
      const frameDispX = spaceOffX + frameOffX * spaceScale;
      const frameDispY = spaceOffY + frameOffY * spaceScale;
      const frameDispW = currentFrame.width * spaceScale;
      const frameDispH = currentFrame.height * spaceScale;

      canvas.width = previewSize;
      canvas.height = previewSize;
      ctx.clearRect(0, 0, previewSize, previewSize);
      ctx.strokeStyle = 'rgba(255, 132, 38, 0.9)';
      ctx.lineWidth = 1;
      ctx.strokeRect(spaceOffX + 0.5, spaceOffY + 0.5, spaceDispW - 1, spaceDispH - 1);
      ctx.drawImage(img, origX, origY, origW, origH, frameDispX, frameDispY, frameDispW, frameDispH);

      const px = spaceOffX + currentPivot.x * spaceDispW;
      const py = spaceOffY + currentPivot.y * spaceDispH;
      ctx.save();
      ctx.strokeStyle = '#ffde59';
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(px - 10, py);
      ctx.lineTo(px + 10, py);
      ctx.moveTo(px, py - 10);
      ctx.lineTo(px, py + 10);
      ctx.stroke();
      ctx.fillStyle = '#ffde59';
      ctx.beginPath();
      ctx.arc(px, py, 3, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();
    };
    img.onerror = () => {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
    };
    img.src = src;
  }, [currentFrame, src, currentPivot.x, currentPivot.y, drawSpaceW, drawSpaceH]);

  const handlePreviewCanvasClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!hasFrames) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const previewSize = 120;
    const clickX = ((e.clientX - rect.left) / rect.width) * previewSize;
    const clickY = ((e.clientY - rect.top) / rect.height) * previewSize;
    const spaceScale = Math.min(previewSize / drawSpaceW, previewSize / drawSpaceH);
    const spaceDispW = drawSpaceW * spaceScale;
    const spaceDispH = drawSpaceH * spaceScale;
    const spaceOffX = (previewSize - spaceDispW) / 2;
    const spaceOffY = (previewSize - spaceDispH) / 2;
    const nx = Math.max(0, Math.min(1, (clickX - spaceOffX) / spaceDispW));
    const ny = Math.max(0, Math.min(1, (clickY - spaceOffY) / spaceDispH));
    onPivotChange?.(safeIndex, { x: nx, y: ny });
  };

  const handlePlayStop = () => {
    if (isPlaying) {
      dispatch({ type: 'stop' });
    } else {
      if (!hasFrames) return;
      dispatch({ type: 'start' });
    }
  };

  return (
    <div className="bg-dark text-light border border-secondary rounded p-3 h-100 d-flex flex-column" style={{ minWidth: 220 }}>
      <h5 className="text-light text-center">{t('Preview')}</h5>
      <hr className="border-secondary mb-3 mt-1" />

      <div className="mb-3">
        <label className="text-light text-center fw-bold d-block" htmlFor="preview-canvas" id="preview-canvas-label">{t('Frame preview')}</label>
        <div className="bg-dark d-flex align-items-center justify-content-center mt-3">
          {currentFrame && (
            <AppTooltip
              content={t('Click frame to reassign pivot point')}
              place="left"
              tooltipClassName="app-tooltip--compact"
            >
              <canvas 
                className="border border-primary"
                ref={previewCanvasRef}
                id="preview-canvas"
                aria-labelledby="preview-canvas-label"
                style={{ cursor: 'crosshair' }}
                onClick={handlePreviewCanvasClick}
              />
            </AppTooltip>
          )}
          {!currentFrame && (
            <p className="text-muted small mb-0 text-center">
              {t('To see preview first create a box or select a cell')}
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

            <AppTooltip content={isPlaying ? t('Stop') : t('Play')} place="top">
              <button
                className={`btn btn-sm ${isPlaying ? 'btn-danger' : 'btn-success'}`}
                onClick={handlePlayStop}
                disabled={!hasFrames}
              >
                {isPlaying ? <StopFill /> : <PlayFill />}
              </button>
            </AppTooltip>

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
                    onClick={() => dispatch({ type: 'select_frame', payload: index })}
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
          <label className="text-light fw-bold d-block mb-1" htmlFor="animation-name-input">{t('Animation name')}</label>
          <input
            id="animation-name-input"
            className="form-control form-control-sm bg-dark text-light border-secondary"
            placeholder="Ej: run, idle, attack"
            value={animationName}
            onChange={(e) => onAnimationNameChange(e.target.value)}
          />
          <div className="form-check mt-2">
            <input
              className="form-check-input"
              type="checkbox"
              id="default-animation-check"
              checked={isDefaultAnimation}
              onChange={(e) => onDefaultAnimationChange(e.target.checked)}
            />
            <label className="form-check-label small" htmlFor="default-animation-check">
              {t('Default animation')}
            </label>
          </div>
        </div>
      )}
    </div>
  );
}
