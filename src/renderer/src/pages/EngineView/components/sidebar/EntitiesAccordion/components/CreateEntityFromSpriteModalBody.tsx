import { useState } from 'react';

import { useModal } from '../../../../../../context/ModalContext';
import { SpritePreviewModalBody, type SpriteFrameRect } from '../SpritePreviewModalBody/SpritePreviewModalBody';

import type { SpriteInfo } from '../../../../../../../../shared-types/types';

interface CreateEntityFromSpriteModalBodyProps {
  sprites: SpriteInfo[];
  onCreateEntity: (payload: {
    spritePath: string;
    animation: {
      name: string;
      frames: SpriteFrameRect[];
      fps: number;
      loop: boolean;
    };
  }) => void;
  previewTitle: string;
}

export function CreateEntityFromSpriteModalBody({
  sprites,
  onCreateEntity,
  previewTitle,
}: CreateEntityFromSpriteModalBodyProps) {
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
        <p className="mb-2">No hay sprites precargados. Carga sprites primero en el acordeon de <strong>Sprites</strong>.</p>
        <button className="btn btn-secondary btn-sm" onClick={closeModal}>Cerrar</button>
      </div>
    );
  }

  return (
    <div>
      <div className="mb-3">
        <label className="form-label">Selecciona un sprite</label>
        <select
          className="form-select"
          value={selectedSpritePath}
          onChange={e => setSelectedSpritePath(e.target.value)}
        >
          <option value="">-- Elige un sprite --</option>
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