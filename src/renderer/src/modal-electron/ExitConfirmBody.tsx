import { ExclamationTriangleFill } from 'react-bootstrap-icons'

import { useTraslate } from '@hooks'

import { useModalClose } from './useModalClose'

type ExitAction = 'save' | 'discard' | 'cancel'

/**
 * Modal de salida del editor Rust.
 *
 * Se renderiza en la ventana modal Electron (React/Bootstrap), no con
 * `dialog.showMessageBox`, para que quede por encima del viewport nativo y
 * mantenga el diseño del resto de modales.
 */
export function ExitConfirmBody() {
	const { t } = useTraslate()
	const closeModal = useModalClose()

	const finish = (action: ExitAction) => {
		window.electronAPI.resolveEditorExit(action)
		closeModal()
	}

	return (
		<div className="text-center">
			<ExclamationTriangleFill size={40} className="text-warning mb-3" />

			<h6 className="mb-2">{t('Exit RER-ENGINE')}</h6>
			<p className="mb-1">{t('Do you want to save your progress before exiting?')}</p>
			<p className="text-secondary small mb-4">{t('Unsaved changes will be lost.')}</p>

			<div className="d-grid gap-2">
				<button
					type="button"
					className="btn btn-primary"
					onClick={() => finish('save')}
				>
					{t('Save and exit')}
				</button>
				<button
					type="button"
					className="btn btn-outline-danger"
					onClick={() => finish('discard')}
				>
					{t('Exit without saving')}
				</button>
				<button
					type="button"
					className="btn btn-secondary"
					onClick={() => finish('cancel')}
				>
					{t('Cancel')}
				</button>
			</div>
		</div>
	)
}

ExitConfirmBody.displayName = 'ExitConfirmBody'
