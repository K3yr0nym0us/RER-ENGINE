import { Accordion } from 'react-bootstrap';
import { Gear } from 'react-bootstrap-icons';
import PropertiesPanel from './sidebar/PropertiesAccordion/PropertiesAccordion';
import { CameraAccordion, ControlsAccordion, ScenesAccordion, WorldAccordion, ToolsAccordion } from './sidebar';
import ResourcesAccordion from './sidebar/ResourcesAccordion/ResourcesAccordion';
import EntitiesAccordion from './sidebar/EntitiesAccordion/EntitiesAccordion';
import UserGuideButton from './sidebar/UserGuideButton';
import { LanguageToggleButton } from '@components';

import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';

import type { GameStyle, ProjectType } from '@shared-types';

export function SideBarLeft({ projectType, gameStyle }: { projectType: ProjectType; gameStyle?: GameStyle }) {
  const { t } = useTraslate();
  const { selectedEntity } = useContextEngine();

  return (
    <aside className="app-sidebar px-1 py-2 border-end border-secondary-subtle d-flex flex-column">
      <div className="flex-grow-1 sidebar-scroll">
        <Accordion defaultActiveKey="scenes" className="sidebar-accordion">
          <ScenesAccordion />
        </Accordion>

        {(projectType === '2D' || projectType === '3D') && (
          <>
            <Accordion className="sidebar-accordion">
              <WorldAccordion projectType={projectType} />
            </Accordion>

            <Accordion className="sidebar-accordion">
              <CameraAccordion projectType={projectType} gameStyle={gameStyle} />
            </Accordion>

            <Accordion className="sidebar-accordion">
              <ResourcesAccordion projectType={projectType} />
            </Accordion>
          </>
        )}

        <Accordion className="sidebar-accordion">
          <EntitiesAccordion projectType={projectType} />
        </Accordion>

        {projectType !== '3D' && (
          <Accordion className="sidebar-accordion">
            <ToolsAccordion />
          </Accordion>
        )}

        <Accordion className="sidebar-accordion">
          <ControlsAccordion />
        </Accordion>

        {selectedEntity && (
          <div className="pt-1">
            <b className="ms-1">{t('Selected element:')}</b>
            <Accordion defaultActiveKey="propiedades" className="sidebar-accordion">
              <Accordion.Item eventKey="propiedades">
                <Accordion.Header><Gear className="me-2" />{t('Properties')}</Accordion.Header>
                <Accordion.Body className="py-2 px-1">
                  <PropertiesPanel projectType={projectType} />
                </Accordion.Body>
              </Accordion.Item>
            </Accordion>
          </div>
        )}
      </div>

      <div className="pt-3 mt-2 border-top border-secondary-subtle d-flex gap-2">
        <UserGuideButton />
        <LanguageToggleButton variant="sidebar" />
      </div>
    </aside>
  );
}

export default SideBarLeft;
