import type { OpenModalElectronOptions } from '../hooks/useModalElectron'

type ParentBridge = {
	openModal: (opts: OpenModalElectronOptions) => Promise<void>
}

let parentBridge: ParentBridge | null = null

export function setModalElectronParentBridge(bridge: ParentBridge | null): void {
	parentBridge = bridge
}

export function getModalElectronParentBridge(): ParentBridge | null {
	return parentBridge
}
