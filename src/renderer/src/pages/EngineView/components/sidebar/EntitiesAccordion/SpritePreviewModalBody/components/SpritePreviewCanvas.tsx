import { useRef, useEffect } from 'react';

import { InfoCircle } from 'react-bootstrap-icons';
import { AppTooltip } from '@components';

import { useTraslate } from '@hooks';

interface SpritePreviewCanvasProps {
  src: string;
  facingRight: boolean;
  onFacingRightChange: (value: boolean) => void;
  selectionMode: 'cell' | 'box';
  selectedCells: { x: number; y: number }[];
  boxes: { x: number; y: number; width: number; height: number }[];
  box: { x: number; y: number; width: number; height: number };
  gridSize: number;
  cellOffsetX: number;
  cellOffsetY: number;
  onCanvasClick: (e: React.MouseEvent<HTMLCanvasElement>) => void;
  onMouseMove?: (e: React.MouseEvent<HTMLCanvasElement>) => void;
  CANVAS_SIZE: number;
}

export function SpritePreviewCanvas({
  src,
  facingRight,
  onFacingRightChange,
  selectionMode,
  selectedCells,
  boxes,
  box,
  gridSize,
  cellOffsetX,
  cellOffsetY,
  onCanvasClick,
  onMouseMove,
  CANVAS_SIZE,
}: SpritePreviewCanvasProps) {
  const { t } = useTraslate();
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    ctx.clearRect(0, 0, CANVAS_SIZE, CANVAS_SIZE);

    const img = new window.Image();
    img.onload = () => {
      const scale = Math.min(CANVAS_SIZE / img.width, CANVAS_SIZE / img.height);
      const drawWidth = img.width * scale;
      const drawHeight = img.height * scale;
      const offsetX = (CANVAS_SIZE - drawWidth) / 2;
      const offsetY = (CANVAS_SIZE - drawHeight) / 2;

      ctx.clearRect(0, 0, CANVAS_SIZE, CANVAS_SIZE);
      ctx.drawImage(img, offsetX, offsetY, drawWidth, drawHeight);

      if (selectionMode === 'cell') {
        ctx.strokeStyle = 'rgba(255,255,255,0.64)';
        ctx.lineWidth = 1;

        const startX = Math.floor(cellOffsetX / gridSize) * gridSize;
        for (let x = startX; x < CANVAS_SIZE + gridSize; x += gridSize) {
          const drawX = x - cellOffsetX;
          ctx.beginPath();
          ctx.moveTo(drawX, 0);
          ctx.lineTo(drawX, CANVAS_SIZE);
          ctx.stroke();
        }

        const startY = Math.floor(cellOffsetY / gridSize) * gridSize;
        for (let y = startY; y < CANVAS_SIZE + gridSize; y += gridSize) {
          const drawY = y - cellOffsetY;
          ctx.beginPath();
          ctx.moveTo(0, drawY);
          ctx.lineTo(CANVAS_SIZE, drawY);
          ctx.stroke();
        }

        ctx.fillStyle = 'rgba(0,200,255,0.25)';
        selectedCells.forEach(cell => {
          ctx.fillRect(
            cell.x * gridSize - cellOffsetX,
            cell.y * gridSize - cellOffsetY,
            gridSize,
            gridSize
          );
        });
      }

      if (selectionMode === 'box') {
        ctx.strokeStyle = 'rgba(0,200,255,0.8)';
        ctx.lineWidth = 2;
        boxes.forEach(b =>
          ctx.strokeRect(b.x, b.y, b.width, b.height)
        );
        ctx.setLineDash([6, 4]);
        ctx.strokeRect(box.x, box.y, box.width, box.height);
        ctx.setLineDash([]);
      }

    };
    img.onerror = () => {
      ctx.clearRect(0, 0, CANVAS_SIZE, CANVAS_SIZE);
    };
    img.src = src;
  }, [src, gridSize, selectionMode, selectedCells, box, boxes, cellOffsetX, cellOffsetY, CANVAS_SIZE]);

  return (
    <div className="text-center">
      <div className="mb-2">
        <div className="d-flex justify-content-center align-items-center gap-2 mb-1">
          <span className="text-light small fw-semibold">{t('Animation orientation')}</span>
          <AppTooltip
            content={t('Animation orientation info')}
            place="top"
          >
            <span className="d-inline-flex align-items-center" role="button" aria-label={t('Animation orientation info aria')}>
              <InfoCircle size={15} className="text-info" />
            </span>
          </AppTooltip>
        </div>
        <div className="d-flex justify-content-center align-items-center gap-4">
          <div className="form-check d-flex align-items-center gap-1 m-0">
            <input
              className="form-check-input m-0"
              type="radio"
              id="facing-left"
              name="facing-direction"
              checked={!facingRight}
              onChange={() => onFacingRightChange(false)}
            />
            <label className="form-check-label small text-light m-0" htmlFor="facing-left">
              {t('Left')}
            </label>
          </div>
          <div className="form-check d-flex align-items-center gap-1 m-0">
            <input
              className="form-check-input m-0"
              type="radio"
              id="facing-right"
              name="facing-direction"
              checked={facingRight}
              onChange={() => onFacingRightChange(true)}
            />
            <label className="form-check-label small text-light m-0" htmlFor="facing-right">
              {t('Right')}
            </label>
          </div>
        </div>
      </div>
      <canvas
        ref={canvasRef}
        width={CANVAS_SIZE}
        height={CANVAS_SIZE}
        className="border border-secondary bg-dark"
        style={{
          cursor: selectionMode === 'box' ? 'crosshair' : 'default'
        }}
        onMouseMove={onMouseMove}
        onClick={onCanvasClick}
      />
    </div>
  );
}
