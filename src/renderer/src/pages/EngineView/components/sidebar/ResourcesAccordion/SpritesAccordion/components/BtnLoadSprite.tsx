import { useCallback } from 'react';

import { Image } from 'react-bootstrap-icons';
import ModalSetNameSprite from './ModalSetNameSprite';

import { useModal } from '@modal';
import { useTraslate } from '@hooks';

export const BtnLoadSprite = () => {
  const { t } = useTraslate();
  const { openModal } = useModal();

  const openLoadSpriteModal = useCallback(async () => {
    const path = await window.electronAPI.openSpriteDialog();
    if (!path) return;
    const autoName = path.split('/').pop()?.replace(/\.[^/.]+$/, '') ?? 'sprite';
    openModal({
      title: t('Assign name to Sprite'),
      body: (
        <ModalSetNameSprite
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
      onClick={openLoadSpriteModal}
    >
      <Image className="me-1" /> {t('Load Sprite')}
    </button>
  );
};

export default BtnLoadSprite;
