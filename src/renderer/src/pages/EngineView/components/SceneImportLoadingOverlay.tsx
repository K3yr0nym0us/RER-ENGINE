import { Spinner } from 'react-bootstrap';

import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';

export function SceneImportLoadingOverlay() {
	const { sceneImportLoading } = useContextEngine();
	const { t } = useTraslate();

	if (!sceneImportLoading) return null;

	return (
		<div
			className="position-absolute top-0 start-0 w-100 h-100 d-flex flex-column align-items-center justify-content-center gap-3"
			style={{ zIndex: 25, backgroundColor: 'var(--bs-body-bg)' }}
			aria-busy="true"
			aria-live="polite"
		>
			<Spinner animation="border" variant="primary" role="status">
				<span className="visually-hidden">{t('Loading scene…')}</span>
			</Spinner>
			<span className="text-secondary user-select-none">{t('Loading scene…')}</span>
		</div>
	);
}
