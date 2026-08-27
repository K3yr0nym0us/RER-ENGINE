import SidebarSubAccordion from './sidebar/SidebarSubAccordion';
import WorldWorkspaceAccordion from './sidebar/WorldAccordion/WorldWorkspaceAccordion';
import WorldBackgroundAccordion from './sidebar/WorldAccordion/WorldBackgroundAccordion';
import WorldGridAccordion from './sidebar/WorldAccordion/WorldGridAccordion';
import WorldLightingAccordion from './sidebar/WorldAccordion/WorldLightingAccordion';
import WorldTexturesAccordion from './sidebar/WorldAccordion/WorldTexturesAccordion';
import WorldReflectionsAccordion from './sidebar/WorldAccordion/WorldReflectionsAccordion';
import WorldShadowsAccordion from './sidebar/WorldAccordion/WorldShadowsAccordion';
import WorldMsaaAccordion from './sidebar/WorldAccordion/WorldMsaaAccordion';
import WorldTaaAccordion from './sidebar/WorldAccordion/WorldTaaAccordion';
import WorldPhysicsAccordion from './sidebar/WorldAccordion/WorldPhysicsAccordion';
import WorldPerformanceAccordion from './sidebar/WorldAccordion/WorldPerformanceAccordion';
import MetricsPanel from './MetricsPanel';

import type { ProjectType } from '@shared-types';

export function SidebarRight({
  projectType,
}: {
  projectType: ProjectType
}) {
  const is3dProject = projectType === '3D';

  return (
    <aside className="app-sidebar px-1 ps-2 pe-0 pb-2 border-start border-secondary-subtle d-flex flex-column">
      <div className="flex-grow-1 sidebar-scroll">

        <h6 className="text-secondary text-uppercase fw-bold text-center mt-3 mb-1 px-2">
          Configuración del mundo
        </h6>

        <SidebarSubAccordion>
          <WorldWorkspaceAccordion projectType={projectType} />
          {!is3dProject && <WorldBackgroundAccordion />}
          <WorldGridAccordion projectType={projectType} />
          {is3dProject && <WorldLightingAccordion />}
          {is3dProject && <WorldTexturesAccordion />}
          {is3dProject && <WorldReflectionsAccordion />}
          {is3dProject && <WorldShadowsAccordion />}
          {is3dProject && <WorldMsaaAccordion />}
          {is3dProject && <WorldTaaAccordion />}
          <WorldPhysicsAccordion />
          <WorldPerformanceAccordion />
        </SidebarSubAccordion>
      </div>

      <MetricsPanel />
    </aside>
  );
}

export default SidebarRight;
