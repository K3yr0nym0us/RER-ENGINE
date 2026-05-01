import { useContextEngine } from '@engine';

/**
 * Hook para cargar un personaje en el motor desde un PNG usando el path seleccionado.
 * Devuelve una función que recibe el path y realiza el envío al engine.
 */
import { useModal } from '@modal';

export function useCreateCharacterFromPng(openDialog: () => Promise<string | null>) {
  const { send } = useContextEngine();
  const { closeModal } = useModal();

  return async () => {
    const p = await openDialog();
    if (!p) {
      closeModal();
      return;
    }
    send({ cmd: 'load_character', path: p });
    closeModal();
  };
}
