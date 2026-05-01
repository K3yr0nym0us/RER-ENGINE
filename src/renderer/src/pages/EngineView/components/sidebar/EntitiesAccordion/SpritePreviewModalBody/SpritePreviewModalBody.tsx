import { useCallback, useEffect, useMemo, useReducer, useRef } from 'react';

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

interface SpritePreviewState {
  animationName: string;
  validationError: string | null;
  cellOffsetX: number;
  cellOffsetY: number;
  gridSize: number;
  selectionMode: SelectionMode;
  selectedCells: { x: number; y: number }[];
  boxes: { x: number; y: number; width: number; height: number }[];
  currentBox: { x: number; y: number; width: number; height: number };
  fps: number;
  isLooping: boolean;
}

type SpritePreviewAction =
  | { type: 'patch'; payload: Partial<SpritePreviewState> }
  | { type: 'toggle_cell'; payload: { x: number; y: number } }
  | { type: 'append_current_box' }
  | { type: 'remove_box'; payload: number }
  | { type: 'pop_box' };

const initialSpritePreviewState: SpritePreviewState = {
  animationName: '',
  validationError: null,
  cellOffsetX: 0,
  cellOffsetY: 0,
  gridSize: 32,
  selectionMode: 'cell',
  selectedCells: [],
  boxes: [],
  currentBox: { x: 0, y: 0, width: 64, height: 64 },
  fps: 12,
  isLooping: false,
};

function spritePreviewReducer(state: SpritePreviewState, action: SpritePreviewAction): SpritePreviewState {
  switch (action.type) {
    case 'patch':
      return { ...state, ...action.payload };
    case 'toggle_cell': {
      const { x, y } = action.payload;
      const exists = state.selectedCells.some((cell) => cell.x === x && cell.y === y);
      return {
        ...state,
        selectedCells: exists
          ? state.selectedCells.filter((cell) => !(cell.x === x && cell.y === y))
          : [...state.selectedCells, { x, y }],
      };
    }
    case 'append_current_box':
      return { ...state, boxes: [...state.boxes, { ...state.currentBox }] };
    case 'remove_box':
      return { ...state, boxes: state.boxes.filter((_, i) => i !== action.payload) };
    case 'pop_box':
      return { ...state, boxes: state.boxes.slice(0, -1) };
    default:
      return state;
  }
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
  const [state, dispatch] = useReducer(spritePreviewReducer, initialSpritePreviewState);
  const initialLoadedRef = useRef(false);

  const {
    animationName,
    validationError,
    cellOffsetX,
    cellOffsetY,
    gridSize,
    selectionMode,
    selectedCells,
    boxes,
    currentBox,
    fps,
    isLooping,
  } = state;

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'z') {
        e.preventDefault();
        dispatch({ type: 'pop_box' });
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  useEffect(() => {
    if (!initialAnimationName) return;
    dispatch({ type: 'patch', payload: { animationName: initialAnimationName } });
  }, [initialAnimationName]);

  useEffect(() => {
    if (typeof initialFps === 'number') {
      dispatch({ type: 'patch', payload: { fps: Math.max(1, Math.min(60, initialFps)) } });
    }
  }, [initialFps]);

  useEffect(() => {
    if (typeof initialLoop === 'boolean') {
      dispatch({ type: 'patch', payload: { isLooping: initialLoop } });
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

    dispatch({
      type: 'patch',
      payload: {
        selectionMode: 'box',
        boxes: initialBoxes,
      },
    });
    initialLoadedRef.current = true;
  }, [imageSize, imageSrc, initialFrames]);

  const rightPanelKey = useMemo(() => {
    if (selectionMode === 'cell') {
      return `${imageSrc}-cell-${selectedCells.map((c) => `${c.x}:${c.y}`).join('|')}`;
    }
    return `${imageSrc}-box-${boxes.map((b) => `${b.x}:${b.y}:${b.width}:${b.height}`).join('|')}`;
  }, [imageSrc, selectionMode, selectedCells, boxes]);

  const handleRemoveBox = useCallback((index: number) => {
    dispatch({ type: 'remove_box', payload: index });
  }, []);

  const handleCanvasClick = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    if (selectionMode === 'cell') {
      const rect = e.currentTarget.getBoundingClientRect();
      const x = Math.floor((e.clientX - rect.left + cellOffsetX) / gridSize);
      const y = Math.floor((e.clientY - rect.top + cellOffsetY) / gridSize);
      dispatch({ type: 'toggle_cell', payload: { x, y } });
      return;
    }

    if (selectionMode === 'box') {
      dispatch({ type: 'append_current_box' });
    }
  }, [selectionMode, cellOffsetX, cellOffsetY, gridSize]);

  const handleMouseMove = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    if (selectionMode !== 'box') return;

    const rect = e.currentTarget.getBoundingClientRect();
    const mouseX = Math.floor(e.clientX - rect.left);
    const mouseY = Math.floor(e.clientY - rect.top);

    const width = currentBox.width;
    const height = currentBox.height;
    let x = mouseX - width / 2;
    let y = mouseY - height / 2;
    x = Math.max(0, Math.min(x, CANVAS_SIZE - width));
    y = Math.max(0, Math.min(y, CANVAS_SIZE - height));

    dispatch({ type: 'patch', payload: { currentBox: { ...currentBox, x, y } } });
  }, [selectionMode, currentBox]);

  const handleBoxChange = useCallback((box: { x: number; y: number; width: number; height: number }) => {
    dispatch({ type: 'patch', payload: { currentBox: box } });
  }, []);

  const handleAddBox = useCallback(() => {
    dispatch({ type: 'append_current_box' });
  }, []);

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
      dispatch({ type: 'patch', payload: { validationError: 'Debes escribir un nombre para la animacion.' } });
      return;
    }
    if (normalizedFrames.length === 0) {
      dispatch({ type: 'patch', payload: { validationError: 'Debes seleccionar al menos 1 frame valido.' } });
      return;
    }

    dispatch({ type: 'patch', payload: { validationError: null } });
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
            setSelectionMode={(mode) => dispatch({ type: 'patch', payload: { selectionMode: mode } })}
            gridSize={gridSize}
            setGridSize={(size) => dispatch({ type: 'patch', payload: { gridSize: size } })}
            cellOffsetX={cellOffsetX}
            setCellOffsetX={(offset) => dispatch({ type: 'patch', payload: { cellOffsetX: offset } })}
            cellOffsetY={cellOffsetY}
            setCellOffsetY={(offset) => dispatch({ type: 'patch', payload: { cellOffsetY: offset } })}
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
              dispatch({
                type: 'patch',
                payload: {
                  animationName: value,
                  validationError: validationError ? null : validationError,
                },
              });
            }}
            fps={fps}
            onFpsChange={(value) => dispatch({ type: 'patch', payload: { fps: value } })}
            isLooping={isLooping}
            onLoopChange={(value) => dispatch({ type: 'patch', payload: { isLooping: value } })}
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
