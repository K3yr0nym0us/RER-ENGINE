import { Controller } from 'react-bootstrap-icons';
import { useTraslate } from '@hooks';
import UiScreensAccordion from '../components/UiScreensAccordion';

const PlayerUiAccordion = () => {
	const { t } = useTraslate();

	return (
		<UiScreensAccordion
			scope="player"
			eventKey="player-ui"
			headerIcon={<Controller className="me-2" />}
			headerTitle={t('Player UI')}
			defaultNamePrefix={t('Player UI')}
		/>
	);
};

export default PlayerUiAccordion;
