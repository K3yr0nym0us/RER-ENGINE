import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';
import { ModelPickerEntry } from './ModelPickerEntry';

interface Props {
  onSpawn: (path: string) => void;
}

/**
 * Lista de modelos precargados para modales de entidades.
 * Lee `models` del contexto en vivo (no snapshot) para actualizar spinners de precarga.
 */
export function CreateEntityFromModelModalBody({ onSpawn }: Props) {
  const { t } = useTraslate();
  const { models } = useContextEngine();

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
        <ModelPickerEntry key={model.path} model={model} onSelect={onSpawn} />
      ))}
    </ul>
  );
}

export default CreateEntityFromModelModalBody;
