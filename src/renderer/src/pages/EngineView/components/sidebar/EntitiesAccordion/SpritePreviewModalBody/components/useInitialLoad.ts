import { useEffect, useRef, type Dispatch } from 'react';
import { CANVAS_SIZE, type SpritePreviewAction, type SpriteFrameRect } from './spritePreviewReducer';

interface UseInitialLoadParams {
  dispatch: Dispatch<SpritePreviewAction>;
  imageSize: { width: number; height: number } | null;
  imageSrc: string;
  initialAnimationName?: string;
  initialFrames?: SpriteFrameRect[];
  initialFps?: number;
  initialLoop?: boolean;
}

export function useInitialLoad({
  dispatch,
  imageSize,
  imageSrc,
  initialAnimationName,
  initialFrames,
  initialFps,
  initialLoop,
}: UseInitialLoadParams) {
  const initialLoadedRef = useRef(false);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'z') {
        e.preventDefault();
        dispatch({ type: 'pop_box' });
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [dispatch]);

  useEffect(() => {
    if (!initialAnimationName) return;
    dispatch({ type: 'patch', payload: { animationName: initialAnimationName } });
  }, [initialAnimationName, dispatch]);

  useEffect(() => {
    if (typeof initialFps === 'number') {
      dispatch({ type: 'patch', payload: { fps: Math.max(1, Math.min(60, initialFps)) } });
    }
  }, [initialFps, dispatch]);

  useEffect(() => {
    if (typeof initialLoop === 'boolean') {
      dispatch({ type: 'patch', payload: { isLooping: initialLoop } });
    }
  }, [initialLoop, dispatch]);

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

    dispatch({ type: 'patch', payload: { selectionMode: 'box', boxes: initialBoxes } });
    initialLoadedRef.current = true;
  }, [imageSize, imageSrc, initialFrames, dispatch]);
}
