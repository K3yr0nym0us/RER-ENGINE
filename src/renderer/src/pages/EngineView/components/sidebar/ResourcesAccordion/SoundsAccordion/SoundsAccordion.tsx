import { Accordion } from 'react-bootstrap';
import { MusicNote, Trash } from 'react-bootstrap-icons';
import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { ModalConfirmBody } from '../../../../../../modal-electron/ModalConfirmBody';
import { useTraslate } from '@hooks';
import BtnLoadSound from './components/BtnLoadSound';

type Sound = {
  path: string;
  name: string;
}

const SoundsAccordion = () => {
  const { t } = useTraslate();
  const { sounds, removeSound } = useContextEngine();
  const { openModal } = useModal();

  const handleDeleteSound = (sound: Sound) => {
    openModal({
      title: t('Delete Sound'),
      size: 'sm',
      body: (
        <ModalConfirmBody
          confirmLabel={t('Yes, Delete')}
          message={
            <div className="text-center">
              <p className="mb-2">
                {t('Are you sure you want to delete the sound')} <strong>{sound.name}</strong>?
              </p>
              <p className="text-danger small mb-0">{t('This action cannot be undone.')}</p>
            </div>
          }
          onConfirm={() => removeSound(sound.path)}
        />
      ),
    });
  };

  return (
    <Accordion.Item eventKey="sounds">
      <Accordion.Header><MusicNote className="me-2" />{t('Sounds')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <BtnLoadSound />
        <ul className="list-unstyled mt-2 mb-0">
          {sounds.length === 0 && <li className="text-muted">{t('No sounds loaded')}</li>}
          {sounds.map((sound) => (
            <li key={sound.path} className="mb-1">
              <span className="d-flex align-items-center gap-2 border rounded p-1 ps-2">
                <MusicNote className="flex-shrink-0" />
                <AppTooltip content={sound.name} place="top">
                  <span className="text-light small text-truncate flex-fill">{sound.name}</span>
                </AppTooltip>
                <AppTooltip content={t('Remove Sound')} place="top">
                  <button
                    className="btn btn-sm text-danger flex-shrink-0"
                    onClick={() => handleDeleteSound(sound)}
                  >
                    <Trash />
                  </button>
                </AppTooltip>
              </span>
            </li>
          ))}
        </ul>
      </Accordion.Body>
    </Accordion.Item>
  );
};

export default SoundsAccordion;
