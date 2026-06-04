import { useRef } from 'react';
import { useModalClose, useTraslate } from '@hooks';
import type { ModelCategory } from '@shared-types';

export interface ModalSetNameModelConfirmPayload {
  path: string;
  name: string;
  category: ModelCategory;
}

interface ModalSetNameModelProps {
  path: string;
  autoName: string;
  category: ModelCategory;
  /** Registrado en el padre vía modal Electron (la ventana hijo no tiene EngineProvider). */
  onConfirm?: (payload: ModalSetNameModelConfirmPayload) => void;
}

function ModalSetNameModel({ path, autoName, category, onConfirm }: ModalSetNameModelProps) {
  const { t } = useTraslate();
  const closeModal = useModalClose();
  const nameRef = useRef<HTMLInputElement>(null);

  const handleLoad = () => {
    const name = nameRef.current?.value?.trim() || autoName;
    onConfirm?.({ path, name, category });
    closeModal();
  };

  return (
    <div>
      <p className="text-muted small">{t('File:')} {path.split(/[/\\]/).pop()}</p>
      <input
        className="form-control mb-2"
        type="text"
        defaultValue={autoName}
        ref={nameRef}
        placeholder={t('Model name')}
      />
      <div className="d-flex gap-2 justify-content-end">
        <button className="btn btn-secondary btn-sm" type="button" onClick={closeModal}>
          {t('Cancel')}
        </button>
        <button className="btn btn-primary btn-sm" type="button" onClick={handleLoad}>
          {t('Load')}
        </button>
      </div>
    </div>
  );
}

ModalSetNameModel.displayName = 'ModalSetNameModel';

export default ModalSetNameModel;
