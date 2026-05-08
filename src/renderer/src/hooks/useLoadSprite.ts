import { useCallback, createElement } from 'react';
import { useModal } from '@modal';
import ModalSetNameSprite from '../pages/EngineView/components/sidebar/ResourcesAccordion/SpritesAccordion/components/ModalSetNameSprite';

/**
 * Hook para cargar un sprite: abre el diálogo, luego modal de nombre, y envía al motor.
 */
export function useLoadSprite() {
  const { openModal } = useModal();

  const triggerLoad = useCallback(async () => {
    const path = await window.electronAPI.openSpriteDialog();
    if (!path) return;

    const autoName = path.split('/').pop()?.replace(/\.[^/.]+$/, '') ?? 'sprite';

    openModal({
      title: 'Asignar nombre al Sprite',
      body: createElement(ModalSetNameSprite, {
        path,
        autoName,
      }),
    });
  }, [openModal]);

  return triggerLoad;
}
