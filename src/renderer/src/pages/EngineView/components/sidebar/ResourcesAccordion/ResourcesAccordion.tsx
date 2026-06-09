import { Accordion } from 'react-bootstrap';
import { BoxSeam } from 'react-bootstrap-icons';

import SpritesAccordion from './SpritesAccordion/SpritesAccordion';
import ModelsAccordion from './ModelsAccordion/ModelsAccordion';
import BackgroundsAccordion from './BackgroundsAccordion/BackgroundsAccordion';
import SoundsAccordion from './SoundsAccordion/SoundsAccordion';
import FontsAccordion from './FontsAccordion/FontsAccordion';
import ImagesAccordion from './ImagesAccordion/ImagesAccordion';
import SidebarSubAccordion from '../SidebarSubAccordion';

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
      <Accordion.Header data-plugin-target="accordion-resources">
        <BoxSeam className="me-2" />{t('Resources')}
      </Accordion.Header>
      <Accordion.Body className="py-2 px-1">
        <SidebarSubAccordion>
          {is3d ? <ModelsAccordion /> : <SpritesAccordion />}
          <SoundsAccordion />
          <FontsAccordion />
          <ImagesAccordion />
          {!is3d && <BackgroundsAccordion />}
        </SidebarSubAccordion>
      </Accordion.Body>
    </Accordion.Item>
  );
};

export default ResourcesAccordion;
