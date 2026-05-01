import { useCallback, useState } from 'react';
import { useContextEngine } from '../context/useContextEngine';
import { useModal } from '../context/ModalContext';
import ModalSetNameSprite from '../pages/EngineView/components/sidebar/SpritesAccordion/components/ModalSetNameSprite';

/**
 * Hook para cargar un sprite: abre el diálogo, luego modal de nombre, y envía al motor.
 */
export function useLoadSprite() {
  const { loadSprite } = useContextEngine();
  const { openModal, closeModal } = useModal();
  const [selectedPath, setSelectedPath] = useState<string | null>(null);

  const triggerLoad = useCallback(async () => {
    const path = await window.electronAPI.openSpriteDialog();
    if (!path) return;
    setSelectedPath(path);
    const autoName = path.split('/').pop()?.replace(/\.[^/.]+$/, '') ?? 'sprite';
    openModal({
      title: 'Asignar nombre al Sprite',
      body: (
        <ModalSetNameSprite
          path={path}
          autoName={autoName}
          onConfirm={(name: string) => {
            loadSprite(path, name);
            setSelectedPath(null);
            closeModal();
          }}
          onCancel={() => {
            setSelectedPath(null);
            closeModal();
          }}
        />
      ),
    });
  }, [loadSprite, openModal, closeModal]);

  return triggerLoad;
}
