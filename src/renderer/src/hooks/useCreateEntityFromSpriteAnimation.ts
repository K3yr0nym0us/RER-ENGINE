import { useCallback } from 'react';

import { useContextEngine } from '@engine';
import { buildPlayAnimationFrameCmd } from '../context/useContextEngine/hooks/applyPendingRestoreToEngine';

type LoadCmd = 'load_character' | 'load_scenario';

interface SpriteFrameRect {
  x: number;
  y: number;
  width: number;
  height: number;
  pivot_x?: number;
  pivot_y?: number;
}

interface CreateEntityFromSpriteAnimationPayload {
  spritePath: string;
  animation: {
    name: string;
    frames: SpriteFrameRect[];
    fps: number;
    loop: boolean;
    facingRight?: boolean;
    audioPath?: string;
    scripts?: { name: string; source: string }[];
    isCancelable?: boolean;
    selectionMode?: 'cell' | 'box';
    gridSize?: number;
    cellOffsetX?: number;
    cellOffsetY?: number;
  };
}

interface LoadedEntityEvent {
  id: number;
  path: string;
  img_width: number;
  img_height: number;
  default_pivot_x: number;
  default_pivot_y: number;
}

export function useCreateEntityFromSpriteAnimation(loadCmd: LoadCmd) {
  const { send, sendAsync, updateEntityAnimations, setAnimationPlaying } = useContextEngine();

  return useCallback(async (payload: CreateEntityFromSpriteAnimationPayload) => {
    const eventName = loadCmd === 'load_character' ? 'character_loaded' : 'scenario_loaded';
    const loaded = await sendAsync<LoadedEntityEvent>(
      { cmd: loadCmd, path: payload.spritePath },
      eventName,
    );

    if (!loaded?.id) return;

    const frameLogicalW = Math.max(1, ...payload.animation.frames.map((f) => f.width));
    const frameLogicalH = Math.max(1, ...payload.animation.frames.map((f) => f.height));

    const animation = {
      name: payload.animation.name,
      fps: payload.animation.fps,
      loop: payload.animation.loop,
      is_default: true,
      is_cancelable: payload.animation.isCancelable ?? false,
      facing_right: payload.animation.facingRight ?? true,
      logical_w: frameLogicalW,
      logical_h: frameLogicalH,
      audio_path: payload.animation.audioPath,
      scripts: payload.animation.scripts ?? [],
      selection_mode: payload.animation.selectionMode,
      grid_size: payload.animation.gridSize,
      cell_offset_x: payload.animation.cellOffsetX,
      cell_offset_y: payload.animation.cellOffsetY,
      frames: payload.animation.frames.map((f) => ({
        path: payload.spritePath,
        ...(f.pivot_x != null ? { pivot_x: f.pivot_x } : {}),
        ...(f.pivot_y != null ? { pivot_y: f.pivot_y } : {}),
        src_x: f.x,
        src_y: f.y,
        src_w: f.width,
        src_h: f.height,
      })),
    };

    const resolved = updateEntityAnimations(loaded.id, [animation]);
    const synced = resolved[0] ?? animation;

    const firstFrame = synced.frames[0];
    if (firstFrame?.path) {
      send(buildPlayAnimationFrameCmd(loaded.id, synced, firstFrame));
    }

    setAnimationPlaying?.(loaded.id, false);
  }, [loadCmd, send, sendAsync, setAnimationPlaying, updateEntityAnimations]);
}

export default useCreateEntityFromSpriteAnimation;