import { Spinner } from 'react-bootstrap'

import { useTraslate } from '@hooks'

/** Cuerpo de la modal Electron mientras se empaqueta y guarda el proyecto (.save). */
export function ProjectSaveBlockingModalBody() {
	const { t } = useTraslate()

	return (
		<div
			className="d-flex flex-column align-items-center gap-3 py-2 text-center"
			aria-busy="true"
			aria-live="polite"
		>
			<Spinner animation="border" variant="primary" role="status">
				<span className="visually-hidden">{t('Saving project…')}</span>
			</Spinner>
			<span className="text-secondary user-select-none mb-0">{t('Saving project…')}</span>
		</div>
	)
}

ProjectSaveBlockingModalBody.displayName = 'ProjectSaveBlockingModalBody'
