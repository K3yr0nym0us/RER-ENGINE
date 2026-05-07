import { useState } from 'react';

import { useModal } from '@modal';
import { SpritePreviewModalBody, type SpriteFrameRect } from '../SpritePreviewModalBody/SpritePreviewModalBody';

import type { SpriteInfo } from '@shared-types';
import { useTraslate } from '@hooks';

interface CreateEntityFromSpriteModalBodyProps {
  sprites: SpriteInfo[];
  onCreateEntity: (payload: {
    spritePath: string;
    animation: {
      name: string;
      frames: SpriteFrameRect[];
      fps: number;
      loop: boolean;
      facingRight: boolean;
      audioPath?: string;
      scripts: { name: string; source: string }[];
      isCancelable: boolean;
      selectionMode?: string;
      gridSize?: number;
      cellOffsetX?: number;
      cellOffsetY?: number;
    };
  }) => void;
  previewTitle: string;
}

export function CreateEntityFromSpriteModalBody({
  sprites,
  onCreateEntity,
  previewTitle,
}: CreateEntityFromSpriteModalBodyProps) {
  const { t } = useTraslate();
  const { closeModal, openModal } = useModal();
  const [selectedSpritePath, setSelectedSpritePath] = useState<string>('');

  const spriteName = (path: string) => path.split('/').pop() ?? path;

  const handleOpenPreview = () => {
    if (!selectedSpritePath) return;

    const spritePath = selectedSpritePath;

    closeModal();
    window.setTimeout(() => {
      openModal({
        title: previewTitle,
        size: 'xl',
        body: (
          <SpritePreviewModalBody
            src={spritePath}
            onConfirm={(config) => {
              onCreateEntity({
                spritePath,
                animation: {
                  name: config.animationName,
                  frames: config.frames,
                  fps: config.fps,
                  loop: config.loop,
                  facingRight: config.facingRight,
                  audioPath: config.audioPath,
                  scripts: config.scripts,
                  isCancelable: config.isCancelable,
                  selectionMode: config.selectionMode,
                  gridSize: config.gridSize,
                  cellOffsetX: config.cellOffsetX,
                  cellOffsetY: config.cellOffsetY,
                },
              });
              closeModal();
            }}
            onCancel={closeModal}
          />
        ),
      });
    }, 0);
  };

  if (sprites.length === 0) {
    return (
      <div className="alert alert-warning mb-0">
        <p className="mb-2">{t('No preloaded sprites. Load sprites first in the Sprites accordion.')}</p>
        <button className="btn btn-secondary btn-sm" onClick={closeModal}>{t('Close')}</button>
      </div>
    );
  }

  return (
    <div>
      <div className="mb-3">
        <label className="form-label" htmlFor="create-entity-sprite-select">{t('Select a sprite')}</label>
        <select
          id="create-entity-sprite-select"
          className="form-select"
          value={selectedSpritePath}
          onChange={e => setSelectedSpritePath(e.target.value)}
        >
          <option value="">{t('-- Choose a sprite --')}</option>
          {sprites.map(s => (
            <option key={s.path} value={s.path}>
              {s.name || spriteName(s.path)} ({s.width}x{s.height})
            </option>
          ))}
        </select>
      </div>

      <div className="d-flex gap-2 justify-content-end mt-3">
        <button className="btn btn-secondary btn-sm" onClick={closeModal}>Cancelar</button>
        {selectedSpritePath && (
          <button className="btn btn-primary btn-sm" onClick={handleOpenPreview}>
            Configurar frames
          </button>
        )}
      </div>
    </div>
  );
}

export default CreateEntityFromSpriteModalBody;