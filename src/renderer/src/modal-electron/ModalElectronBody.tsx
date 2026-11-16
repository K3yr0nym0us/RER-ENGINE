import type { ModalElectronOpenRequest } from '@shared-types'
import { ModalElectronGenericBody } from './ModalElectronGenericBody'
import ModalElectronErrorBoundary from './ModalElectronErrorBoundary'

interface ModalElectronBodyProps {
	payload: ModalElectronOpenRequest
	onClose: () => void
}

export function ModalElectronBody({ payload, onClose }: ModalElectronBodyProps) {
	return (
		<ModalElectronErrorBoundary key={payload.handlerId}>
			<ModalElectronGenericBody payload={payload} onClose={onClose} />
		</ModalElectronErrorBoundary>
	)
}
