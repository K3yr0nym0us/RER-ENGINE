import { useState } from 'react';

import { SpritePreviewModalBody, type SpriteFrameRect } from '@components';

import type { SpriteInfo } from '@shared-types';
import { useTraslate } from '@hooks';
import { useModalClose } from '../../../../../../modal-electron/useModalClose';

interface CreateEntityFromSpriteModalBodyProps {
  sprites?: SpriteInfo[];
  parentHandlerId?: string;
  previewTitle: string;
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
      defaultAnimation?: boolean;
      selectionMode?: string;
      gridSize?: number;
      cellOffsetX?: number;
      cellOffsetY?: number;
    };
  }) => void;
}

export function CreateEntityFromSpriteModalBody({
  sprites = [],
  parentHandlerId,
  onCreateEntity,
  previewTitle,
}: CreateEntityFromSpriteModalBodyProps) {
  const { t } = useTraslate();
  const closeModal = useModalClose();
  const [selectedSpritePath, setSelectedSpritePath] = useState<string>('');

  const spriteName = (path: string) => path.split('/').pop() ?? path;

  const handleOpenPreview = () => {
    if (!selectedSpritePath || !parentHandlerId) return;

    closeModal();
    window.setTimeout(() => {
      window.electronAPI.requestParentModalOpen({
        parentHandlerId,
        action: 'openSpritePreview',
        payload: {
          spritePath: selectedSpritePath,
          previewTitle,
        },
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
        <button className="btn btn-secondary btn-sm" onClick={closeModal}>{t('Cancel')}</button>
        {selectedSpritePath && (
          <button className="btn btn-primary btn-sm" onClick={handleOpenPreview}>
            {t('Configure frames')}
          </button>
        )}
      </div>
    </div>
  );
}
