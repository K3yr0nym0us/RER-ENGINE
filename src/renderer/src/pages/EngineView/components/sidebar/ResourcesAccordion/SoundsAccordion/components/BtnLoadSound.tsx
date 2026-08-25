import { useCallback } from 'react';
import { MusicNoteBeamed } from 'react-bootstrap-icons';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import ModalSetNameSound from './ModalSetNameSound';

const BtnLoadSound = () => {
  const { t } = useTraslate();
  const { openModal } = useModal();

  const handleLoad = useCallback(async () => {
    const path = await window.electronAPI.openAudioDialog();
    if (!path) return;
    const autoName = path.replace(/\\/g, '/').split('/').pop()?.replace(/\.[^/.]+$/, '') ?? 'sound';
    openModal({
      title: t('Assign name to Sound'),
      body: (
        <ModalSetNameSound
          path={path}
          autoName={autoName}
        />
      ),
    });
  }, [openModal, t]);

  return (
    <button
      className="btn btn-outline-primary btn-sm w-100 mb-2"
      type="button"
      onClick={handleLoad}
    >
      <MusicNoteBeamed className="me-1" /> {t('Load Sound')}
    </button>
  );
};

export default BtnLoadSound;
