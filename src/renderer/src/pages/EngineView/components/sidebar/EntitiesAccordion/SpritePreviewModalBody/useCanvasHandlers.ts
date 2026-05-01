import { useCallback, type Dispatch } from 'react';
import { CANVAS_SIZE, type SpritePreviewAction, type SpritePreviewState } from './spritePreviewReducer';

interface UseCanvasHandlersParams {
  selectionMode: SpritePreviewState['selectionMode'];
  cellOffsetX: number;
  cellOffsetY: number;
  gridSize: number;
  currentBox: SpritePreviewState['currentBox'];
  dispatch: Dispatch<SpritePreviewAction>;
}

export function useCanvasHandlers({
  selectionMode,
  cellOffsetX,
  cellOffsetY,
  gridSize,
  currentBox,
  dispatch,
}: UseCanvasHandlersParams) {
  const handleCanvasClick = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
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
    },
    [selectionMode, cellOffsetX, cellOffsetY, gridSize, dispatch],
  );

  const handleMouseMove = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      if (selectionMode !== 'box') return;

      const rect = e.currentTarget.getBoundingClientRect();
      const mouseX = Math.floor(e.clientX - rect.left);
      const mouseY = Math.floor(e.clientY - rect.top);

      const { width, height } = currentBox;
      const x = Math.max(0, Math.min(mouseX - width / 2, CANVAS_SIZE - width));
      const y = Math.max(0, Math.min(mouseY - height / 2, CANVAS_SIZE - height));

      dispatch({ type: 'patch', payload: { currentBox: { ...currentBox, x, y } } });
    },
    [selectionMode, currentBox, dispatch],
  );

  const handleBoxChange = useCallback(
    (box: { x: number; y: number; width: number; height: number }) => {
      dispatch({ type: 'patch', payload: { currentBox: box } });
    },
    [dispatch],
  );

  const handleAddBox = useCallback(() => {
    dispatch({ type: 'append_current_box' });
  }, [dispatch]);

  const handleRemoveBox = useCallback(
    (index: number) => {
      dispatch({ type: 'remove_box', payload: index });
    },
    [dispatch],
  );

  return { handleCanvasClick, handleMouseMove, handleBoxChange, handleAddBox, handleRemoveBox };
}
