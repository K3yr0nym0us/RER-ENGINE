import { Accordion } from 'react-bootstrap';
import { LayoutTextWindow } from 'react-bootstrap-icons';
import { useTraslate } from '@hooks';
import PlayerUiAccordion from './PlayerUiAccordion/PlayerUiAccordion';
import UiMenuAccordion from './UiMenuAccordion/UiMenuAccordion';

const UIAccordion = () => {
  const { t } = useTraslate();

  return (
    <Accordion.Item eventKey="ui">
      <Accordion.Header><LayoutTextWindow className="me-2" />{t('User interface')}</Accordion.Header>
      <Accordion.Body className="py-2 px-1">
        <Accordion className="sidebar-accordion">
          <PlayerUiAccordion />
        </Accordion>
        <Accordion className="sidebar-accordion mt-2">
          <UiMenuAccordion />
        </Accordion>
      </Accordion.Body>
    </Accordion.Item>
  );
};

export default UIAccordion;
