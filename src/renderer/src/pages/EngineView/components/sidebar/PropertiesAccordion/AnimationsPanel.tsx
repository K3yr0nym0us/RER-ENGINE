import { useEffect, useState } from 'react';

import { Accordion } from 'react-bootstrap';
import { Pencil, PlayFill, StopFill, Trash } from 'react-bootstrap-icons';

import { AppTooltip, SpritePreviewModalBody } from '@components';
import { CreateEntityFromSpriteModalBody } from '../EntitiesAccordion/components/CreateEntityFromSpriteModalBody';
import type { SpriteFrameRect } from '@components';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';

interface AnimationFrame {
  path: string;
  pivot_x: number;
  pivot_y: number;
  src_x?: number;
  src_y?: number;
  src_w?: number;
  src_h?: number;
}

interface Animation {
  id?: string;
  name: string;
  fps: number;
  loop: boolean;
  is_default?: boolean;
  is_cancelable?: boolean;
  facing_right?: boolean;
  logical_w: number;
  logical_h: number;
  audio_path?: string;
  scripts?: { name: string; source: string }[];
  frames: AnimationFrame[];
  selection_mode?: 'cell' | 'box';
  grid_size?: number;
  cell_offset_x?: number;
  cell_offset_y?: number;
}

let animationIdCounter = 0;

const createAnimationId = () => {
  animationIdCounter += 1;
  return `anim_${animationIdCounter}`;
};

const getLogicalBaseFromFrames = (frames: Array<{ width: number; height: number }>) => ({
  logical_w: Math.max(1, ...frames.map((f) => Math.max(1, f.width))),
  logical_h: Math.max(1, ...frames.map((f) => Math.max(1, f.height))),
});

