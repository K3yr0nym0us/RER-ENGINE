import { useMemo } from 'react';
import { CANVAS_SIZE, SelectionMode, SpriteFrameRect } from './spritePreviewReducer';

interface UseNormalizedFramesParams {
  imageSize: { width: number; height: number } | null;
  imageSrc: string;
  selectionMode: SelectionMode;
  selectedCells: { x: number; y: number }[];
  boxes: { x: number; y: number; width: number; height: number }[];
  gridSize: number;
  cellOffsetX: number;
  cellOffsetY: number;
}

function toSourceRect(
  x: number,
  y: number,
  width: number,
  height: number,
  drawOffsetX: number,
  drawOffsetY: number,
  drawWidth: number,
  drawHeight: number,
  imgW: number,
  imgH: number,
  scale: number,
): SpriteFrameRect | null {
  const left = Math.max(x, drawOffsetX);
  const top = Math.max(y, drawOffsetY);
  const right = Math.min(x + width, drawOffsetX + drawWidth);
  const bottom = Math.min(y + height, drawOffsetY + drawHeight);

  if (right <= left || bottom <= top) return null;

  const srcX = Math.max(0, Math.min(imgW - 1, Math.round((left - drawOffsetX) / scale)));
  const srcY = Math.max(0, Math.min(imgH - 1, Math.round((top - drawOffsetY) / scale)));
  const srcRight = Math.max(srcX + 1, Math.min(imgW, Math.round((right - drawOffsetX) / scale)));
  const srcBottom = Math.max(srcY + 1, Math.min(imgH, Math.round((bottom - drawOffsetY) / scale)));

  return {
    x: srcX,
    y: srcY,
    width: Math.max(1, srcRight - srcX),
    height: Math.max(1, srcBottom - srcY),
  };
}

export function useNormalizedFrames({
  imageSize,
  imageSrc,
  selectionMode,
  selectedCells,
  boxes,
  gridSize,
  cellOffsetX,
  cellOffsetY,
}: UseNormalizedFramesParams): SpriteFrameRect[] {
  return useMemo(() => {
    if (!imageSize || !imageSrc) return [];

    const imgW = imageSize.width;
    const imgH = imageSize.height;
    const scale = Math.min(CANVAS_SIZE / imgW, CANVAS_SIZE / imgH);
    const drawWidth = imgW * scale;
    const drawHeight = imgH * scale;
    const drawOffsetX = (CANVAS_SIZE - drawWidth) / 2;
    const drawOffsetY = (CANVAS_SIZE - drawHeight) / 2;

    if (selectionMode === 'cell') {
      const frames: SpriteFrameRect[] = [];
      for (const cell of selectedCells) {
        const canvasX = cell.x * gridSize - cellOffsetX;
        const canvasY = cell.y * gridSize - cellOffsetY;
        const rect = toSourceRect(canvasX, canvasY, gridSize, gridSize, drawOffsetX, drawOffsetY, drawWidth, drawHeight, imgW, imgH, scale);
        if (rect) frames.push(rect);
      }
      return frames;
    }

    const frames: SpriteFrameRect[] = [];
    for (const box of boxes) {
      const rect = toSourceRect(box.x, box.y, box.width, box.height, drawOffsetX, drawOffsetY, drawWidth, drawHeight, imgW, imgH, scale);
      if (rect) frames.push(rect);
    }
    return frames;
  }, [imageSize, imageSrc, selectionMode, selectedCells, boxes, gridSize, cellOffsetX, cellOffsetY]);
}
