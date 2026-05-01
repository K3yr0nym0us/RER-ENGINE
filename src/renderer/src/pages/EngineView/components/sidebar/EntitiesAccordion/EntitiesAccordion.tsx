import { Accordion } from 'react-bootstrap';

import ScenariosAccordion, { EnvironmentsAccordion } from './EnvironmentsAccordion/EnvironmentsAccordion';
import BtnCreateCharacter from './CharactersAccordion/components/BtnCreateCharacter';
import ObjectsAccordion from './ObjectsAccordion/ObjectsAccordion';

export default function EntitiesAccordeon({ projectType, engineReady, loadModel }: any) {
  return (
    <Accordion.Item eventKey="entities">
      <Accordion.Header>Entidades</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <Accordion>
          <Accordion.Item eventKey="escenarios">
            <Accordion.Header>Entorno</Accordion.Header>
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
              {projectType === '2D' && (
                <EnvironmentsAccordion
                  config={{
                    openDialog: () => window.electronAPI.openScenarioDialog(),
                    loadCmd: 'load_scenario',
                    dupCmd: 'duplicate_scenario',
                    addBtnLabel: '+ Agregar escenario (PNG)',
                    emptyText: 'Sin escenarios cargados',
                  }} 
                />
              )}
            </Accordion.Body>
          </Accordion.Item>
          <Accordion.Item eventKey="personajes">
            <Accordion.Header>Personajes</Accordion.Header>
            <Accordion.Body className="py-2 px-2">
              {projectType === '2D' && (
                <BtnCreateCharacter />
              )}
            </Accordion.Body>
          </Accordion.Item>
          <Accordion.Item eventKey="objetos">
            <Accordion.Header>Objetos</Accordion.Header>
            <Accordion.Body className="py-2 px-2">
              {projectType === '2D' && (
                <ObjectsAccordion />
              )}
            </Accordion.Body>
          </Accordion.Item>
        </Accordion>
      </Accordion.Body>
    </Accordion.Item>
  )
}
