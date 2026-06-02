import { useRef } from 'react';
import { useModal } from '@modal';
import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';

interface ModalSetNameHudImageProps {
  path: string;
  autoName: string;
}

export default function ModalSetNameHudImage({ path, autoName }: ModalSetNameHudImageProps) {
  const { t } = useTraslate();
  const { loadHudImage } = useContextEngine();
  const { closeModal } = useModal();

  const nameRef = useRef<HTMLInputElement>(null);

  const handleConfirm = (name: string) => {
    loadHudImage(path, name);
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
        placeholder={t('Image name')}
      />
      <div className="d-flex gap-2 justify-content-end">
        <button
          className="btn btn-secondary btn-sm"
          type="button"
          onClick={closeModal}
        >
          {t('Cancel')}
        </button>
        <button
          className="btn btn-primary btn-sm"
          type="button"
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
