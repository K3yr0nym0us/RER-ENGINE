
import { Accordion } from 'react-bootstrap';
import { BoxSeam } from 'react-bootstrap-icons';

import SpritesAccordion from './SpritesAccordion/SpritesAccordion';
import BackgroundsAccordion from './BackgroundsAccordion/BackgroundsAccordion';
import SoundsAccordion from './SoundsAccordion/SoundsAccordion';

import { useTraslate } from '@hooks';

const ResourcesAccordion = () => {
  const { t } = useTraslate();

  return (
    <Accordion.Item eventKey="resources">
      <Accordion.Header><BoxSeam className="me-2" />{t('Resources')}</Accordion.Header>
      <Accordion.Body className="py-2 px-1">
        <Accordion className="sidebar-accordion">
          <SpritesAccordion />
        </Accordion>
        <Accordion className="sidebar-accordion mt-2">
          <SoundsAccordion />
        </Accordion>
        <Accordion className="sidebar-accordion mt-2">
          <BackgroundsAccordion />
        </Accordion>
      </Accordion.Body>
    </Accordion.Item>
  );
};

export default ResourcesAccordion;
