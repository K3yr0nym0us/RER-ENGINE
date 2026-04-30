import { useRef } from 'react';

import { Image } from 'react-bootstrap-icons';

import { useModal } from '../../../../../../context/ModalContext';

interface BtnLoadSpriteProps {
  onSpriteLoaded?: (sprite: { name: string; src: string }) => void;
}

export const BtnLoadSprite = ({ onSpriteLoaded }: BtnLoadSpriteProps) => {
  const { openModal, closeModal } = useModal();
  const spriteNameRef = useRef<HTMLInputElement>(null);

  const handleLoadSprite = async () => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'image/*';
    input.onchange = () => {
      const file = input.files?.[0];
      if (file) {
        const reader = new FileReader();
        reader.onload = () => {
          const src = reader.result as string;
          let tempName = file.name.replace(/\.[^/.]+$/, '');
          openModal({
            title: 'Asignar nombre al Sprite',
            body: (
              <div>
                <div className="mb-3 text-center">
                  <img 
                    src={src} 
                    alt="preview" 
                    style={{ maxWidth: '30vw' }} 
                  />
                </div>
                <input
                  className="form-control mb-2"
                  type="text"
                  defaultValue={tempName}
                  ref={spriteNameRef}
                  placeholder="Nombre del sprite"
                  autoFocus
                />
                <div className="d-flex gap-2 justify-content-end">
                  <button className="btn btn-secondary btn-sm" onClick={closeModal}>Cancelar</button>
                  <button
                    className="btn btn-primary btn-sm"
                    onClick={() => {
                      const name = spriteNameRef.current?.value || tempName;
                      if (onSpriteLoaded) onSpriteLoaded({ name, src });
                      closeModal();
                    }}
                  >
                    Guardar
                  </button>
                </div>
              </div>
            )
          });
        };
        reader.readAsDataURL(file);
      }
    };
    input.click();
  };

  return (
    <button
      className="btn btn-outline-primary btn-sm w-100 mb-2"
      type="button"
      onClick={handleLoadSprite}
    >
      <Image className="me-1" /> Cargar Sprite
    </button>
  );
};

export default BtnLoadSprite;