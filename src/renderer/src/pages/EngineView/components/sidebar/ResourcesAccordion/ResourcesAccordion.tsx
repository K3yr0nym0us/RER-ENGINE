import { Accordion } from 'react-bootstrap';
import { BoxSeam } from 'react-bootstrap-icons';

import SpritesAccordion from './SpritesAccordion/SpritesAccordion';
import ModelsAccordion from './ModelsAccordion/ModelsAccordion';
import BackgroundsAccordion from './BackgroundsAccordion/BackgroundsAccordion';
import SoundsAccordion from './SoundsAccordion/SoundsAccordion';

import { useTraslate } from '@hooks';
import type { ProjectType } from '@shared-types';

interface Props {
  projectType?: ProjectType;
}

const ResourcesAccordion = ({ projectType = '2D' }: Props) => {
  const { t } = useTraslate();
  const is3d = projectType === '3D';

  return (
    <Accordion.Item eventKey="resources">
      <Accordion.Header><BoxSeam className="me-2" />{t('Resources')}</Accordion.Header>
      <Accordion.Body className="py-2 px-1">
        <Accordion className="sidebar-accordion">
          {is3d ? <ModelsAccordion /> : <SpritesAccordion />}
        </Accordion>
        <Accordion className="sidebar-accordion mt-2">
          <SoundsAccordion />
        </Accordion>
        {!is3d && (
          <Accordion className="sidebar-accordion mt-2">
            <BackgroundsAccordion />
          </Accordion>
        )}
      </Accordion.Body>
    </Accordion.Item>
  );
};

export default ResourcesAccordion;