export function AnimationsPanel() {
  const { t } = useTraslate();
  const { selectedEntity: entity, send, sendAsync, setAnimationPlaying, updateEntityAnimations, animationPlaying, sprites } = useContextEngine();
  const { openModal, closeModal } = useModal();

  const [animations, setAnimations] = useState<Animation[]>([]);
  const [playingAnimationName, setPlayingAnimationName] = useState<string | null>(null);

  useEffect(() => {
    setAnimations(entity?.animations ?? []);
    setPlayingAnimationName(null);
  }, [entity?.id]);

  useEffect(() => {
    if (!entity?.id) return;
    const isEntityPlaying = animationPlaying.get(entity.id) ?? false;
    if (!isEntityPlaying) {
      setPlayingAnimationName(null);
    }
  }, [entity?.id, animationPlaying]);

  const syncAnimations = (next: Animation[]) => {
    setAnimations(next);
    if (entity?.id) updateEntityAnimations?.(entity.id, next);
  };

  const applyFirstFrame = (anim: Animation) => {
    if (!entity?.id) return;
    const first = anim.frames[0];
    if (!first) return;

    send({
      cmd: 'play_animation_frame',
      id: entity.id,
      path: first.path,
      pivot_x: first.pivot_x,
      pivot_y: first.pivot_y,
      logical_w: anim.logical_w ?? 64,
      logical_h: anim.logical_h ?? 64,
      src_x: first.src_x,
      src_y: first.src_y,
      src_w: first.src_w,
      src_h: first.src_h,
    });
  };

  const openCreateAnimationModal = () => {
    if (!entity?.id) return;

    openModal({
      title: t('New animation'),
      body: (
        <CreateEntityFromSpriteModalBody
          sprites={sprites}
          previewTitle={t('Configure animation')}
          onCreateEntity={({ spritePath, animation }) => {
            const { logical_w, logical_h } = getLogicalBaseFromFrames(animation.frames);

            const newAnimation: Animation = {
              id: createAnimationId(),
              name: animation.name,
              fps: animation.fps,
              loop: animation.loop,
              is_default: !animations.some((a) => a.is_default),
              is_cancelable: animation.isCancelable,
              facing_right: animation.facingRight,
              logical_w,
              logical_h,
              audio_path: animation.audioPath,
              scripts: animation.scripts,
              selection_mode: animation.selectionMode as 'cell' | 'box' | undefined,
              grid_size: animation.gridSize,
              cell_offset_x: animation.cellOffsetX,
              cell_offset_y: animation.cellOffsetY,
              frames: animation.frames.map((f) => ({
                path: spritePath,
                pivot_x: f.pivot_x ?? Math.round(f.width / 2),
                pivot_y: f.pivot_y ?? f.height,
                src_x: f.x,
                src_y: f.y,
                src_w: f.width,
                src_h: f.height,
              })),
            };

            const next = [...animations, newAnimation];
            syncAnimations(next);
            applyFirstFrame(newAnimation);
          }}
        />
      ),
    });
  };

  const removeAnimation = (index: number) => {
    if (!entity?.id) return;
    const anim = animations[index];
    if (!anim) return;

    if (animationPlaying.get(entity.id)) {
      send({ cmd: 'stop_animation', id: entity.id });
    }

    // Notificar al motor para eliminar la animación
    send({ cmd: 'remove_animation', id: entity.id, name: anim.name });

    const next = animations.filter((_, i) => i !== index);
    syncAnimations(next);
  };

  const confirmRemoveAnimation = (index: number) => {
    const anim = animations[index];
    if (!anim) return;

    openModal({
      title: t('Confirm deletion'),
      body: (
        <div>
          <p className="mb-3">{t('Are you sure you want to delete the animation')} <strong>{anim.name}</strong>?</p>
          <div className="d-flex justify-content-end gap-2">
            <button className="btn btn-secondary btn-sm" onClick={closeModal}>{t('Cancel')}</button>
            <button
              className="btn btn-danger btn-sm"
              onClick={() => {
                removeAnimation(index);
                closeModal();
              }}
            >
              {t('Delete')}
            </button>
          </div>
        </div>
      ),
    });
  };

  const playAnimation = async (index: number) => {
    if (!entity) return;
    const anim = animations[index];
    if (!anim || anim.frames.length === 0) return;

    const isPlayingThisAnimation = (animationPlaying.get(entity.id) ?? false) && playingAnimationName === anim.name;
    if (isPlayingThisAnimation) {
      send({ cmd: 'stop_animation', id: entity.id });
      setAnimationPlaying(entity.id, false);
      setPlayingAnimationName(null);
      return;
    }

    if (animationPlaying.get(entity.id) ?? false) {
      send({ cmd: 'stop_animation', id: entity.id });
    }

    if (anim.loop) {
      send({ cmd: 'play_animation', id: entity.id, name: anim.name });
      setAnimationPlaying(entity.id, true);
      setPlayingAnimationName(anim.name);
    } else {
      await sendAsync(
        { cmd: 'play_animation', id: entity.id, name: anim.name },
        'animation_finished',
        () => {
          setAnimationPlaying(entity.id, true);
          setPlayingAnimationName(anim.name);
        },
      );
      setPlayingAnimationName(null);
    }
  };

  const editAnimation = (index: number) => {
    const anim = animations[index];
    if (!anim) return;

    const spritePath = anim.frames[0]?.path;
    if (!spritePath) return;

    const initialFrames: SpriteFrameRect[] = anim.frames.map((f) => ({
      x: f.src_x ?? 0,
      y: f.src_y ?? 0,
      width: f.src_w ?? anim.logical_w ?? 64,
      height: f.src_h ?? anim.logical_h ?? 64,
      pivot_x: f.pivot_x,
      pivot_y: f.pivot_y,
    }));

    openModal({
      title: `${t('Edit animation:')} ${anim.name}`,
      size: 'xl',
      body: (
        <SpritePreviewModalBody
          src={spritePath}
          initialAnimationName={anim.name}
          initialFrames={initialFrames}
          initialFps={anim.fps}
          initialLoop={anim.loop}
          initialIsDefaultAnimation={anim.is_default ?? false}
          initialIsCancelable={anim.is_cancelable ?? false}
          initialFacingRight={anim.facing_right ?? true}
          initialAudioPath={anim.audio_path}
          initialScripts={anim.scripts}
          initialSelectionMode={anim.selection_mode}
          initialGridSize={anim.grid_size}
          initialCellOffsetX={anim.cell_offset_x}
          initialCellOffsetY={anim.cell_offset_y}
          onConfirm={(config) => {
            const { logical_w, logical_h } = getLogicalBaseFromFrames(config.frames);

            const updatedAnimation: Animation = {
              ...anim,
              name: config.animationName,
              fps: config.fps,
              loop: config.loop,
              is_default: config.defaultAnimation,
              is_cancelable: config.isCancelable,
              facing_right: config.facingRight,
              logical_w,
              logical_h,
              audio_path: config.audioPath,
              scripts: config.scripts,
              selection_mode: config.selectionMode,
              grid_size: config.gridSize,
              cell_offset_x: config.cellOffsetX,
              cell_offset_y: config.cellOffsetY,
              frames: config.frames.map((f) => ({
                path: spritePath,
                pivot_x: f.pivot_x ?? Math.round(f.width / 2),
                pivot_y: f.pivot_y ?? f.height,
                src_x: f.x,
                src_y: f.y,
                src_w: f.width,
                src_h: f.height,
              })),
            };

            const next = animations.map((a, i) => {
              if (i === index) return updatedAnimation;
              if (config.defaultAnimation) return { ...a, is_default: false };
              return a;
            });
            syncAnimations(next);
            applyFirstFrame(updatedAnimation);
            closeModal();
          }}
          onCancel={closeModal}
        />
      ),
    });
  };

  return (
    <Accordion.Item eventKey="animaciones">
      <Accordion.Header>{t('Animations')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <button
          className="btn btn-outline-success btn-sm w-100 fw-bold mb-2"
          onClick={openCreateAnimationModal}
          disabled={!entity?.id}
        >
          {t('+ New animation')}
        </button>

        {animations.length === 0 && (
          <div className="alert alert-secondary py-1 text-center" role="alert">
            {t('No animations. Add a new one to start.')}
          </div>
        )}

        {animations.length > 0 && (
          <div className="d-flex flex-column gap-2">
            {animations.map((anim, idx) => {
              const canPlayOrEdit = anim.frames.length > 0;
              const isPlayingThisAnimation = !!entity?.id && (animationPlaying.get(entity.id) ?? false) && playingAnimationName === anim.name;
              return (
                <div key={anim.id ?? `${anim.name}-${idx}`} className="d-flex align-items-center gap-2 p-2 pt-1 pb-1 border border-secondary rounded bg-dark">
                  <AppTooltip content={anim.name} place="top">
                    <span className="small fw-semibold text-light flex-fill text-truncate">{anim.name}</span>
                  </AppTooltip>

                  <AppTooltip content={isPlayingThisAnimation ? t('Stop animation') : t('Play animation')} place="top">
                    <span
                      role="button"
                      tabIndex={canPlayOrEdit ? 0 : -1}
                      aria-disabled={!canPlayOrEdit}
                      className={isPlayingThisAnimation ? 'text-danger' : 'text-success'}
                      style={{ cursor: canPlayOrEdit ? 'pointer' : 'not-allowed', opacity: canPlayOrEdit ? 1 : 0.5 }}
                      onClick={canPlayOrEdit ? () => playAnimation(idx) : undefined}
                      onKeyDown={canPlayOrEdit ? (e) => { if (e.key === 'Enter' || e.key === ' ') playAnimation(idx); } : undefined}
                    >
                      {isPlayingThisAnimation ? <StopFill /> : <PlayFill />}
                    </span>
                  </AppTooltip>

                  <AppTooltip content={t('Edit animation')} place="top">
                    <span
                      role="button"
                      tabIndex={canPlayOrEdit ? 0 : -1}
                      aria-disabled={!canPlayOrEdit}
                      className="text-warning"
                      style={{ cursor: canPlayOrEdit ? 'pointer' : 'not-allowed', opacity: canPlayOrEdit ? 1 : 0.5 }}
                      onClick={canPlayOrEdit ? () => editAnimation(idx) : undefined}
                      onKeyDown={canPlayOrEdit ? (e) => { if (e.key === 'Enter' || e.key === ' ') editAnimation(idx); } : undefined}
                    >
                      <Pencil />
                    </span>
                  </AppTooltip>

                  <AppTooltip content={t('Delete animation')} place="top">
                    <span
                      role="button"
                      tabIndex={0}
                      className="text-danger"
                      style={{ cursor: 'pointer' }}
                      onClick={() => confirmRemoveAnimation(idx)}
                      onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') confirmRemoveAnimation(idx); }}
                    >
                      <Trash />
                    </span>
                  </AppTooltip>
                </div>
              );
            })}
          </div>
        )}
      </Accordion.Body>
    </Accordion.Item>
  );
}

export default AnimationsPanel;
