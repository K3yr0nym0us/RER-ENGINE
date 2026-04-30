import { useState } from 'react';
import { Sprite } from '../../../SpritesAccordion/SpritesAccordion';
import { useModal } from '../../../../../../../context/ModalContext';
import { useCreateCharacterFromPng } from '../../../../../../../hooks/useCreateCharacterFromPng';
import { SpritePreviewModalBody } from '../../SpritePreviewModalBody/SpritePreviewModalBody';

interface CreateCharacterModalBodyProps {
  sprites: Sprite[];
  openDialog: () => Promise<string | null>;
}

export const CreateCharacterModalBody = ({ sprites, openDialog }: CreateCharacterModalBodyProps) => {
  const { openModal, closeModal } = useModal();
  const [selectedType, setSelectedType] = useState<'image' | 'sprite'>('image');
  const [selectedSpriteId, setSelectedSpriteId] = useState<string>('');
  const createCharacterFromPng = useCreateCharacterFromPng(openDialog);

  return (
    <div>
      <div className="mb-3">
        <label className="form-label">Tipo de creación</label>
        <select className="form-select mb-2" value={selectedType} onChange={e => setSelectedType(e.target.value as any)}>
          <option value="image">Desde imagen PNG</option>
          <option value="sprite" disabled={sprites.length === 0}>Desde sprite cargado</option>
        </select>
      </div>
      {selectedType === 'sprite' && (
        <div className="mb-3">
          <label className="form-label">Sprite</label>
          <select className="form-select" value={selectedSpriteId} onChange={e => setSelectedSpriteId(e.target.value)}>
            <option value="">Selecciona un sprite</option>
            {sprites.map(s => (
              <option key={s.id} value={s.id}>{s.name}</option>
            ))}
          </select>
        </div>
      )}
      <div className="d-flex gap-2 justify-content-end mt-3">
        <button className="btn btn-secondary btn-sm" onClick={closeModal}>Cancelar</button>
        {selectedType === 'image' && (
          <button className="btn btn-primary btn-sm" onClick={createCharacterFromPng}>
            Seleccionar imagen
          </button>
        )}
        {selectedType === 'sprite' && selectedSpriteId && (
          <button className="btn btn-primary btn-sm" onClick={() => {
            const sprite = sprites.find(s => s.id === selectedSpriteId);
            if (sprite) {
              openModal({
                title: `Vista previa de ${sprite.name}`,
                body: <SpritePreviewModalBody src={sprite.src} />,
                size: 'xl',
              });
            }
          }}>Usar sprite seleccionado</button>
        )}
      </div>
    </div>
  );
};