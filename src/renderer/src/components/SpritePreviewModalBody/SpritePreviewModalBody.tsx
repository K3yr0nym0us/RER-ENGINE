import { useMemo, useReducer, useState, useEffect, useRef } from 'react';
import { Modal } from 'react-bootstrap';

import { 
  SpritePreviewLeftPanel, 
  SpritePreviewCanvas,
  SpritePreviewRightPanel,
  SpritePreviewFooter,
  useCanvasHandlers,
  useInitialLoad,
  useNormalizedFrames,
  CANVAS_SIZE,
  initialSpritePreviewState,
  spritePreviewReducer,
  type SpriteFrameRect,
  type ScriptEntry,
  type SelectionMode,
} from './components';
import { ScriptEditorModalBody } from './components/';

import { useContextEngine } from '@engine';
import { useSpritePreviewImage } from '@hooks';

interface SpritePreviewConfirmConfig {
  animationName: string;
  frames: SpriteFrameRect[];
  fps: number;
  loop: boolean;
  defaultAnimation: boolean;
  facingRight: boolean;
  audioPath?: string;
  scripts: ScriptEntry[];
  isCancelable: boolean;
  selectionMode: SelectionMode;
  gridSize: number;
  cellOffsetX: number;
  cellOffsetY: number;
}

