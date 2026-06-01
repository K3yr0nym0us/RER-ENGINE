import { Accordion } from 'react-bootstrap';
import { Type, Trash } from 'react-bootstrap-icons';
import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import BtnLoadFont from './components/BtnLoadFont';

type Font = {
  path: string;
  name: string;
};

const FontsAccordion = () => {
  const { t } = useTraslate();
  const { fonts, removeFont } = useContextEngine();
  const { openModal, closeModal } = useModal();

  const handleDeleteFont = (font: Font) => {
    openModal({
      title: t('Delete Font'),
      body: (
        <div className="text-center">
          <p>{t('Are you sure you want to delete the font')} <strong>{font.name}</strong>?</p>
          <p>{t('This action cannot be undone.')}</p>
          <div className="d-flex justify-content-end gap-2 mt-3">
            <button className="btn btn-secondary" onClick={() => closeModal()}>
              {t('Cancel')}
            </button>
            <button
              className="btn btn-danger"
              onClick={() => {
                removeFont(font.path);
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
    <Accordion.Item eventKey="fonts">
      <Accordion.Header><Type className="me-2" />{t('Fonts')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <BtnLoadFont />
        <ul className="list-unstyled mt-2 mb-0">
          {fonts.length === 0 && <li className="text-muted">{t('No fonts loaded')}</li>}
          {fonts.map((font) => (
            <li key={font.path} className="mb-1">
              <span className="d-flex align-items-center gap-2 border rounded p-1 ps-2">
                <Type className="flex-shrink-0" />
                <AppTooltip content={font.name} place="top">
                  <span className="text-light small text-truncate flex-fill">{font.name}</span>
                </AppTooltip>
                <AppTooltip content={t('Remove Font')} place="top">
                  <button
                    className="btn btn-sm text-danger flex-shrink-0"
                    onClick={() => handleDeleteFont(font)}
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

export default FontsAccordion;
