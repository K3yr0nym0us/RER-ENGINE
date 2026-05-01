import { useCallback } from 'react';

import { useContextEngine } from '../context/useContextEngine';

type LoadCmd = 'load_character' | 'load_scenario';

interface SpriteFrameRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface CreateEntityFromSpriteAnimationPayload {
  spritePath: string;
  animation: {
    name: string;
    frames: SpriteFrameRect[];
    fps: number;
    loop: boolean;
  };
}

interface LoadedEntityEvent {
  id: number;
  path: string;
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

    const logicalW = Math.max(1, ...payload.animation.frames.map((f) => f.width));
    const logicalH = Math.max(1, ...payload.animation.frames.map((f) => f.height));

    const animation = {
      name: payload.animation.name,
      fps: payload.animation.fps,
      loop: payload.animation.loop,
      logical_w: logicalW,
      logical_h: logicalH,
      frames: payload.animation.frames.map((f) => ({
        path: payload.spritePath,
        pivot_x: Math.round(f.width / 2),
        pivot_y: f.height,
        src_x: f.x,
        src_y: f.y,
        src_w: f.width,
        src_h: f.height,
      })),
    };

    updateEntityAnimations?.(loaded.id, [animation]);

    const first = animation.frames[0];
    if (first) {
      send({
        cmd: 'play_animation_frame',
        id: loaded.id,
        path: first.path,
        pivot_x: first.pivot_x,
        pivot_y: first.pivot_y,
        logical_w: animation.logical_w,
        logical_h: animation.logical_h,
        src_x: first.src_x,
        src_y: first.src_y,
        src_w: first.src_w,
        src_h: first.src_h,
      });
    }

    setAnimationPlaying?.(loaded.id, false);
  }, [loadCmd, send, sendAsync, setAnimationPlaying, updateEntityAnimations]);
}

export default useCreateEntityFromSpriteAnimation;