export function SpritePreviewModalBody({
  src,
  onConfirm,
  onCancel,
  initialAnimationName,
  initialFrames,
  initialFps,
  initialLoop,
  initialIsDefaultAnimation,
  initialFacingRight,
  initialAudioPath,
  initialScripts,
  initialIsCancelable,
  initialSelectionMode,
  initialGridSize,
  initialCellOffsetX,
  initialCellOffsetY,
}: {
  src: string
  onConfirm?: (config: SpritePreviewConfirmConfig) => void
  onCancel?: () => void
  initialAnimationName?: string
  initialFrames?: SpriteFrameRect[]
  initialFps?: number
  initialLoop?: boolean
  initialIsDefaultAnimation?: boolean
  initialFacingRight?: boolean
  initialAudioPath?: string
  initialScripts?: ScriptEntry[]
  initialIsCancelable?: boolean
  initialSelectionMode?: SelectionMode
  initialGridSize?: number
  initialCellOffsetX?: number
  initialCellOffsetY?: number
}) {
  const { sounds } = useContextEngine();
  const [scriptEditorState, setScriptEditorState] = useState<
    { mode: 'add' } | { mode: 'edit'; name: string } | null
  >(null);
  const closeScriptEditor = () => setScriptEditorState(null);
  const [confirmRemoveScript, setConfirmRemoveScript] = useState<string | null>(null);
  const { imageSrc, imageSize } = useSpritePreviewImage(src);
  const [state, dispatch] = useReducer(spritePreviewReducer, initialSpritePreviewState);
  const [facingRight, setFacingRight] = useState(true);
  const [isDefaultAnimation, setIsDefaultAnimation] = useState(false);
  const [defaultPivotNormalized] = useState<{ x: number; y: number }>({ x: 0.5, y: 1.0 });
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
    audioPath,
    scripts,
    isCancelable,
  } = state;

  useEffect(() => {
    if (initialAudioPath) {
      dispatch({ type: 'patch', payload: { audioPath: initialAudioPath } });
    }
  }, [initialAudioPath]);

  useEffect(() => {
    if (initialScripts && initialScripts.length > 0) {
      dispatch({ type: 'patch', payload: { scripts: initialScripts } });
    }
  }, [initialScripts]);

  useEffect(() => {
    dispatch({ type: 'patch', payload: { isCancelable: initialIsCancelable ?? false } });
  }, [initialIsCancelable]);

  useEffect(() => {
    setFacingRight(initialFacingRight ?? true);
  }, [initialFacingRight]);

  useEffect(() => {
    setIsDefaultAnimation(initialIsDefaultAnimation ?? false);
  }, [initialIsDefaultAnimation]);

  const handleAudioChange = (path: string) => {
    dispatch({ type: 'patch', payload: { audioPath: path || undefined } });
  };

  const handleAddScript = () => {
    setScriptEditorState({ mode: 'add' });
  };

  const handleEditScript = (name: string) => {
    setScriptEditorState({ mode: 'edit', name });
  };

  const handleRemoveScript = (name: string) => {
    setConfirmRemoveScript(name);
  };

  useInitialLoad({
    dispatch,
    imageSize,
    imageSrc,
    initialAnimationName,
    initialFrames,
    initialFps,
    initialLoop,
    initialSelectionMode,
    initialGridSize,
    initialCellOffsetX,
    initialCellOffsetY,
  });

  const rightPanelKey = useMemo(() => {
    if (selectionMode === 'cell') {
      return `${imageSrc}-cell-${selectedCells.map((c) => `${c.x}:${c.y}`).join('|')}`;
    }
    return `${imageSrc}-box-${boxes.map((b) => `${b.x}:${b.y}:${b.width}:${b.height}`).join('|')}`;
  }, [imageSrc, selectionMode, selectedCells, boxes]);

  const { handleCanvasClick, handleMouseMove, handleBoxChange, handleRemoveBox } =
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

    const drawW = Math.max(1, ...initialFrames.map((f) => f.width));
    const drawH = Math.max(1, ...initialFrames.map((f) => f.height));
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
        x: Math.max(0, Math.min(1, frame.pivot_x / drawW)),
        y: Math.max(0, Math.min(1, frame.pivot_y / drawH)),
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
    const drawW = Math.max(1, ...normalizedFrames.map((f) => f.width));
    const drawH = Math.max(1, ...normalizedFrames.map((f) => f.height));
    const framesWithPivot = normalizedFrames.map((frame, index) => {
      const pivotNormalized = pivotByFrameIndex[index] ?? defaultPivotNormalized;
      return {
      ...frame,
      pivot_x: Math.max(0, Math.min(drawW, Math.round(drawW * pivotNormalized.x))),
      pivot_y: Math.max(0, Math.min(drawH, Math.round(drawH * pivotNormalized.y))),
      };
    });

    onConfirm?.({
      animationName: cleanName,
      frames: framesWithPivot,
      fps,
      loop: isLooping,
      defaultAnimation: isDefaultAnimation,
      facingRight,
      audioPath,
      scripts,
      isCancelable,
      selectionMode,
      gridSize,
      cellOffsetX,
      cellOffsetY,
    });
  };

  return (
    <div>
      <div data-bs-theme="dark" className="row g-3 p-3 rounded-3 pt-0" style={{ minHeight: 300 }}>
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
            sounds={sounds}
            audioPath={audioPath}
            onAudioChange={handleAudioChange}
            scripts={scripts}
            onAddScript={handleAddScript}
            onEditScript={handleEditScript}
            onRemoveScript={handleRemoveScript}
            isCancelable={isCancelable}
            onIsCancelableChange={(value) => dispatch({ type: 'patch', payload: { isCancelable: value } })}
          />
        </div>

        <div className="col">
          <SpritePreviewCanvas
            src={imageSrc}
            facingRight={facingRight}
            onFacingRightChange={setFacingRight}
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
            isDefaultAnimation={isDefaultAnimation}
            onDefaultAnimationChange={setIsDefaultAnimation}
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
            audioPath={audioPath}
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

      {/* Modal de confirmación de eliminación de script */}
      <Modal
        show={confirmRemoveScript !== null}
        onHide={() => setConfirmRemoveScript(null)}
        size="sm"
        centered
      >
        <Modal.Header closeButton>
          <Modal.Title>Eliminar script</Modal.Title>
        </Modal.Header>
        <Modal.Body>
          <p className="mb-0">
            ¿Seguro que quieres eliminar el script{' '}
            <strong>{confirmRemoveScript}</strong>? Esta acción no se puede deshacer.
          </p>
        </Modal.Body>
        <Modal.Footer>
          <button
            className="btn btn-sm btn-outline-secondary"
            onClick={() => setConfirmRemoveScript(null)}
          >
            Cancelar
          </button>
          <button
            className="btn btn-sm btn-danger"
            onClick={() => {
              if (confirmRemoveScript) {
                dispatch({ type: 'patch', payload: { scripts: scripts.filter((s) => s.name !== confirmRemoveScript) } });
              }
              setConfirmRemoveScript(null);
            }}
          >
            Eliminar
          </button>
        </Modal.Footer>
      </Modal>

      {/* Modal local para el editor de scripts — evita reemplazar el modal padre */}
      <Modal
        show={scriptEditorState !== null}
        onHide={closeScriptEditor}
        size="lg"
        centered
      >
        <Modal.Header closeButton>
          <Modal.Title>
            {scriptEditorState?.mode === 'edit'
              ? `Editar Script: ${(scriptEditorState as { mode: 'edit'; name: string }).name}`
              : 'Nuevo Script Lua'}
          </Modal.Title>
        </Modal.Header>
        <Modal.Body>
          {scriptEditorState !== null && (
            <ScriptEditorModalBody
              initialData={
                scriptEditorState.mode === 'edit'
                  ? scripts.find((s) => s.name === (scriptEditorState as { mode: 'edit'; name: string }).name)
                  : undefined
              }
              onSave={(data) => {
                if (scriptEditorState.mode === 'add') {
                  dispatch({ type: 'patch', payload: { scripts: [...scripts, data] } });
                } else {
                  const editName = (scriptEditorState as { mode: 'edit'; name: string }).name;
                  dispatch({ type: 'patch', payload: { scripts: scripts.map((s) => s.name === editName ? data : s) } });
                }
                closeScriptEditor();
              }}
              onCancel={closeScriptEditor}
            />
          )}
        </Modal.Body>
      </Modal>
    </div>
  );
}
