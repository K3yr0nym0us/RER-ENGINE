import { Escape } from 'react-bootstrap-icons';
import { useTraslate } from '@hooks';
import UiScreensAccordion from '../components/UiScreensAccordion';

const UiMenuAccordion = () => {
	const { t } = useTraslate();

	return (
		<UiScreensAccordion
			scope="menu"
			eventKey="ui-menu"
			headerIcon={<Escape className="me-2" />}
			headerTitle={t('UI Menu')}
			defaultNamePrefix={t('UI Menu')}
		/>
	);
};

export default UiMenuAccordion;
