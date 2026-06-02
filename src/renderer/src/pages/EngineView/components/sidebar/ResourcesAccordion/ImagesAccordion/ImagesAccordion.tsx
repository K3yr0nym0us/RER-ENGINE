import { Accordion } from 'react-bootstrap';
import { Image, Trash } from 'react-bootstrap-icons';
import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import BtnLoadImage from './components/BtnLoadImage';

type HudImage = {
  path: string;
  name: string;
};

const ImagesAccordion = () => {
  const { t } = useTraslate();
  const { hudImages, removeHudImage } = useContextEngine();
  const { openModal, closeModal } = useModal();

  const handleDeleteImage = (image: HudImage) => {
    openModal({
      title: t('Delete image'),
      body: (
        <div className="text-center">
          <p>{t('Are you sure you want to delete the image')} <strong>{image.name}</strong>?</p>
          <p>{t('This action cannot be undone.')}</p>
          <div className="d-flex justify-content-end gap-2 mt-3">
            <button className="btn btn-secondary" type="button" onClick={() => closeModal()}>
              {t('Cancel')}
            </button>
            <button
              className="btn btn-danger"
              type="button"
              onClick={() => {
                removeHudImage(image.path);
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
    <Accordion.Item eventKey="hud-images">
      <Accordion.Header><Image className="me-2" />{t('Images')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <BtnLoadImage />
        <ul className="list-unstyled mt-2 mb-0">
          {hudImages.length === 0 && <li className="text-muted">{t('No images loaded')}</li>}
          {hudImages.map((image) => (
            <li key={image.path} className="mb-1">
              <span className="d-flex align-items-center gap-2 border rounded p-1 ps-2">
                <Image className="flex-shrink-0" aria-hidden />
                <AppTooltip content={image.name} place="top">
                  <span className="text-light small text-truncate flex-fill">{image.name}</span>
                </AppTooltip>
                <AppTooltip content={t('Remove image')} place="top">
                  <button
                    className="btn btn-sm text-danger flex-shrink-0"
                    type="button"
                    onClick={() => handleDeleteImage(image)}
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

export default ImagesAccordion;
