import { Accordion } from 'react-bootstrap';
import { BoxArrowInRight, Collection, Pencil, PlusLg, Trash } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import { useTraslate } from '@hooks';
import { useSceneManager } from '../../hooks/useSceneManager';

export function ScenesAccordion() {
  const { t } = useTraslate();
  const {
    scenes,
    activeSceneId,
    loadScene,
    openCreateSceneModal,
    openRenameSceneModal,
    openDeleteSceneModal,
  } = useSceneManager();

  return (
    <Accordion.Item eventKey="scenes">
      <Accordion.Header><Collection className="me-2" />{t('Scenes')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <button
          className="btn btn-sm btn-outline-secondary w-100 d-flex align-items-center justify-content-center gap-1 mb-2"
          type="button"
          onClick={openCreateSceneModal}
        >
          <PlusLg size={12} />
          {t('Create new scene')}
        </button>

        <ul className="list-unstyled mb-0 scenes-accordion-list">
          {scenes.map((scene) => {
            const isActive = scene.id === activeSceneId;
            return (
              <li
                key={scene.id}
                className={`scenes-accordion-item d-flex align-items-center gap-1 mb-1${isActive ? ' scenes-accordion-item--active' : ''}`}
              >
                <span className="scenes-accordion-item__name flex-grow-1 text-truncate" title={scene.name}>
                  {scene.name}
                </span>
                {!isActive && (
                  <AppTooltip content={t('Load scene')} place="top">
                    <span className="d-inline-flex scenes-accordion-icon-wrap">
                      <button
                        className="btn btn-sm btn-link scenes-accordion-icon-btn text-info p-0"
                        type="button"
                        onClick={() => {
                          void loadScene(scene.id);
                        }}
                        aria-label={`${t('Load scene')} ${scene.name}`}
                      >
                        <BoxArrowInRight size={14} />
                      </button>
                    </span>
                  </AppTooltip>
                )}
                <AppTooltip content={t('Edit scene name')} place="top">
                  <span className="d-inline-flex scenes-accordion-icon-wrap">
                    <button
                      className="btn btn-sm btn-link scenes-accordion-icon-btn text-primary p-0"
                      type="button"
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
      </Accordion.Body>
    </Accordion.Item>
  );
}

export default ScenesAccordion;
