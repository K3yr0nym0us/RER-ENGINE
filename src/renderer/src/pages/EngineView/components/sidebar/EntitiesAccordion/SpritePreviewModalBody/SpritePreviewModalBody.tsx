import { useState, useCallback, useMemo, useEffect, useRef } from 'react';

import { SpritePreviewLeftPanel } from './SpritePreviewLeftPanel';
import { SpritePreviewCanvas } from './SpritePreviewCanvas';
import { SpritePreviewRightPanel } from './SpritePreviewRightPanel';

import { useSpritePreviewImage } from '../../../../../../hooks/useSpritePreviewImage';

const CANVAS_SIZE = 500;
export type SelectionMode = 'cell' | 'box';

export interface SpriteFrameRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface SpritePreviewConfirmConfig {
  animationName: string;
  frames: SpriteFrameRect[];
  fps: number;
  loop: boolean;
}

export function SpritePreviewModalBody({ 
  src,
  onConfirm,
  onCancel,
  initialAnimationName,
  initialFrames,
  initialFps,
  initialLoop,
}: { 
  src: string
  onConfirm?: (config: SpritePreviewConfirmConfig) => void
  onCancel?: () => void
  initialAnimationName?: string
  initialFrames?: SpriteFrameRect[]
  initialFps?: number
  initialLoop?: boolean
}) {
  const { imageSrc, imageSize } = useSpritePreviewImage(src);
  const [animationName, setAnimationName] = useState('');
  const [validationError, setValidationError] = useState<string | null>(null);
  const [cellOffsetX, setCellOffsetX] = useState(0);
  const [cellOffsetY, setCellOffsetY] = useState(0);
  const [gridSize, setGridSize] = useState(32);
  const [selectionMode, setSelectionMode] = useState<SelectionMode>('cell');
  const [selectedCells, setSelectedCells] = useState<{ x: number, y: number }[]>([]);
  const [boxes, setBoxes] = useState<{ x: number, y: number, width: number, height: number }[]>([]);
  const [currentBox, setCurrentBox] = useState({ x: 0, y: 0, width: 64, height: 64 });
  const [fps, setFps] = useState(12);
  const [isLooping, setIsLooping] = useState(false);
  const initialLoadedRef = useRef(false);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'z') {
        e.preventDefault();
        setBoxes(prev => prev.slice(0, -1));
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  useEffect(() => {
    if (!initialAnimationName) return;
    setAnimationName(initialAnimationName);
  }, [initialAnimationName]);

  useEffect(() => {
    if (typeof initialFps === 'number') {
      setFps(Math.max(1, Math.min(60, initialFps)));
    }
  }, [initialFps]);

  useEffect(() => {
    if (typeof initialLoop === 'boolean') {
      setIsLooping(initialLoop);
    }
  }, [initialLoop]);

  useEffect(() => {
    if (initialLoadedRef.current) return;
    if (!imageSize || !imageSrc) return;
    if (!initialFrames || initialFrames.length === 0) return;

    const imgW = imageSize.width;
    const imgH = imageSize.height;
    const scale = Math.min(CANVAS_SIZE / imgW, CANVAS_SIZE / imgH);
    const drawWidth = imgW * scale;
    const drawHeight = imgH * scale;
    const drawOffsetX = (CANVAS_SIZE - drawWidth) / 2;
    const drawOffsetY = (CANVAS_SIZE - drawHeight) / 2;

    const initialBoxes = initialFrames.map((frame) => ({
      x: drawOffsetX + frame.x * scale,
      y: drawOffsetY + frame.y * scale,
      width: frame.width * scale,
      height: frame.height * scale,
    }));

    setSelectionMode('box');
    setBoxes(initialBoxes);
    initialLoadedRef.current = true;
  }, [imageSize, imageSrc, initialFrames]);

  const rightPanelKey = useMemo(() => {
    if (selectionMode === 'cell') {
      return `${imageSrc}-cell-${selectedCells.map((c) => `${c.x}:${c.y}`).join('|')}`;
    }
    return `${imageSrc}-box-${boxes.map((b) => `${b.x}:${b.y}:${b.width}:${b.height}`).join('|')}`;
  }, [imageSrc, selectionMode, selectedCells, boxes]);

  const handleRemoveBox = useCallback((index: number) => {
    setBoxes(prev => prev.filter((_, i) => i !== index));
  }, []);

  const handleCanvasClick = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    if (selectionMode === 'cell') {
      const rect = e.currentTarget.getBoundingClientRect();
      const x = Math.floor((e.clientX - rect.left + cellOffsetX) / gridSize);
      const y = Math.floor((e.clientY - rect.top + cellOffsetY) / gridSize);

      setSelectedCells(prev => {
        const exists = prev.some(cell => cell.x === x && cell.y === y);
        return exists
          ? prev.filter(cell => !(cell.x === x && cell.y === y))
          : [...prev, { x, y }];
      });
    } else if (selectionMode === 'box') {
      setBoxes(prev => [...prev, { ...currentBox }]);
    }
  }, [selectionMode, cellOffsetX, cellOffsetY, gridSize, currentBox]);

  const handleMouseMove = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    if (selectionMode !== 'box') return;

    const rect = e.currentTarget.getBoundingClientRect();
    const mouseX = Math.floor(e.clientX - rect.left);
    const mouseY = Math.floor(e.clientY - rect.top);

    setCurrentBox(b => {
      const width = b.width;
      const height = b.height;
      let x = mouseX - width / 2;
      let y = mouseY - height / 2;
      x = Math.max(0, Math.min(x, CANVAS_SIZE - width));
      y = Math.max(0, Math.min(y, CANVAS_SIZE - height));
      return { ...b, x, y };
    });
  }, [selectionMode]);

  const handleBoxChange = useCallback((box: { x: number; y: number; width: number; height: number }) => {
    setCurrentBox(box);
  }, []);

  const handleAddBox = useCallback(() => {
    setBoxes(prev => [...prev, { ...currentBox }]);
  }, [currentBox]);

  const selectedFrameCount = selectionMode === 'cell' ? selectedCells.length : boxes.length;

  const normalizedFrames = useMemo(() => {
    if (!imageSize || !imageSrc) return [] as SpriteFrameRect[];

    const imgW = imageSize.width;
    const imgH = imageSize.height;
    const scale = Math.min(CANVAS_SIZE / imgW, CANVAS_SIZE / imgH);
    const drawWidth = imgW * scale;
    const drawHeight = imgH * scale;
    const drawOffsetX = (CANVAS_SIZE - drawWidth) / 2;
    const drawOffsetY = (CANVAS_SIZE - drawHeight) / 2;

    const toSourceRect = (x: number, y: number, width: number, height: number): SpriteFrameRect | null => {
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
    };

    if (selectionMode === 'cell') {
      const frames: SpriteFrameRect[] = [];
      for (const cell of selectedCells) {
        const canvasX = cell.x * gridSize - cellOffsetX;
        const canvasY = cell.y * gridSize - cellOffsetY;
        const rect = toSourceRect(canvasX, canvasY, gridSize, gridSize);
        if (rect) frames.push(rect);
      }
      return frames;
    }

    const frames: SpriteFrameRect[] = [];
    for (const box of boxes) {
      const rect = toSourceRect(box.x, box.y, box.width, box.height);
      if (rect) frames.push(rect);
    }
    return frames;
  }, [imageSize, imageSrc, selectionMode, selectedCells, boxes, gridSize, cellOffsetX, cellOffsetY]);

  const handleConfirm = () => {
    const cleanName = animationName.trim();
    if (!cleanName) {
      setValidationError('Debes escribir un nombre para la animacion.');
      return;
    }
    if (normalizedFrames.length === 0) {
      setValidationError('Debes seleccionar al menos 1 frame valido.');
      return;
    }
    setValidationError(null);
    onConfirm?.({
      animationName: cleanName,
      frames: normalizedFrames,
      fps,
      loop: isLooping,
    });
  };

  return (
    <div>
      <div data-bs-theme="dark" className="row g-3 p-3 rounded-3" style={{ minHeight: 300 }}>
        <div className="col-3">
          <SpritePreviewLeftPanel
            selectionMode={selectionMode}
            setSelectionMode={setSelectionMode}
            gridSize={gridSize}
            setGridSize={setGridSize}
            cellOffsetX={cellOffsetX}
            setCellOffsetX={setCellOffsetX}
            cellOffsetY={cellOffsetY}
            setCellOffsetY={setCellOffsetY}
            CANVAS_SIZE={CANVAS_SIZE}
            onBoxChange={handleBoxChange}
            onAddBox={handleAddBox}
          />
        </div>

        <div className="col">
          <SpritePreviewCanvas
            src={imageSrc}
            selectionMode={selectionMode}
            selectedCells={selectedCells}
            boxes={boxes}
            box={currentBox}
            gridSize={gridSize}
            cellOffsetX={cellOffsetX}
            cellOffsetY={cellOffsetY}
            onCanvasClick={handleCanvasClick}
            onMouseMove={selectionMode === 'box' ? handleMouseMove : undefined}
            CANVAS_SIZE={CANVAS_SIZE}
          />
        </div>

        <div className="col-3">
          <SpritePreviewRightPanel
            key={rightPanelKey}
            src={imageSrc}
            selectionMode={selectionMode}
            selectedCells={selectedCells}
            boxes={boxes}
            gridSize={gridSize}
            cellOffsetX={cellOffsetX}
            cellOffsetY={cellOffsetY}
            onRemoveBox={handleRemoveBox}
            animationName={animationName}
            onAnimationNameChange={(value: string) => {
              setAnimationName(value);
              if (validationError) setValidationError(null);
            }}
            fps={fps}
            onFpsChange={setFps}
            isLooping={isLooping}
            onLoopChange={setIsLooping}
          />
        </div>
      </div>

      {validationError && (
        <div className="alert alert-danger py-2 px-3 mt-2 mb-0">
          {validationError}
        </div>
      )}
      
      {(onConfirm || onCancel) && (
        <div className="d-flex gap-2 justify-content-end mt-3 px-3">
          {onCancel && (
            <button className="btn btn-secondary btn-sm" onClick={onCancel}>
              Cancelar
            </button>
          )}
          {onConfirm && (
            <button 
              className="btn btn-primary btn-sm" 
              onClick={handleConfirm}
              disabled={selectedFrameCount === 0 || !imageSrc}
            >
              Confirmar
            </button>
          )}
        </div>
      )}
    </div>
  );
}
