import { PlusLg } from 'react-bootstrap-icons';

import { CreateEntityFromSpriteModalBody } from '../components/CreateEntityFromSpriteModalBody';

import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useCreateEntityFromSpriteAnimation } from '@hooks';
import { useTraslate } from '@hooks';

export function ObjectsAccordion() {
  const { t } = useTraslate()
  const { 
    engineReady, 
    sprites,
  } = useContextEngine()
  const { openModal } = useModal()
  const createObjectFromSprite = useCreateEntityFromSpriteAnimation('load_scenario')

  const handleCreateObject = () => {
    openModal({
      title: t('Create object'),
      body: <CreateEntityFromSpriteModalBody
        sprites={sprites} 
        onCreateEntity={createObjectFromSprite}
        previewTitle={t('Configure object')}
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
      {t('Create object')}
    </button>
  )
}

export default ObjectsAccordion;
