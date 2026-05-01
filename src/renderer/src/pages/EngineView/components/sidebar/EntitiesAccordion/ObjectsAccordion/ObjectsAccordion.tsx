import { PlusLg } from 'react-bootstrap-icons';

import { CreateEntityFromSpriteModalBody } from '../components/CreateEntityFromSpriteModalBody';

import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useCreateEntityFromSpriteAnimation } from '../../../../../../hooks/useCreateEntityFromSpriteAnimation';

export function ObjectsAccordion() {
  const { 
    engineReady, 
    sprites,
  } = useContextEngine()
  const { openModal } = useModal()
  const createObjectFromSprite = useCreateEntityFromSpriteAnimation('load_scenario')

  const handleCreateObject = () => {
    openModal({
      title: 'Crear objeto',
      body: <CreateEntityFromSpriteModalBody
        sprites={sprites} 
        onCreateEntity={createObjectFromSprite}
        previewTitle="Configurar objeto"
      />,
    });
  }

  return (
    <button
      className="btn btn-outline-warning btn-sm w-100 fw-bold mb-2"
      disabled={!engineReady}
      onClick={handleCreateObject}
    >
      <PlusLg className="me-2" />
      Crear objeto
    </button>
  )
}

export default ObjectsAccordion;
