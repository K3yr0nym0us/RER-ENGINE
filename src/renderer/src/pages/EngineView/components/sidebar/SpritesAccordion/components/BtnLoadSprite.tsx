import { useCallback } from 'react';

import { Image } from 'react-bootstrap-icons';
import ModalSetNameSprite from './ModalSetNameSprite';

import { useModal } from '../../../../../../context/ModalContext';

export const BtnLoadSprite = () => {

  const { openModal } = useModal();

  const handleClick = useCallback(async () => {
    const path = await window.electronAPI.openSpriteDialog();
    if (!path) return;
    const autoName = path.split('/').pop()?.replace(/\.[^/.]+$/, '') ?? 'sprite';
    openModal({
      title: 'Asignar nombre al Sprite',
      body: (
        <ModalSetNameSprite
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
      onClick={handleClick}
    >
      <Image className="me-1" /> Cargar Sprite
    </button>
  );
};

export default BtnLoadSprite;
