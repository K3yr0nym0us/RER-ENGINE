import { Accordion } from 'react-bootstrap';

import { useContextEngine } from '../context/useContextEngine';
import PropertiesPanel from './PropertiesPanel';
import { WorldAccordion, ScenariosAccordion, CharactersAccordion, ToolsAccordion } from './sidebar';

import { ProjectType } from '../../../shared-types/types';

export function SideBarLeft({ projectType }: { projectType: ProjectType }) {
  const {
    engineReady,
    selectedEntity,
    loadModel,
  } = useContextEngine()

  const isCollider = useContextEngine().colliderEntities.some((c: any) => c.id === selectedEntity?.id)

  return (
    <aside className="app-sidebar p-3 border-end border-secondary-subtle overflow-auto">
      <Accordion defaultActiveKey="mundo" className="sidebar-accordion">

        {projectType === '2D' && <WorldAccordion />}

        <Accordion.Item eventKey="assets">
          <Accordion.Header>Assets</Accordion.Header>
          <Accordion.Body className="py-2 px-2">
            <Accordion>
              <Accordion.Item eventKey="escenarios">
                <Accordion.Header>Escenarios</Accordion.Header>
                <Accordion.Body className="py-2 px-2">
                  {projectType === '3D' && (
                    <button
                      className="btn btn-outline-light btn-sm w-100 fw-bold"
                      disabled={!engineReady}
                      onClick={() =>
                        window.electronAPI.openModelDialog().then((p: string | null) => { if (p) loadModel(p) })
                      }
                    >
                      Cargar modelo (.glb)
                    </button>
                  )}
                  {projectType === '2D' && <ScenariosAccordion config={{
                    openDialog: () => window.electronAPI.openScenarioDialog(),
                    loadCmd: 'load_scenario',
                    dupCmd: 'duplicate_scenario',
                    addBtnLabel: '+ Agregar escenario (PNG)',
                    emptyText: 'Sin escenarios cargados',
                  }} />}
                </Accordion.Body>
              </Accordion.Item>
              <Accordion.Item eventKey="personajes">
                <Accordion.Header>Personajes</Accordion.Header>
                <Accordion.Body className="py-2 px-2">
                  {projectType === '2D' && <CharactersAccordion config={{
                    openDialog: () => window.electronAPI.openCharacterDialog(),
                    loadCmd: 'load_character',
                    dupCmd: 'duplicate_character',
                    addBtnLabel: '+ Agregar personaje (PNG)',
                    emptyText: 'Sin personajes cargados',
                  }} />}
                </Accordion.Body>
              </Accordion.Item>
            </Accordion>
          </Accordion.Body>
        </Accordion.Item>

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