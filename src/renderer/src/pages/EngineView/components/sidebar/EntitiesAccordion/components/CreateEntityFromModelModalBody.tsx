import { useEffect, useState } from 'react';

import type { ModelInfo } from '@shared-types';
import { useTraslate } from '@hooks';
import { ModelPickerEntry } from './ModelPickerEntry';

interface Props {
  onSpawn: (path: string) => void;
  intent: 'environment' | 'character' | 'object';
  /** Lista inyectada por modal Electron (snapshot IPC al abrir). */
  models?: ModelInfo[];
  hintText?: string;
  /** Id de la ventana modal Electron; recibe parches en vivo cuando un modelo termina de cargar. */
  handlerId?: string;
}

/**
 * Lista de modelos precargados para modales de entidades.
 * En ventana modal Electron se actualiza vía `patchModalElectron` cuando cambia `engine.models`.
 */
export function CreateEntityFromModelModalBody({
  onSpawn,
  intent,
  models: modelsProp = [],
  hintText,
  handlerId,
}: Props) {
  const { t } = useTraslate();
  const [models, setModels] = useState<ModelInfo[]>(modelsProp);

  useEffect(() => {
    setModels(modelsProp);
  }, [modelsProp]);

  useEffect(() => {
    if (!handlerId) return;
    const remove = window.electronAPI.onModalElectronPatch((data) => {
      if (data.handlerId !== handlerId || !data.models) return;
      setModels(data.models);
    });
    return remove;
  }, [handlerId]);

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
