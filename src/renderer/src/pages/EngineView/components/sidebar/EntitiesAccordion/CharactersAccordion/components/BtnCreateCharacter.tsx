import { PlusLg } from 'react-bootstrap-icons';
import { CreateCharacterModalBody } from './CreateCharacterModalBody';
import { Sprite } from '../../../SpritesAccordion/SpritesAccordion';

import { useModal } from '../../../../../../../context/ModalContext';

interface BtnCreateCharacterProps {
  sprites: Sprite[];
  openDialog: () => Promise<string | null>;
}

const BtnCreateCharacter = ({ sprites, openDialog }: BtnCreateCharacterProps) => {
  const { openModal } = useModal();

  const handleClick = () => {
    openModal({
      title: 'Crear personaje',
      body: <CreateCharacterModalBody 
        sprites={sprites} 
        openDialog={openDialog} 
      />
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