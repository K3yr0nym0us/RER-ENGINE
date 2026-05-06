import { Accordion } from 'react-bootstrap';

import { EnvironmentsAccordion } from './EnvironmentsAccordion/EnvironmentsAccordion';
import BtnCreateCharacter from './CharactersAccordion/components/BtnCreateCharacter';
import ObjectsAccordion from './ObjectsAccordion/ObjectsAccordion';
import { useTraslate } from '@hooks';

export default function EntitiesAccordeon({ projectType, engineReady, loadModel }: any) {
  const { t } = useTraslate();
  return (
    <Accordion.Item eventKey="entities">
      <Accordion.Header>{t('Entities')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <Accordion>
          <Accordion.Item eventKey="escenarios">
            <Accordion.Header>{t('Environment')}</Accordion.Header>
            <Accordion.Body className="py-2 px-2">
              {projectType === '3D' && (
                <button
                  className="btn btn-outline-light btn-sm w-100 fw-bold"
                  disabled={!engineReady}
                  onClick={() =>
                    window.electronAPI.openModelDialog().then((p: string | null) => { if (p) loadModel(p) })
                  }
                >
                  {t('Load model (.glb)')}
                </button>
              )}
              {projectType === '2D' && (
                <EnvironmentsAccordion
                  config={{
                    openDialog: () => window.electronAPI.openScenarioDialog(),
                    loadCmd: 'load_scenario',
                    dupCmd: 'duplicate_scenario',
                    addBtnLabel: t('+ Add scenario (PNG)'),
                    emptyText: t('No scenarios loaded'),
                  }} 
                />
              )}
            </Accordion.Body>
          </Accordion.Item>
          <Accordion.Item eventKey="personajes">
            <Accordion.Header>{t('Characters')}</Accordion.Header>
            <Accordion.Body className="py-2 px-2">
              {projectType === '2D' && (
                <BtnCreateCharacter />
              )}
            </Accordion.Body>
          </Accordion.Item>
          <Accordion.Item eventKey="objetos">
            <Accordion.Header>{t('Objects')}</Accordion.Header>
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
