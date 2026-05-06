import { PlusLg } from 'react-bootstrap-icons';
import { CreateEntityFromSpriteModalBody } from '../../components/CreateEntityFromSpriteModalBody';

import { useModal } from '@modal';
import { useContextEngine } from '@engine';
import { useCreateEntityFromSpriteAnimation, useTraslate } from '@hooks';

const BtnCreateCharacter = () => {
  const { t } = useTraslate();
  const { openModal } = useModal();
  const { sprites } = useContextEngine();
  const createCharacterFromSprite = useCreateEntityFromSpriteAnimation('load_character');

  const openCreateCharacterModal = () => {
    openModal({
      title: t('Create character'),
      body: <CreateEntityFromSpriteModalBody
        sprites={sprites} 
        onCreateEntity={createCharacterFromSprite}
        previewTitle={t('Configure character')}
      />,
    });
  };

  return (
    <button 
      className="btn btn-outline-success btn-sm w-100 fw-bold mb-2" 
      onClick={openCreateCharacterModal}
    >
      <PlusLg className="me-2" />
      {t('Create character')}
    </button>
  );
};

export default BtnCreateCharacter;
