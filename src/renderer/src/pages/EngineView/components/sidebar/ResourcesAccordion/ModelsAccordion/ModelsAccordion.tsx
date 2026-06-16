import { Accordion, Spinner } from 'react-bootstrap';
import { Box, Trash } from 'react-bootstrap-icons';
import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import { ModalConfirmBody } from '../../../../../../modal-electron/ModalConfirmBody';
import type { ModelInfo } from '@shared-types';
import BtnLoadModel from './components/BtnLoadModel';
import SidebarSubAccordion from '../../SidebarSubAccordion';

const ModelsAccordion = () => {
  const { t } = useTraslate();
  const { models, removeModelAsset } = useContextEngine();
  const { openModal } = useModal();
  const grouped = {
    character: models.filter((m) => m.category === 'character'),
    environment: models.filter((m) => m.category === 'environment'),
    object: models.filter((m) => m.category === 'object' || m.category == null),
    weapon: models.filter((m) => m.category === 'weapon'),
    projectile: models.filter((m) => m.category === 'projectile'),
  };

  const handleDeleteModel = (model: ModelInfo) => {
    openModal({
      title: t('Delete model'),
      size: 'sm',
      body: (
        <ModalConfirmBody
          message={
            <>
              <p className="mb-2">
                {t('Are you sure you want to delete the model')} <strong>{model.name}</strong>?
              </p>
              <p className="text-danger small mb-0">{t('This action cannot be undone.')}</p>
            </>
          }
          confirmLabel={t('Yes, Delete')}
          onConfirm={() => removeModelAsset(model.path)}
        />
      ),
    });
  };

  return (
    <Accordion.Item eventKey="models">
      <Accordion.Header><Box className="me-2" />{t('Models')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <SidebarSubAccordion>
          {([
            ['character', t('Characters')],
            ['environment', t('Environments')],
            ['object', t('Objects')],
            ['weapon', t('Weapons')],
            ['projectile', t('Projectiles')],
          ] as const).map(([key, label]) => {
            const list = grouped[key];
            return (
              <Accordion.Item eventKey={`models-${key}`} key={key}>
                <Accordion.Header>{label} ({list.length})</Accordion.Header>
                <Accordion.Body className="py-2 px-2">
                  <BtnLoadModel category={key} />
                  <ul className="list-unstyled mb-0">
                    {list.length === 0 && (
                      <li className="text-muted small">{t('No models loaded')}</li>
                    )}
                    {list.map((model) => (
                      <li key={`${key}-${model.model_id ?? model.path}`} className="mb-1">
                        <span className="d-flex align-items-center gap-2 border rounded p-1 ps-2">
                          <Box className="flex-shrink-0" />
                          <AppTooltip content={model.name} place="top">
                            <span className="text-light small text-truncate flex-fill">{model.name}</span>
                          </AppTooltip>
                          {model.loading ? (
                            <AppTooltip content={t('Preloading model into memory…')} place="left">
                              <span className="d-inline-flex flex-shrink-0" tabIndex={0}>
                                <Spinner
                                  animation="border"
                                  size="sm"
                                  variant="primary"
                                  role="status"
                                  aria-label={t('Preloading model into memory…')}
                                />
                              </span>
                            </AppTooltip>
                          ) : (
                            <AppTooltip content={t('Delete model')} place="left">
                              <button
                                className="btn btn-sm text-danger flex-shrink-0"
                                type="button"
                                onClick={() => handleDeleteModel(model)}
                              >
                                <Trash />
                              </button>
                            </AppTooltip>
                          )}
                        </span>
                      </li>
                    ))}
                  </ul>
                </Accordion.Body>
              </Accordion.Item>
            );
          })}
        </SidebarSubAccordion>
      </Accordion.Body>
    </Accordion.Item>
  );
};

export default ModelsAccordion;
