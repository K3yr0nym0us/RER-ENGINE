import { Accordion } from 'react-bootstrap';
import { CodeSlash, Diagram3, FileEarmarkCode, Plus } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import { useTraslate } from '@hooks';
import { useContextEngine } from '@engine';
import { useSceneManager } from '../../../hooks/useSceneManager';

export function ProgrammingAccordion() {
  const { t } = useTraslate();
  const { engineReady } = useContextEngine();
  const { openSceneScriptEditor, openVisualScriptingModal, sceneActionsDisabled } = useSceneManager();

  return (
    <Accordion.Item eventKey="scenes-programming">
      <Accordion.Header><CodeSlash className="me-2" />{t('Programming')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2 d-flex flex-column gap-2">
        <AppTooltip content={t('Scene script')} place="top">
          <span className="d-block">
            <button
              type="button"
              className="btn btn-sm btn-outline-warning w-100 d-flex align-items-center justify-content-center gap-1"
              onClick={() => openSceneScriptEditor()}
              disabled={!engineReady || sceneActionsDisabled}
            >
              <Plus size={14} />
              <FileEarmarkCode size={14} />
              {t('Scene script')}
            </button>
          </span>
        </AppTooltip>

        <AppTooltip content={t('Scene logic (nodes)')} place="top">
          <span className="d-block">
            <button
              type="button"
              className="btn btn-sm btn-outline-info w-100 d-flex align-items-center justify-content-center gap-1"
              onClick={() => openVisualScriptingModal()}
              disabled={!engineReady || sceneActionsDisabled}
            >
              <Diagram3 size={14} />
              {t('Scene logic (nodes)')}
            </button>
          </span>
        </AppTooltip>
      </Accordion.Body>
    </Accordion.Item>
  );
}

export default ProgrammingAccordion;
