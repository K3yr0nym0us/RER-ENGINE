import { useEffect, useState } from 'react';

import { Accordion } from 'react-bootstrap';
import { Files, Pencil, PlayFill, StopFill, Trash } from 'react-bootstrap-icons';

import AppTooltip from '../../../../../components/AppTooltip';
import { CreateEntityFromSpriteModalBody } from '../EntitiesAccordion/components/CreateEntityFromSpriteModalBody';
import { SpritePreviewModalBody } from '../EntitiesAccordion/SpritePreviewModalBody/SpritePreviewModalBody';
import type { SpriteFrameRect } from '../EntitiesAccordion/SpritePreviewModalBody/components';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';

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
  facing_right?: boolean;
  logical_w: number;
  logical_h: number;
  audio_path?: string;
  scripts?: { name: string; source: string }[];
  frames: AnimationFrame[];
}

let animationIdCounter = 0;

const createAnimationId = () => {
  animationIdCounter += 1;
  return `anim_${animationIdCounter}`;
};

export function AnimationsPanel() {
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
      title: 'Nueva animacion',
      body: (
        <CreateEntityFromSpriteModalBody
          sprites={sprites}
          previewTitle="Configurar animacion"
          onCreateEntity={({ spritePath, animation }) => {
            const logicalW = Math.max(1, ...animation.frames.map((f) => f.width));
            const logicalH = Math.max(1, ...animation.frames.map((f) => f.height));
            const baseLogicalW = animations[0]?.logical_w ?? logicalW;
            const baseLogicalH = animations[0]?.logical_h ?? logicalH;

            const newAnimation: Animation = {
              id: createAnimationId(),
              name: animation.name,
              fps: animation.fps,
              loop: animation.loop,
              facing_right: animation.facingRight,
              logical_w: baseLogicalW,
              logical_h: baseLogicalH,
              audio_path: animation.audioPath,
              scripts: animation.scripts,
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

            const normalizedExisting = animations.map((a) => ({
              ...a,
              logical_w: baseLogicalW,
              logical_h: baseLogicalH,
            }));

            const next = [...normalizedExisting, newAnimation];
            syncAnimations(next);
            applyFirstFrame(newAnimation);
          }}
        />
      ),
    });
  };

  const removeAnimation = (index: number) => {
    if (animationPlaying.get(entity?.id ?? 0)) {
      send({ cmd: 'stop_animation', id: entity?.id });
    }
    const next = animations.filter((_, i) => i !== index);
    syncAnimations(next);
  };

  const duplicateAnimation = (index: number) => {
    const anim = animations[index];
    if (!anim) return;

    const duplicated: Animation = {
      ...anim,
      id: createAnimationId(),
      name: `${anim.name} copia`,
      frames: anim.frames.map((frame) => ({ ...frame })),
      scripts: anim.scripts?.map((script) => ({ ...script })),
    };

    const next = [...animations, duplicated];
    syncAnimations(next);
  };

  const confirmRemoveAnimation = (index: number) => {
    const anim = animations[index];
    if (!anim) return;

    openModal({
      title: 'Confirmar eliminacion',
      body: (
        <div>
          <p className="mb-3">¿Seguro que deseas eliminar la animacion <strong>{anim.name}</strong>?</p>
          <div className="d-flex justify-content-end gap-2">
            <button className="btn btn-secondary btn-sm" onClick={closeModal}>Cancelar</button>
            <button
              className="btn btn-danger btn-sm"
              onClick={() => {
                removeAnimation(index);
                closeModal();
              }}
            >
              Eliminar
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
      title: `Editar animacion: ${anim.name}`,
      size: 'xl',
      body: (
        <SpritePreviewModalBody
          src={spritePath}
          initialAnimationName={anim.name}
          initialFrames={initialFrames}
          initialFps={anim.fps}
          initialLoop={anim.loop}
          initialFacingRight={anim.facing_right ?? true}
          initialAudioPath={anim.audio_path}
          initialScripts={anim.scripts}
          onConfirm={(config) => {
            const logicalW = Math.max(1, ...config.frames.map((f) => f.width));
            const logicalH = Math.max(1, ...config.frames.map((f) => f.height));

            const updatedAnimation: Animation = {
              ...anim,
              name: config.animationName,
              fps: config.fps,
              loop: config.loop,
              facing_right: config.facingRight,
              logical_w: logicalW,
              logical_h: logicalH,
              audio_path: config.audioPath,
              scripts: config.scripts,
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

            const next = animations.map((a, i) => (i === index ? updatedAnimation : a));
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
      <Accordion.Header>Animaciones</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <button
          className="btn btn-outline-success btn-sm w-100 fw-bold mb-2"
          onClick={openCreateAnimationModal}
          disabled={!entity?.id}
        >
          + Nueva animacion
        </button>

        {animations.length === 0 && (
          <div className="alert alert-secondary py-1 text-center" role="alert">
            Sin animaciones. Agrega una nueva para empezar.
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

                  <AppTooltip content={isPlayingThisAnimation ? 'Detener animacion' : 'Reproducir animacion'} place="top">
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

                  <AppTooltip content="Editar animacion" place="top">
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

                  <AppTooltip content="Duplicar animacion" place="top">
                    <span
                      role="button"
                      tabIndex={0}
                      className="text-info"
                      style={{ cursor: 'pointer' }}
                      onClick={() => duplicateAnimation(idx)}
                      onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') duplicateAnimation(idx); }}
                    >
                      <Files />
                    </span>
                  </AppTooltip>

                  <AppTooltip content="Eliminar animacion" place="top">
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
