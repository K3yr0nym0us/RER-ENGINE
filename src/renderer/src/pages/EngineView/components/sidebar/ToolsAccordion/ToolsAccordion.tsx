import { useState } from 'react'


import { Accordion } from 'react-bootstrap';
import { Wrench } from 'react-bootstrap-icons';

import { ColliderToolButton } from './components/ColliderToolButton';
import { SocketConfigToolButton } from './components/SocketConfigToolButton';
import { ExecutionAreaToolButton } from './components/ExecutionAreaToolButton';
import { QuickBuildToolButton } from './components/QuickBuildToolButton';
import { useTraslate } from '@hooks';
import type { ProjectType } from '@shared-types';

interface Props {
  projectType: ProjectType;
}

export function ToolsAccordion({ projectType }: Props) {
  const { t } = useTraslate();
  const [activePointTool, setActivePointTool] = useState<'draw_collider' | 'draw_execution_area' | null>(null)

  return (
    <Accordion.Item eventKey="herramientas">
      <Accordion.Header><Wrench className="me-2" />{t('Tools')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <div className="d-flex flex-wrap gap-1 justify-content-between">
          <ColliderToolButton
            projectType={projectType}
            activeTool={activePointTool}
            setActiveTool={setActivePointTool}
          />
          <ExecutionAreaToolButton
            projectType={projectType}
            activeTool={activePointTool}
            setActiveTool={setActivePointTool}
          />
          <QuickBuildToolButton />
          <SocketConfigToolButton projectType={projectType} />
        </div>
      </Accordion.Body>
    </Accordion.Item>
  )
}

export default ToolsAccordion
