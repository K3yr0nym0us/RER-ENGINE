import { useCallback } from 'react';
import { Type } from 'react-bootstrap-icons';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import ModalSetNameFont from './ModalSetNameFont';

const BtnLoadFont = () => {
  const { t } = useTraslate();
  const { openModal } = useModal();

  const handleLoad = useCallback(async () => {
    const path = await window.electronAPI.openFontDialog();
    if (!path) return;
    const autoName = path.replace(/\\/g, '/').split('/').pop()?.replace(/\.[^/.]+$/, '') ?? 'font';
    openModal({
      title: t('Assign name to Font'),
      body: (
        <ModalSetNameFont
          path={path}
          autoName={autoName}
        />
      ),
    });
  }, [openModal, t]);

  return (
    <button
      className="btn btn-outline-primary btn-sm w-100 mb-2"
      type="button"
      onClick={handleLoad}
    >
      <Type className="me-1" /> {t('Load Font (.ttf, .otf)')}
    </button>
  );
};

export default BtnLoadFont;
