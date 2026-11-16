import { useEffect, useState } from 'react';

import type { ModelInfo } from '@shared-types';
import { useTraslate } from '@hooks';
import { ModelPickerEntry } from './ModelPickerEntry';

interface Props {
  onSpawn: (path: string) => void;
  intent: 'environment' | 'character' | 'object' | 'weapon' | 'projectile';
  /** Lista inyectada por modal Electron (snapshot IPC al abrir). */
  models?: ModelInfo[];
  hintText?: string;
  /** Id de la ventana modal Electron; recibe parches en vivo cuando un modelo termina de cargar. */
  handlerId?: string;
  /** Modelo asignado actualmente a la entidad: no se ofrece para reemplazarse a sí mismo. */
  currentModelPath?: string;
}

/** Normaliza rutas/miembros del motor para comparar el modelo actual. */
function normalizeModelKey(path: string): string {
  return path.replace(/\\/g, '/').toLowerCase();
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
  currentModelPath,
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

  // Solo modelos globales o de la escena activa: el motor marca los de otras
  // escenas con `available_in_scene: false` (se globalizan desde «Cargar desde
  // otra escena»).
  const scopedModels = models.filter((model) => model.available_in_scene !== false);
  const categoryModels = scopedModels.filter((model) => {
    if (intent === 'character') return model.category === 'character';
    if (intent === 'environment') return model.category === 'environment';
    if (intent === 'weapon') return model.category === 'weapon';
    if (intent === 'projectile') return model.category === 'projectile';
    return model.category === 'object' || model.category == null;
  });

  const currentKey = currentModelPath ? normalizeModelKey(currentModelPath) : null;
  const isCurrentModel = (model: ModelInfo) =>
    currentKey !== null
    && [model.path, model.model_id, model.asset].some(
      (value) => typeof value === 'string' && normalizeModelKey(value) === currentKey,
    );

  // Orden estable por nombre: el motor emite la lista desde un HashMap.
  const filteredModels = categoryModels
    .filter((model) => !isCurrentModel(model))
    .sort(
      (a, b) =>
        a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
        || a.path.localeCompare(b.path),
    );

  if (filteredModels.length === 0) {
    return (
      <p className="mb-0 text-muted">
        {categoryModels.length > 0
          ? t('This is the current model. There are no other models for this category.')
          : t('No models available for this category. Load models first in the Models accordion.')}
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
