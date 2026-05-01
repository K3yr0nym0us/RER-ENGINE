import { useMemo, useReducer, useState, useEffect, useRef } from 'react';

import { SpritePreviewLeftPanel } from './SpritePreviewLeftPanel';
import { SpritePreviewCanvas } from './SpritePreviewCanvas';
import { SpritePreviewRightPanel } from './SpritePreviewRightPanel';
import { SpritePreviewFooter } from './SpritePreviewFooter';

import { useSpritePreviewImage } from '../../../../../../hooks/useSpritePreviewImage';
import { useNormalizedFrames } from './useNormalizedFrames';
import { useCanvasHandlers } from './useCanvasHandlers';
import { useInitialLoad } from './useInitialLoad';
import {
  CANVAS_SIZE,
  initialSpritePreviewState,
  spritePreviewReducer,
  type SpriteFrameRect,
} from './spritePreviewReducer';

export type { SelectionMode, SpriteFrameRect } from './spritePreviewReducer';

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
  const [state, dispatch] = useReducer(spritePreviewReducer, initialSpritePreviewState);
  const [defaultPivotNormalized] = useState<{ x: number; y: number }>({ x: 0.5, y: 0.5 });
  const selectedPreviewFrameIndexRef = useRef(0);
  const [pivotByFrameIndex, setPivotByFrameIndex] = useState<Record<number, { x: number; y: number }>>({});
  const initialPivotsLoadedRef = useRef(false);

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

  useInitialLoad({
    dispatch,
    imageSize,
    imageSrc,
    initialAnimationName,
    initialFrames,
    initialFps,
    initialLoop,
  });

  const rightPanelKey = useMemo(() => {
    if (selectionMode === 'cell') {
      return `${imageSrc}-cell-${selectedCells.map((c) => `${c.x}:${c.y}`).join('|')}`;
    }
    return `${imageSrc}-box-${boxes.map((b) => `${b.x}:${b.y}:${b.width}:${b.height}`).join('|')}`;
  }, [imageSrc, selectionMode, selectedCells, boxes]);

  const { handleCanvasClick, handleMouseMove, handleBoxChange, handleAddBox, handleRemoveBox } =
    useCanvasHandlers({
      selectionMode,
      cellOffsetX,
      cellOffsetY,
      gridSize,
      currentBox,
      dispatch,
    });

  const selectedFrameCount = selectionMode === 'cell' ? selectedCells.length : boxes.length;

  const normalizedFrames = useNormalizedFrames({
    imageSize,
    imageSrc,
    selectionMode,
    selectedCells,
    boxes,
    gridSize,
    cellOffsetX,
    cellOffsetY,
  });

  useEffect(() => {
    if (selectedPreviewFrameIndexRef.current < normalizedFrames.length) return;
    selectedPreviewFrameIndexRef.current = Math.max(0, normalizedFrames.length - 1);
  }, [normalizedFrames.length]);

  useEffect(() => {
    if (initialPivotsLoadedRef.current) return;
    if (!initialFrames || initialFrames.length === 0) return;

    const pivots: Record<number, { x: number; y: number }> = {};
    for (let i = 0; i < initialFrames.length; i += 1) {
      const frame = initialFrames[i];
      if (
        typeof frame.pivot_x !== 'number'
        || typeof frame.pivot_y !== 'number'
        || frame.width <= 0
        || frame.height <= 0
      ) {
        continue;
      }

      pivots[i] = {
        x: Math.max(0, Math.min(1, frame.pivot_x / frame.width)),
        y: Math.max(0, Math.min(1, frame.pivot_y / frame.height)),
      };
    }

    setPivotByFrameIndex(pivots);
    initialPivotsLoadedRef.current = true;
  }, [initialFrames]);

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
    const framesWithPivot = normalizedFrames.map((frame, index) => {
      const pivotNormalized = pivotByFrameIndex[index] ?? defaultPivotNormalized;
      return {
      ...frame,
      pivot_x: Math.max(0, Math.min(frame.width, Math.round(frame.width * pivotNormalized.x))),
      pivot_y: Math.max(0, Math.min(frame.height, Math.round(frame.height * pivotNormalized.y))),
      };
    });

    onConfirm?.({
      animationName: cleanName,
      frames: framesWithPivot,
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
            onSelectedFrameChange={(index) => {
              selectedPreviewFrameIndexRef.current = index;
            }}
            pivotsByFrameIndex={pivotByFrameIndex}
            onPivotChange={(index, pivot) => {
              setPivotByFrameIndex((prev) => ({
                ...prev,
                [index]: pivot,
              }));
            }}
          />
        </div>
      </div>

      <SpritePreviewFooter
        validationError={validationError}
        selectedFrameCount={selectedFrameCount}
        hasImageSrc={!!imageSrc}
        onConfirm={onConfirm ? handleConfirm : undefined}
        onCancel={onCancel}
      />
    </div>
  );
}
