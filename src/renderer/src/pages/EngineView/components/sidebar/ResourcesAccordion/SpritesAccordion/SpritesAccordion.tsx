import { Accordion } from 'react-bootstrap';
import { Image, Trash } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import { BtnLoadSprite } from './components/BtnLoadSprite';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import { ModalConfirmBody } from '../../../../../../modal-electron/ModalConfirmBody';

type Sprite = {
  height: number;
  name: string;
  path: string;
  width: number;
}

const SpritesAccordion = () => {
  const { t } = useTraslate();
  const { sprites, removeSprite } = useContextEngine();
  const { openModal } = useModal();

  const handleDeleteSprite = (sprite: Sprite) => {
    openModal({
      title: t('Delete Sprite'),
      size: 'sm',
      body: (
        <ModalConfirmBody
          message={
            <>
              <p className="mb-2">
                {t('Are you sure you want to delete the sprite')} <strong>{sprite.name}</strong>?
              </p>
              <p className="small mb-1">{t('All configuration linked to the Sprite will be removed.')}</p>
              <p className="text-danger small mb-0">{t('This action cannot be undone.')}</p>
            </>
          }
          confirmLabel={t('Yes, Delete')}
          onConfirm={() => removeSprite(sprite.path)}
        />
      ),
    })
  }

  return (
    <Accordion.Item eventKey="sprites">
      <Accordion.Header><Image className="me-2" />{t('Sprites')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <BtnLoadSprite />
        <ul className="list-unstyled mt-2 mb-0">
          {sprites.length === 0 && <li className="text-muted">{t('No sprites loaded')}</li>}
          {sprites.map((sprite) => (
            <li key={sprite.path} className="mb-1">
              <span className="d-flex align-items-center gap-2 border rounded p-1 ps-2">
                <Image className="flex-shrink-0" />
                <AppTooltip content={sprite.name} place="top">
                  <span className="text-light small text-truncate flex-fill">{sprite.name}</span>
                </AppTooltip>
                <AppTooltip content={t('Delete Sprite')} place="top">
                  <button 
                    className="btn btn-sm text-danger flex-shrink-0"
                    onClick={() => handleDeleteSprite(sprite)}
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

export default SpritesAccordion;
