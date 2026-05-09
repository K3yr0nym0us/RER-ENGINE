import { Accordion } from 'react-bootstrap';
import { MusicNote, Trash } from 'react-bootstrap-icons';
import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import BtnLoadSound from './components/BtnLoadSound';

type Sound = {
  path: string;
  name: string;
}

const SoundsAccordion = () => {
  const { t } = useTraslate();
  const { sounds, removeSound } = useContextEngine();
  const { openModal, closeModal } = useModal();

  const handleDeleteSound = (sound: Sound) => {
    openModal({
      title: t('Delete Sound'),
      body: (
        <div className="text-center">
          <p>{t('Are you sure you want to delete the sound')} <strong>{sound.name}</strong>?</p>
          <p>{t('This action cannot be undone.')}</p>
          <div className="d-flex justify-content-end gap-2 mt-3">
            <button className="btn btn-secondary" onClick={() => closeModal()}>
              {t('Cancel')}
            </button>
            <button
              className="btn btn-danger"
              onClick={() => {
                removeSound(sound.path);
                closeModal();
              }}
            >
              {t('Yes, Delete')}
            </button>
          </div>
        </div>
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
