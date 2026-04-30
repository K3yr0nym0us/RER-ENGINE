import { Accordion } from 'react-bootstrap';

import ScenariosAccordion, { EnvironmentsAccordion } from './EnvironmentsAccordion/EnvironmentsAccordion';
import BtnCreateCharacter from './CharactersAccordion/components/BtnCreateCharacter';
import CharactersAccordion from './CharactersAccordion/CharactersAccordion';

export default function EntitiesAccordeon({ projectType, engineReady, loadModel, sprites }: any) {
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
                <>
                  <BtnCreateCharacter 
                    sprites={sprites} 
                    openDialog={() => window.electronAPI.openCharacterDialog()} 
                  />
                  <CharactersAccordion 
                    config={{
                      openDialog: () => window.electronAPI.openCharacterDialog(),
                        loadCmd: 'load_character',
                        dupCmd: 'duplicate_character',
                        addBtnLabel: '+ Agregar personaje (PNG)',
                        emptyText: 'Sin personajes cargados',
                      }
                    } 
                  />
                </>
              )}
            </Accordion.Body>
          </Accordion.Item>
        </Accordion>
      </Accordion.Body>
    </Accordion.Item>
  )
}
