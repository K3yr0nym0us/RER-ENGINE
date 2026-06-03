import { Accordion } from 'react-bootstrap';

import { Image, Trash } from 'react-bootstrap-icons';
import { AppTooltip } from '@components';
import BtnLoadBackground from './components/BtnLoadBackground';

import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import { ModalConfirmBody } from '../../../../../../modal-electron/ModalConfirmBody';

type Background = {
  path: string;
  name: string;
}

const BackgroundsAccordion = () => {
  const { t } = useTraslate();
  const { backgroundPath, backgrounds, removeBackgroundFromLibrary } = useContextEngine();
  const { openModal } = useModal();

  const handleDeleteBackground = (bg: Background) => {
    openModal({
      title: t('Delete Background'),
      size: 'sm',
      body: (
        <ModalConfirmBody
          message={
            <>
              <p className="mb-2">
                {t('Are you sure you want to delete the background')} <strong>{bg.name}</strong>?
              </p>
              <p className="text-secondary small mb-0">{t('This action cannot be undone.')}</p>
            </>
          }
          confirmLabel={t('Yes, Delete')}
          onConfirm={() => removeBackgroundFromLibrary(bg.path)}
        />
      ),
    });
  };

  return (
    <Accordion.Item eventKey="backgrounds">
      <Accordion.Header><Image className="me-2" />{t('Backgrounds')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <BtnLoadBackground />
        <ul className="list-unstyled mt-2 mb-0">
          {backgrounds.length === 0 && (
            <li className="text-muted">{t('No backgrounds loaded')}</li>
          )}
          {backgrounds.map((bg) => {
            const isActive = bg.path === backgroundPath;
            return (
              <li key={bg.path} className="mb-1">
                <span
                  className={`d-flex align-items-center gap-2 border rounded p-1 ps-2 ${isActive ? 'border-primary' : ''}`}
                >
                  <Image className="flex-shrink-0" />
                  <AppTooltip content={bg.name} place="top">
                    <span className="text-light small text-truncate flex-fill">{bg.name}</span>
                  </AppTooltip>
                  <AppTooltip content={t('Remove Background')} place="top">
                    <button
                      className="btn btn-sm text-danger flex-shrink-0"
                      onClick={() => handleDeleteBackground(bg)}
                    >
                      <Trash />
                    </button>
                  </AppTooltip>
                </span>
              </li>
            );
          })}
        </ul>
      </Accordion.Body>
    </Accordion.Item>
  );
};

export default BackgroundsAccordion;
