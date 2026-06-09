import { Accordion } from 'react-bootstrap';
import { CameraAccordion, ControlsAccordion, ScenesAccordion, WorldAccordion, ToolsAccordion } from './sidebar';
import ResourcesAccordion from './sidebar/ResourcesAccordion/ResourcesAccordion';
import EntitiesAccordion from './sidebar/EntitiesAccordion/EntitiesAccordion';
import UIAccordion from './sidebar/UIAccordion/UIAccordion';
import UserGuideButton from './sidebar/UserGuideButton';
import PluginsButton from '../../../plugins/PluginsButton';
import { LanguageToggleButton } from '@components';
import { useSidebarAccordion } from '../../../context/SidebarAccordionContext';

import type { GameStyle, ProjectType } from '@shared-types';

export function SideBarLeft({ projectType, gameStyle }: { projectType: ProjectType; gameStyle?: GameStyle }) {
  const sidebarAccordion = useSidebarAccordion();

  return (
    <aside className="app-sidebar px-1 py-2 border-end border-secondary-subtle d-flex flex-column">
      <div className="flex-grow-1 sidebar-scroll">
        <Accordion {...sidebarAccordion.propsFor('scenes')}>
          <ScenesAccordion />
        </Accordion>

        {(projectType === '2D' || projectType === '3D') && (
          <>
            <Accordion {...sidebarAccordion.propsFor('mundo')}>
              <WorldAccordion projectType={projectType} />
            </Accordion>

            <Accordion {...sidebarAccordion.propsFor('camera')}>
              <CameraAccordion projectType={projectType} gameStyle={gameStyle} />
            </Accordion>

            <Accordion {...sidebarAccordion.propsFor('resources')}>
              <ResourcesAccordion projectType={projectType} />
            </Accordion>
          </>
        )}

        <Accordion {...sidebarAccordion.propsFor('entities')}>
          <EntitiesAccordion projectType={projectType} />
        </Accordion>
        <Accordion {...sidebarAccordion.propsFor('ui')}>
          <UIAccordion />
        </Accordion>
        <Accordion {...sidebarAccordion.propsFor('herramientas')}>
          <ToolsAccordion projectType={projectType} />
        </Accordion>

        <Accordion {...sidebarAccordion.propsFor('controles')}>
          <ControlsAccordion />
        </Accordion>

      </div>

      <div className="pt-3 mt-2 border-top border-secondary-subtle d-flex flex-column gap-2">
        <PluginsButton />
        <div className="d-flex gap-2">
          <UserGuideButton />
          <LanguageToggleButton variant="sidebar" />
        </div>
      </div>
    </aside>
  );
}

export default SideBarLeft;
