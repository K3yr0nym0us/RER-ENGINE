import { PlusLg } from 'react-bootstrap-icons';

import { CreateEntityFromModelModalBody } from './CreateEntityFromModelModalBody';
import type { EntityCategory } from '@shared-types';
import type { EntityMeta } from '@engine';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';

export type ModelEntityIntent = 'environment' | 'character' | 'object' | 'weapon' | 'projectile';

const ENTITY_CATEGORY_BY_INTENT: Partial<Record<ModelEntityIntent, EntityCategory>> = {
  environment: 'environment',
  object: 'object',
  weapon: 'weapon',
  projectile: 'projectile',
};

const INTENT_CONFIG: Record<
  ModelEntityIntent,
  { btnClass: string; titleKey: string; labelKey: string; kind: EntityMeta['kind'] }
> = {
  environment: {
    btnClass: 'btn-outline-info',
    titleKey: 'Create environment',
    labelKey: 'Create environment',
    kind: 'model',
  },
  character: {
    btnClass: 'btn-outline-success',
    titleKey: 'Create character',
    labelKey: 'Create character',
    kind: 'character',
  },
  object: {
    btnClass: 'btn-outline-warning',
    titleKey: 'Create object',
    labelKey: 'Create object',
    kind: 'model',
  },
  weapon: {
    btnClass: 'btn-outline-danger',
    titleKey: 'Create weapon',
    labelKey: 'Create weapon',
    kind: 'model',
  },
  projectile: {
    btnClass: 'btn-outline-secondary',
    titleKey: 'Create projectile',
    labelKey: 'Create projectile',
    kind: 'model',
  },
};

interface Props {
  intent: ModelEntityIntent;
}

export function BtnCreateEntityFromModel({ intent }: Props) {
  const { t } = useTraslate();
  const { engineReady, spawnModel } = useContextEngine();
  const { openModal } = useModal();
  const config = INTENT_CONFIG[intent];

  const handleCreate = () => {
    openModal({
      title: t(config.titleKey),
      body: (
        <CreateEntityFromModelModalBody
          intent={intent}
          onSpawn={(path) => {
            spawnModel(path, config.kind, ENTITY_CATEGORY_BY_INTENT[intent]);
          }}
        />
      ),
    });
  };

  return (
    <button
      className={`btn ${config.btnClass} btn-sm w-100 fw-bold mb-2`}
      type="button"
      disabled={!engineReady}
      onClick={handleCreate}
    >
      <PlusLg className="me-2" />
      {t(config.labelKey)}
    </button>
  );
}

export default BtnCreateEntityFromModel;
