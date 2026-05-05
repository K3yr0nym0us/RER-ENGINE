import { useState } from 'react';

import { Accordion } from 'react-bootstrap';

import { ColliderToolButton } from './components/ColliderToolButton';
import { ExecutionAreaToolButton } from './components/ExecutionAreaToolButton';
import { QuickBuildToolButton } from './components/QuickBuildToolButton';

export function ToolsAccordion() {
  const [activePointTool, setActivePointTool] = useState<'draw_collider' | 'draw_execution_area' | null>(null)

  return (
    <Accordion.Item eventKey="herramientas">
      <Accordion.Header>Herramientas</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <div className="d-flex justify-content-between">
          <ColliderToolButton activeTool={activePointTool} setActiveTool={setActivePointTool} />
          <ExecutionAreaToolButton activeTool={activePointTool} setActiveTool={setActivePointTool} />
          <QuickBuildToolButton />
        </div>
      </Accordion.Body>
    </Accordion.Item>
  )
}

export default ToolsAccordion