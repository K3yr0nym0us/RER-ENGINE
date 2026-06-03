import type { ModalElectronOpenRequest } from '@shared-types'
import { ModalElectronGenericBody } from './ModalElectronGenericBody'

interface ModalElectronBodyProps {
	payload: ModalElectronOpenRequest
	onClose: () => void
}

export function ModalElectronBody({ payload, onClose }: ModalElectronBodyProps) {
	return <ModalElectronGenericBody payload={payload} onClose={onClose} />
}
