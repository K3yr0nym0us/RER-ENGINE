import { Accordion } from 'react-bootstrap';
import { Box, Trash } from 'react-bootstrap-icons';
import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import BtnLoadModel from './components/BtnLoadModel';

type ModelEntry = {
  path: string;
  name: string;
};

const ModelsAccordion = () => {
  const { t } = useTraslate();
  const { models, removeModelAsset } = useContextEngine();
  const { openModal, closeModal } = useModal();

  const handleDeleteModel = (model: ModelEntry) => {
    openModal({
      title: t('Delete model'),
      body: (
        <div className="text-center">
          <p>{t('Are you sure you want to delete the model')} <strong>{model.name}</strong>?</p>
          <p className="text-danger">{t('This action cannot be undone.')}</p>
          <div className="d-flex justify-content-end gap-2 mt-3">
            <button className="btn btn-secondary" type="button" onClick={() => closeModal()}>
              {t('Cancel')}
            </button>
            <button
              className="btn btn-danger"
              type="button"
              onClick={() => {
                removeModelAsset(model.path);
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
    <Accordion.Item eventKey="models">
      <Accordion.Header><Box className="me-2" />{t('Models')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <BtnLoadModel />
        <ul className="list-unstyled mt-2 mb-0">
          {models.length === 0 && <li className="text-muted">{t('No models loaded')}</li>}
          {models.map((model) => (
            <li key={model.path} className="mb-1">
              <span className="d-flex align-items-center gap-2 border rounded p-1 ps-2">
                <Box className="flex-shrink-0" />
                <AppTooltip content={model.name} place="top">
                  <span className="text-light small text-truncate flex-fill">{model.name}</span>
                </AppTooltip>
                <AppTooltip content={t('Delete model')} place="top">
                  <button
                    className="btn btn-sm text-danger flex-shrink-0"
                    type="button"
                    onClick={() => handleDeleteModel(model)}
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

export default ModelsAccordion;
