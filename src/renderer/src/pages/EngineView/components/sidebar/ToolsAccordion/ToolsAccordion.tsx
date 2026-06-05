import { useState } from 'react';


import { Accordion } from 'react-bootstrap';
import { Wrench } from 'react-bootstrap-icons';

import { ColliderToolButton } from './components/ColliderToolButton';
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
  const is2D = projectType === '2D'

  return (
    <Accordion.Item eventKey="herramientas">
      <Accordion.Header><Wrench className="me-2" />{t('Tools')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <div className={`d-flex flex-wrap gap-1 ${is2D ? 'justify-content-between' : 'justify-content-center'}`}>
          {is2D && (
            <>
              <ColliderToolButton activeTool={activePointTool} setActiveTool={setActivePointTool} />
              <ExecutionAreaToolButton activeTool={activePointTool} setActiveTool={setActivePointTool} />
            </>
          )}
          <QuickBuildToolButton />
        </div>
      </Accordion.Body>
    </Accordion.Item>
  )
}

export default ToolsAccordion
