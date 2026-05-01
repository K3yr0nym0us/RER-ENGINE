import { PlusLg } from 'react-bootstrap-icons';
import { CreateEntityFromSpriteModalBody } from '../../components/CreateEntityFromSpriteModalBody';

import { useModal } from '../../../../../../../context/ModalContext';
import { useContextEngine } from '../../../../../../../context/useContextEngine';
import { useCreateEntityFromSpriteAnimation } from '../../../../../../../hooks/useCreateEntityFromSpriteAnimation';

const BtnCreateCharacter = () => {
  const { openModal } = useModal();
  const { sprites } = useContextEngine();
  const createCharacterFromSprite = useCreateEntityFromSpriteAnimation('load_character');

  const handleClick = () => {
    openModal({
      title: 'Crear personaje',
      body: <CreateEntityFromSpriteModalBody
        sprites={sprites} 
        onCreateEntity={createCharacterFromSprite}
        previewTitle="Configurar personaje"
      />,
    });
  };

  return (
    <button 
      className="btn btn-outline-success btn-sm w-100 fw-bold mb-2" 
      onClick={handleClick}
    >
      <PlusLg className="me-2" />
      Crear personaje
    </button>
  );
};

export default BtnCreateCharacter;
