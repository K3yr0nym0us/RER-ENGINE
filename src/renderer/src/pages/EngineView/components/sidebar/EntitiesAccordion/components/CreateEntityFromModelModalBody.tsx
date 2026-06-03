import type { ModelInfo } from '@shared-types';
import { useTraslate } from '@hooks';
import { ModelPickerEntry } from './ModelPickerEntry';

interface Props {
  onSpawn: (path: string) => void;
  intent: 'environment' | 'character' | 'object';
  /** Lista inyectada por modal Electron (sin EngineProvider en ventana hijo). */
  models?: ModelInfo[];
  hintText?: string;
}

/**
 * Lista de modelos precargados para modales de entidades.
 * Lee `models` del contexto en vivo (no snapshot) para actualizar spinners de precarga.
 */
export function CreateEntityFromModelModalBody({ onSpawn, intent, models = [], hintText }: Props) {
  const { t } = useTraslate();
  const filteredModels = models.filter((model) => {
    if (intent === 'character') return model.category === 'character';
    if (intent === 'environment') return model.category === 'environment';
    return model.category === 'object' || model.category == null;
  });

  if (filteredModels.length === 0) {
    return (
      <p className="mb-0 text-muted">
        {t('No models available for this category. Load models first in the Models accordion.')}
      </p>
    );
  }

  return (
    <>
      {hintText ? <p className="text-secondary small">{hintText}</p> : null}
    <ul className="list-unstyled mb-0">
      {filteredModels.map((model) => (
        <ModelPickerEntry key={model.path} model={model} onSelect={onSpawn} />
      ))}
    </ul>
    </>
  );
}

export default CreateEntityFromModelModalBody;
