import { Accordion } from 'react-bootstrap';
import PropertiesPanel from './sidebar/PropertiesAccordion/PropertiesAccordion';
import { WorldAccordion, ToolsAccordion } from './sidebar';
import SpritesAccordion from './sidebar/SpritesAccordion/SpritesAccordion';
import EntitiesAccordion from './sidebar/EntitiesAccordion/EntitiesAccordion';

import { useContextEngine } from '../../../context/useContextEngine';
import { ProjectType } from '../../../../../shared-types/types';

export function SideBarLeft({ projectType }: { projectType: ProjectType }) {
  const {
    engineReady,
    selectedEntity,
    loadModel,
  } = useContextEngine()

  const isCollider = useContextEngine().colliderEntities.some((c: any) => c.id === selectedEntity?.id)

  return (
    <aside className="app-sidebar p-3 border-end border-secondary-subtle overflow-auto">
      {projectType === '2D' && (
        <>
          <Accordion className="sidebar-accordion">
            <WorldAccordion />
          </Accordion>

          <Accordion className="sidebar-accordion mt-3">
            <SpritesAccordion />
          </Accordion>
        </>
      )}

      <Accordion className="sidebar-accordion mt-3">
        <EntitiesAccordion
          projectType={projectType}
          engineReady={engineReady}
          loadModel={loadModel}
        />
      </Accordion>

      <Accordion className="sidebar-accordion mt-3">
        <ToolsAccordion />
      </Accordion>

      {selectedEntity && !isCollider && (
        <div className="pt-4">
          <b className="ms-2">Elemento seleccionado:</b>
          <Accordion defaultActiveKey="propiedades" className="sidebar-accordion mt-1">
            <Accordion.Item eventKey="propiedades">
              <Accordion.Header>Propiedades</Accordion.Header>
              <Accordion.Body className="py-2 px-1">
                <PropertiesPanel
                  projectType={projectType}
                />
              </Accordion.Body>
            </Accordion.Item>
          </Accordion>
        </div>
      )}
    </aside>
  )
}

export default SideBarLeft
