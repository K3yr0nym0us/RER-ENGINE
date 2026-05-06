import { PlusLg } from 'react-bootstrap-icons';

import { CreateEntityFromSpriteModalBody } from '../components/CreateEntityFromSpriteModalBody';

import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useCreateEntityFromSpriteAnimation } from '@hooks';
import { useTraslate } from '@hooks';

export interface AssetGroupConfig {
  openDialog:  () => Promise<string | null>
  loadCmd:     string
  dupCmd:      string
  addBtnLabel: string
  emptyText:   string
}

interface Props {
  config: AssetGroupConfig
}

export function EnvironmentsAccordion({ config }: Props) {
  const { t } = useTraslate()
  const { 
    engineReady, 
    sprites,
  } = useContextEngine()
  const { openModal } = useModal()
  const createEnvironmentFromSprite = useCreateEntityFromSpriteAnimation(config.loadCmd as 'load_scenario')

  const handleCreateEnvironment = () => {
    openModal({
      title: t('Create environment'),
      body: <CreateEntityFromSpriteModalBody
        sprites={sprites} 
        onCreateEntity={createEnvironmentFromSprite}
        previewTitle={t('Configure environment')}
      />,
    });
  }

  return (
    <button
      className="btn btn-outline-info btn-sm w-100 fw-bold mb-2"
      disabled={!engineReady}
      onClick={handleCreateEnvironment}
    >
      <PlusLg className="me-2" />
      {t('Create environment')}
    </button>
  )
}

export default EnvironmentsAccordion;