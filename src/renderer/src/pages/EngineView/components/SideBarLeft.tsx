import { useState } from 'react';

import { Accordion } from 'react-bootstrap';
import PropertiesPanel from './sidebar/PropertiesAccordion/PropertiesAccordion';
import { WorldAccordion, ToolsAccordion } from './sidebar';
import SpritesAccordion, { Sprite } from './sidebar/SpritesAccordion/SpritesAccordion';
import EntitiesAccordeon from './sidebar/EntitiesAccordion/EntitiesAccordion';

import { useContextEngine } from '../../../context/useContextEngine';
import { ProjectType } from '../../../../../shared-types/types';

export function SideBarLeft({ projectType }: { projectType: ProjectType }) {
  // State de sprites global para la barra lateral
  const [sprites, setSprites] = useState<Sprite[]>([]);

  const {
    engineReady,
    selectedEntity,
    loadModel,
  } = useContextEngine()

  const isCollider = useContextEngine().colliderEntities.some((c: any) => c.id === selectedEntity?.id)

  const handleAddSprite = (sprite: Omit<Sprite, 'id'>) => {
    const id = `sprite_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    setSprites(prev => [...prev, { ...sprite, id }]);
  };


  return (
    <aside className="app-sidebar p-3 border-end border-secondary-subtle overflow-auto">
      {projectType === '2D' && (
        <>
          <Accordion className="sidebar-accordion">
            <WorldAccordion />
          </Accordion>

          <Accordion className="sidebar-accordion mt-3">
            <SpritesAccordion
              sprites={sprites}
              onAddSprite={handleAddSprite}
            />
          </Accordion>
        </>
      )}

      <Accordion className="sidebar-accordion mt-3">
        <EntitiesAccordeon
          projectType={projectType}
          engineReady={engineReady}
          loadModel={loadModel}
          sprites={sprites}
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