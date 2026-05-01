import { useMemo, useReducer } from 'react';

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
    useCanvasHandlers({ selectionMode, cellOffsetX, cellOffsetY, gridSize, currentBox, dispatch });

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
