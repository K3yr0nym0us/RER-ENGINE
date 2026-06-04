import { Accordion, Spinner } from 'react-bootstrap';
import { BoxArrowInRight, Collection, Pencil, PlusLg, Trash } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import { useTraslate } from '@hooks';
import { useSceneManager } from '../../hooks/useSceneManager';

export function ScenesAccordion() {
  const { t } = useTraslate();
  const {
    scenes,
    activeSceneId,
    scenesListLoading,
    switchingToSceneId,
    sceneActionsDisabled,
    openSwitchSceneModal,
    openCreateSceneModal,
    openRenameSceneModal,
    openDeleteSceneModal,
  } = useSceneManager();

  return (
    <Accordion.Item eventKey="scenes">
      <Accordion.Header><Collection className="me-2" />{t('Scenes')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        {scenesListLoading ? (
          <div
            className="scenes-accordion-loading d-flex flex-column align-items-center justify-content-center gap-2 py-3 text-secondary"
            aria-busy="true"
          >
            <Spinner animation="border" size="sm" variant="info" />
            <span className="small text-center">{t('Loading scenes...')}</span>
          </div>
        ) : (
          <>
        <button
          className="btn btn-sm btn-outline-secondary w-100 d-flex align-items-center justify-content-center gap-1 mb-2"
          type="button"
          onClick={openCreateSceneModal}
          disabled={sceneActionsDisabled}
        >
          <PlusLg size={12} />
          {t('Create new scene')}
        </button>

        <ul className="list-unstyled mb-0 scenes-accordion-list">
          {scenes.map((scene) => {
            const isActive = scene.id === activeSceneId;
            const isSwitching = switchingToSceneId === scene.id;
            return (
              <li
                key={scene.id}
                className={`scenes-accordion-item d-flex align-items-center gap-1 mb-1${isActive ? ' scenes-accordion-item--active' : ''}`}
              >
                <span className="scenes-accordion-item__name flex-grow-1 text-truncate" title={scene.name}>
                  {scene.name}
                </span>
                {!isActive && (
                  isSwitching ? (
                    <span
                      className="d-inline-flex scenes-accordion-icon-wrap scenes-accordion-icon-wrap--loading"
                      aria-busy="true"
                      aria-label={t('Load scene')}
                    >
                      <Spinner animation="border" size="sm" variant="info" />
                    </span>
                  ) : (
                    <AppTooltip content={t('Load scene')} place="top">
                      <span className="d-inline-flex scenes-accordion-icon-wrap">
                        <button
                          className="btn btn-sm btn-link scenes-accordion-icon-btn text-info p-0"
                          type="button"
                          disabled={sceneActionsDisabled}
                          onClick={() => openSwitchSceneModal(scene)}
                          aria-label={`${t('Load scene')} ${scene.name}`}
                        >
                          <BoxArrowInRight size={14} />
                        </button>
                      </span>
                    </AppTooltip>
                  )
                )}
                <AppTooltip content={t('Edit scene name')} place="top">
                  <span className="d-inline-flex scenes-accordion-icon-wrap">
                    <button
                      className="btn btn-sm btn-link scenes-accordion-icon-btn text-primary p-0"
                      type="button"
                      disabled={sceneActionsDisabled}
                      onClick={() => openRenameSceneModal(scene)}
                      aria-label={`${t('Edit')} ${scene.name}`}
                    >
                      <Pencil size={14} />
                    </button>
                  </span>
                </AppTooltip>
                <AppTooltip content={t('Delete scene')} place="top">
                  <span className="d-inline-flex scenes-accordion-icon-wrap">
                    <button
                      className="btn btn-sm btn-link scenes-accordion-icon-btn text-danger p-0"
                      type="button"
                      disabled={sceneActionsDisabled}
                      onClick={() => openDeleteSceneModal(scene)}
                      aria-label={`${t('Delete')} ${scene.name}`}
                    >
                      <Trash size={14} />
                    </button>
                  </span>
                </AppTooltip>
              </li>
            );
          })}
        </ul>
          </>
        )}
      </Accordion.Body>
    </Accordion.Item>
  );
}

export default ScenesAccordion;
