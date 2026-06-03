import type { ModalElectronOpenRequest } from '@shared-types'
import { BluePrintModalElectronHost } from './BluePrintModalElectronHost'
import { PlayerUiEditorElectronHost } from './PlayerUiEditorElectronHost'
import { buildModalElectronHostProps, MODAL_ELECTRON_REGISTRY } from './modalElectronRegistry'

interface ModalElectronGenericBodyProps {
	payload: ModalElectronOpenRequest
	onClose: () => void
}

export function ModalElectronGenericBody({ payload, onClose }: ModalElectronGenericBodyProps) {
	if (payload.componentKey === 'BluePrintModalBody') {
		return <BluePrintModalElectronHost payload={payload} />
	}

	if (payload.componentKey === 'PlayerUiEditorModalBody') {
		return <PlayerUiEditorElectronHost payload={payload} />
	}

	const Component = MODAL_ELECTRON_REGISTRY[payload.componentKey]
	if (!Component) {
		return (
			<p className="text-danger small mb-0">
				Componente modal no soportado: {payload.componentKey}
			</p>
		)
	}

	const hostProps = buildModalElectronHostProps(payload, onClose)

	return <Component {...hostProps} />
}
