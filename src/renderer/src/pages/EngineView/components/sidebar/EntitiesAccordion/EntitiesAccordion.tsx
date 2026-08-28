import { Accordion } from 'react-bootstrap';
import { PeopleFill, TreeFill, PersonFill, Box, ShieldFill, RecordCircleFill } from 'react-bootstrap-icons';

import { EnvironmentsAccordion } from './EnvironmentsAccordion/EnvironmentsAccordion';
import BtnCreateEntityFromModel from './components/BtnCreateEntityFromModel';
import BtnCreateCharacter from './CharactersAccordion/components/BtnCreateCharacter';
import ObjectsAccordion from './ObjectsAccordion/ObjectsAccordion';
import ProjectilesAccordion from './ProjectilesAccordion/ProjectilesAccordion';
import SidebarSubAccordion from '../SidebarSubAccordion';
import { useTraslate } from '@hooks';

export default function EntitiesAccordeon({ projectType }: { projectType?: string }) {
  const { t } = useTraslate();
  return (
    <Accordion.Item eventKey="entities">
      <Accordion.Header><PeopleFill className="me-2" />{t('Create entity')}</Accordion.Header>
      <Accordion.Body className="py-2 px-1">
        <SidebarSubAccordion>
          <Accordion.Item eventKey="personajes">
            <Accordion.Header><PersonFill className="me-2" />{t('Characters')}</Accordion.Header>
            <Accordion.Body className="py-2 px-2">
              {projectType === '3D' && <BtnCreateEntityFromModel intent="character" />}
              {projectType === '2D' && <BtnCreateCharacter />}
            </Accordion.Body>
          </Accordion.Item>
          <Accordion.Item eventKey="escenarios">
            <Accordion.Header><TreeFill className="me-2" />{t('Environment')}</Accordion.Header>
            <Accordion.Body className="py-2 px-2">
              {projectType === '3D' && <BtnCreateEntityFromModel intent="environment" />}
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
          <Accordion.Item eventKey="objetos">
            <Accordion.Header><Box className="me-2" />{t('Objects')}</Accordion.Header>
            <Accordion.Body className="py-2 px-2">
              {projectType === '3D' && <BtnCreateEntityFromModel intent="object" />}
              {projectType === '2D' && <ObjectsAccordion />}
            </Accordion.Body>
          </Accordion.Item>
          {projectType === '3D' && (
            <Accordion.Item eventKey="armas">
              <Accordion.Header><ShieldFill className="me-2" />{t('Weapons')}</Accordion.Header>
              <Accordion.Body className="py-2 px-2">
                <BtnCreateEntityFromModel intent="weapon" />
              </Accordion.Body>
            </Accordion.Item>
          )}
          {projectType === '3D' && (
            <Accordion.Item eventKey="proyectiles">
              <Accordion.Header><RecordCircleFill className="me-2" />{t('Projectiles')}</Accordion.Header>
              <Accordion.Body className="py-2 px-2">
                <BtnCreateEntityFromModel intent="projectile" />
              </Accordion.Body>
            </Accordion.Item>
          )}
          {projectType === '2D' && (
            <Accordion.Item eventKey="proyectiles">
              <Accordion.Header><RecordCircleFill className="me-2" />{t('Projectiles')}</Accordion.Header>
              <Accordion.Body className="py-2 px-2">
                <ProjectilesAccordion />
              </Accordion.Body>
            </Accordion.Item>
          )}
        </SidebarSubAccordion>
      </Accordion.Body>
    </Accordion.Item>
  )
}
