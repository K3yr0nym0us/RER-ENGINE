import { useRef } from 'react';
import { useModal } from '@modal';
import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';
import type { ModelCategory } from '@shared-types';

interface ModalSetNameModelProps {
  path: string;
  autoName: string;
  category: ModelCategory;
}

export default function ModalSetNameModel({ path, autoName, category }: ModalSetNameModelProps) {
  const { t } = useTraslate();
  const { loadModelAsset } = useContextEngine();
  const { closeModal } = useModal();
  const nameRef = useRef<HTMLInputElement>(null);

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
        <button
          className="btn btn-primary btn-sm"
          type="button"
          onClick={() => {
            const name = nameRef.current?.value?.trim() || autoName;
            loadModelAsset(path, name, category);
            closeModal();
          }}
        >
          {t('Load')}
        </button>
      </div>
    </div>
  );
}
