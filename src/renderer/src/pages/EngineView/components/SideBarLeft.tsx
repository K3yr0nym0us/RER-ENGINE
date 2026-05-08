import { Accordion } from 'react-bootstrap';
import PropertiesPanel from './sidebar/PropertiesAccordion/PropertiesAccordion';
import { ControlsAccordion, WorldAccordion, ToolsAccordion } from './sidebar';
import ResourcesAccordion from './sidebar/ResourcesAccordion/ResourcesAccordion';
import EntitiesAccordion from './sidebar/EntitiesAccordion/EntitiesAccordion';
import UserGuideButton from './sidebar/UserGuideButton';

import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';

import { ProjectType } from '@shared-types';

export function SideBarLeft({ projectType }: { projectType: ProjectType }) {
  const { t } = useTraslate();
  const {
    engineReady,
    selectedEntity,
    loadModel,
  } = useContextEngine()

  return (
    <aside className="app-sidebar px-2 py-2 border-end border-secondary-subtle d-flex flex-column">
      <div className="flex-grow-1 sidebar-scroll">
        {projectType === '2D' && (
          <>
            <Accordion className="sidebar-accordion">
              <WorldAccordion />
            </Accordion>

            <Accordion className="sidebar-accordion mt-1">
              <ResourcesAccordion />
            </Accordion>
          </>
        )}

        <Accordion className="sidebar-accordion mt-1">
          <EntitiesAccordion
            projectType={projectType}
            engineReady={engineReady}
            loadModel={loadModel}
          />
        </Accordion>

        <Accordion className="sidebar-accordion mt-1">
          <ToolsAccordion />
        </Accordion>

        <Accordion className="sidebar-accordion mt-1">
          <ControlsAccordion />
        </Accordion>

        {selectedEntity && (
          <div className="pt-2">
            <b className="ms-2">{t('Selected element:')}</b>
            <Accordion defaultActiveKey="propiedades" className="sidebar-accordion mt-1">
              <Accordion.Item eventKey="propiedades">
                <Accordion.Header>{t('Properties')}</Accordion.Header>
                <Accordion.Body className="py-2 px-1">
                  <PropertiesPanel
                    projectType={projectType}
                  />
                </Accordion.Body>
              </Accordion.Item>
            </Accordion>
          </div>
        )}
      </div>

      <div className="pt-3 mt-2 border-top border-secondary-subtle">
        <UserGuideButton />
      </div>
    </aside>
  )
}

export default SideBarLeft;
