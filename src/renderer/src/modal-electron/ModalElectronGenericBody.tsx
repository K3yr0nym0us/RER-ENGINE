import type { ModalElectronOpenRequest } from '@shared-types'
import { BluePrintModalElectronHost } from './BluePrintModalElectronHost'
import { EntityPropertiesElectronHost } from './EntityPropertiesElectronHost'
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

	if (payload.componentKey === 'EntityPropertiesModalBody') {
		return <EntityPropertiesElectronHost payload={payload} />
	}

	const Component = MODAL_ELECTRON_REGISTRY[payload.componentKey]
	if (!Component) {
		return (
			<p className="text-danger small mb-0">
				Componente modal no soportado: <strong>{payload.componentKey}</strong>.
				{' '}Regístralo en <code>modalElectronRegistry.tsx</code> (ver{' '}
				<code>docs/MODAL_ELECTRON.yaml</code>).
			</p>
		)
	}

	const hostProps = buildModalElectronHostProps(payload, onClose)

	return <Component {...hostProps} />
}
