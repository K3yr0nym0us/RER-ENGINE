import { useState } from 'react';
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

import type { AccordionEventKey } from 'react-bootstrap/esm/Accordion';
import type { GameStyle, ProjectType } from '@shared-types';

function useExclusiveSidebarAccordion(defaultKey: string) {
  const [activeKey, setActiveKey] = useState<string | null>(defaultKey);

  const propsFor = (key: string) => ({
    activeKey: activeKey === key ? key : undefined,
    onSelect: (next: AccordionEventKey) => {
      setActiveKey(typeof next === 'string' ? next : null);
    },
    className: 'sidebar-accordion' as const,
  });

  return propsFor;
}

export function SideBarLeft({ projectType, gameStyle }: { projectType: ProjectType; gameStyle?: GameStyle }) {
  const { t } = useTraslate();
  const { selectedEntity } = useContextEngine();
  const sidebarAccordion = useExclusiveSidebarAccordion('scenes');

  return (
    <aside className="app-sidebar px-1 py-2 border-end border-secondary-subtle d-flex flex-column">
      <div className="flex-grow-1 sidebar-scroll">
        <Accordion {...sidebarAccordion('scenes')}>
          <ScenesAccordion />
        </Accordion>

        {(projectType === '2D' || projectType === '3D') && (
          <>
            <Accordion {...sidebarAccordion('mundo')}>
              <WorldAccordion projectType={projectType} />
            </Accordion>

            <Accordion {...sidebarAccordion('camera')}>
              <CameraAccordion projectType={projectType} gameStyle={gameStyle} />
            </Accordion>

            <Accordion {...sidebarAccordion('resources')}>
              <ResourcesAccordion projectType={projectType} />
            </Accordion>
          </>
        )}

        <Accordion {...sidebarAccordion('entities')}>
          <EntitiesAccordion projectType={projectType} />
        </Accordion>

        <Accordion {...sidebarAccordion('herramientas')}>
          <ToolsAccordion projectType={projectType} />
        </Accordion>

        <Accordion {...sidebarAccordion('controles')}>
          <ControlsAccordion />
        </Accordion>

        {selectedEntity && (
          <div className="pt-1">
            <b className="ms-1">{t('Selected element:')}</b>
            <Accordion {...sidebarAccordion('propiedades')}>
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
