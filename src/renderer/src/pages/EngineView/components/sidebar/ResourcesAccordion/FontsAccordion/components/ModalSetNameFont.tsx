import { useRef } from 'react';
import { useModalClose, useTraslate } from '@hooks';

export interface ModalSetNameFontConfirmPayload {
  path: string;
  name: string;
}

interface ModalSetNameFontProps {
  path: string;
  autoName: string;
  /** Registrado en el padre vía modal Electron (la ventana hijo no tiene EngineProvider). */
  onConfirm?: (payload: ModalSetNameFontConfirmPayload) => void;
}

function ModalSetNameFont({ path, autoName, onConfirm }: ModalSetNameFontProps) {
  const { t } = useTraslate();
  const closeModal = useModalClose();
  const nameRef = useRef<HTMLInputElement>(null);

  const handleLoad = () => {
    const name = nameRef.current?.value?.trim() || autoName;
    onConfirm?.({ path, name });
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
        placeholder={t('Font name')}
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
          onClick={handleLoad}
        >
          {t('Load')}
        </button>
      </div>
    </div>
  );
}

ModalSetNameFont.displayName = 'ModalSetNameFont';

export default ModalSetNameFont;
