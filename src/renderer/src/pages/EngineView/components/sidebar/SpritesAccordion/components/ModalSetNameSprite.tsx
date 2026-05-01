import { useRef } from 'react';
import { useModal } from '@modal';
import { useContextEngine } from '@engine';

interface ModalSetNameSpriteProps {
  path: string;
  autoName: string;
}

export default function ModalSetNameSprite({ path, autoName }: ModalSetNameSpriteProps) {
  const { loadSprite } = useContextEngine();
  const { closeModal } = useModal();
  
  const nameRef = useRef<HTMLInputElement>(null);

  const handleConfirm = (name: string) => {
    loadSprite(path, name);
    closeModal();
  }

  return (
    <div>
      <p className="text-muted small">Archivo: {path.split('/').pop()}</p>
      <input
        className="form-control mb-2"
        type="text"
        defaultValue={autoName}
        ref={nameRef}
        placeholder="Nombre del sprite"
      />
      <div className="d-flex gap-2 justify-content-end">
        <button 
          className="btn btn-secondary btn-sm" 
          onClick={closeModal}
        >
          Cancelar
        </button>
        <button
          className="btn btn-primary btn-sm"
          onClick={() => {
            const name = nameRef.current?.value || autoName;
            handleConfirm(name);
          }}
        >
          Cargar
        </button>
      </div>
    </div>
  );
}
