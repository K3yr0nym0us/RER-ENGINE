import { useCallback } from 'react';
import { Image } from 'react-bootstrap-icons';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import ModalSetNameHudImage from './ModalSetNameHudImage';

const BtnLoadImage = () => {
  const { t } = useTraslate();
  const { openModal } = useModal();

  const handleLoad = useCallback(async () => {
    const path = await window.electronAPI.openSpriteDialog();
    if (!path) return;
    const autoName = path.replace(/\\/g, '/').split('/').pop()?.replace(/\.[^/.]+$/, '') ?? 'image';
    openModal({
      title: t('Assign name to image'),
      body: (
        <ModalSetNameHudImage
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
      <Image className="me-1" aria-hidden /> {t('Load image (PNG, JPEG, WebP)')}
    </button>
  );
};

export default BtnLoadImage;
