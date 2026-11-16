import { useState } from 'react';
import { useModalClose, useTraslate } from '@hooks';
import type { ModelInfo } from '@shared-types';
import type { SceneTab } from '../../../../../hooks/useSceneManager';

export interface ModalLoadFromSceneConfirmPayload {
  modelId: string;
}

interface ModalLoadFromSceneProps {
  scenes?: SceneTab[];
  /** `ModelInfo[]` del snapshot del engine (`buildEngineSnapshot`): `scene_id` presente = recurso por escena. */
  models?: ModelInfo[];
  activeSceneId?: number;
  /** Registrado en el padre vía `openModal` (confirma → `warm_scene_model`). */
  onConfirm?: (payload: ModalLoadFromSceneConfirmPayload) => void;
}

/**
 * Modal "Cargar desde otra escena" (Recursos por escena):
 * lista los `.rerasset` por escena guardados en el `.save` y los carga en GPU
 * sin reimportar (`warm_scene_model`).
 */
function ModalLoadFromScene({
  scenes,
  models,
  activeSceneId,
  onConfirm,
}: ModalLoadFromSceneProps) {
  const { t } = useTraslate();
  const closeModal = useModalClose();
  const [selected, setSelected] = useState<string | null>(null);

  const sceneName = (sceneId: number) =>
    (scenes ?? []).find((scene) => scene.id === sceneId)?.name ??
    `${t('Scene')} ${sceneId}`;

  const perScene = (models ?? []).filter(
    (m) => m.scene_id != null && m.model_id && m.state !== 'importing' && m.state !== 'failed',
  );
  const grouped = new Map<number, ModelInfo[]>();
  for (const model of perScene) {
    const key = model.scene_id as number;
    const list = grouped.get(key) ?? [];
    list.push(model);
    grouped.set(key, list);
  }

  const handleLoad = () => {
    if (!selected) return;
    onConfirm?.({ modelId: selected });
    closeModal();
  };

  return (
    <div>
      {grouped.size === 0 ? (
        <p className="text-muted small mb-2">
          {t(
            'No per-scene resources found yet. Save the project to write scene resources into the .save.',
          )}
        </p>
      ) : (
        <div className="mb-2" style={{ maxHeight: '18rem', overflowY: 'auto' }}>
          {[...grouped.entries()].map(([sceneId, sceneModels]) => (
            <div key={sceneId} className="mb-2">
              <div className="small fw-semibold text-muted mb-1">
                {sceneName(sceneId)}
                {sceneId === activeSceneId ? ` (${t('active')})` : ''}
              </div>
              {sceneModels.map((model) => (
                <label
                  key={model.model_id}
                  className="d-flex align-items-center gap-2 ms-2 py-1"
                  style={{ cursor: 'pointer' }}
                >
                  <input
                    type="radio"
                    name="load-from-scene-model"
                    checked={selected === model.model_id}
                    onChange={() => setSelected(model.model_id ?? null)}
                  />
                  <span className="small">{model.name}</span>
                </label>
              ))}
            </div>
          ))}
        </div>
      )}
      <div className="d-flex gap-2 justify-content-end">
        <button className="btn btn-secondary btn-sm" type="button" onClick={closeModal}>
          {t('Cancel')}
        </button>
        <button
          className="btn btn-primary btn-sm"
          type="button"
          disabled={!selected}
          onClick={handleLoad}
        >
          {t('Load')}
        </button>
      </div>
    </div>
  );
}

ModalLoadFromScene.displayName = 'ModalLoadFromScene';

export default ModalLoadFromScene;
