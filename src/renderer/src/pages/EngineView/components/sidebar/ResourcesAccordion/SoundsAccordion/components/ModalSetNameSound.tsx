import { useRef } from 'react';
import { useContextEngine } from '@engine';
import { useModalClose, useTraslate } from '@hooks';

interface ModalSetNameSoundProps {
  path: string;
  autoName: string;
}

export default function ModalSetNameSound({ path, autoName }: ModalSetNameSoundProps) {
  const { t } = useTraslate();
  const { loadSound } = useContextEngine();
  const closeModal = useModalClose();

  const nameRef = useRef<HTMLInputElement>(null);

  const handleConfirm = (name: string) => {
    loadSound(path, name);
    closeModal();
  };

  return (
    <div>
      <p className="text-muted small">{t('File:')} {path.replace(/\\/g, '/').split('/').pop()}</p>
      <input
        className="form-control mb-2"
        type="text"
        defaultValue={autoName}
        ref={nameRef}
        placeholder={t('Sound name')}
      />
      <div className="d-flex gap-2 justify-content-end">
        <button
          className="btn btn-secondary btn-sm"
          onClick={closeModal}
        >
          {t('Cancel')}
        </button>
        <button
          className="btn btn-primary btn-sm"
          onClick={() => {
            const name = nameRef.current?.value || autoName;
            handleConfirm(name);
          }}
        >
          {t('Load')}
        </button>
      </div>
    </div>
  );
}
