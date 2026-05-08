import { useCallback } from 'react';
import { CardImage } from 'react-bootstrap-icons';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import ModalSetNameBackground from './ModalSetNameBackground';

const BtnLoadBackground = () => {
  const { t } = useTraslate();
  const { openModal } = useModal();

  const handleLoad = useCallback(async () => {
    const path = await window.electronAPI.openBackgroundDialog();
    if (!path) return;
    const autoName = path.replace(/\\/g, '/').split('/').pop()?.replace(/\.[^/.]+$/, '') ?? 'background';
    openModal({
      title: t('Assign name to Background'),
      body: (
        <ModalSetNameBackground
          path={path}
          autoName={autoName}
        />
      ),
    });
  }, [openModal]);

  return (
    <button
      className="btn btn-outline-primary btn-sm w-100 mb-2"
      type="button"
      onClick={handleLoad}
    >
      <CardImage className="me-1" /> {t('Load Background')}
    </button>
  );
};

export default BtnLoadBackground;
