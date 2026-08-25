import { createContext, useContext, type ReactNode } from 'react'

import { useModalElectron, type OpenModalElectronOptions } from '../hooks/useModalElectron'

export type ModalSize = 'sm' | 'md' | 'lg' | 'xl' | 'xxl'

export interface ModalConfig {
	title: string
	body: ReactNode
	size?: ModalSize
}

type ModalElectronApi = ReturnType<typeof useModalElectron>

const ModalElectronContext = createContext<ModalElectronApi | null>(null)

/** Una sola instancia de listeners IPC modal; los hijos usan `useModal()`. */
export function ModalProvider({ children }: { children: ReactNode }) {
	const electron = useModalElectron()
	return (
		<ModalElectronContext.Provider value={electron}>
			{children}
		</ModalElectronContext.Provider>
	)
}

export function useModal() {
	const electron = useContext(ModalElectronContext)
	if (!electron) {
		throw new Error('useModal debe usarse dentro de ModalProvider')
	}
	return {
		openModal: (cfg: ModalConfig) => electron.openModal(cfg as OpenModalElectronOptions),
		closeModal: electron.closeModal,
	}
}
