import type { ModelInfo } from '@shared-types';
import { useTraslate } from '@hooks';

interface Props {
  models: ModelInfo[];
  onSpawn: (path: string) => void;
}

export function CreateEntityFromModelModalBody({ models, onSpawn }: Props) {
  const { t } = useTraslate();

  if (models.length === 0) {
    return (
      <p className="mb-0 text-muted">
        {t('No preloaded models. Load models first in the Models accordion.')}
      </p>
    );
  }

  return (
    <ul className="list-unstyled mb-0">
      {models.map((model) => (
        <li key={model.path} className="mb-2">
          <button
            type="button"
            className="btn btn-outline-light btn-sm w-100 text-start"
            onClick={() => onSpawn(model.path)}
          >
            {model.name}
          </button>
        </li>
      ))}
    </ul>
  );
}

export default CreateEntityFromModelModalBody;
