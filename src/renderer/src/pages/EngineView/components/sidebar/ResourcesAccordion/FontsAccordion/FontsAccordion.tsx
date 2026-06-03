import { Accordion } from 'react-bootstrap';
import { Type, Trash } from 'react-bootstrap-icons';
import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import { ModalConfirmBody } from '../../../../../../modal-electron/ModalConfirmBody';
import BtnLoadFont from './components/BtnLoadFont';

type Font = {
  path: string;
  name: string;
};

const FontsAccordion = () => {
  const { t } = useTraslate();
  const { fonts, removeFont } = useContextEngine();
  const { openModal } = useModal();

  const handleDeleteFont = (font: Font) => {
    openModal({
      title: t('Delete Font'),
      size: 'sm',
      body: (
        <ModalConfirmBody
          message={
            <>
              <p className="mb-2">
                {t('Are you sure you want to delete the font')} <strong>{font.name}</strong>?
              </p>
              <p className="text-secondary small mb-0">{t('This action cannot be undone.')}</p>
            </>
          }
          confirmLabel={t('Yes, Delete')}
          onConfirm={() => removeFont(font.path)}
        />
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
