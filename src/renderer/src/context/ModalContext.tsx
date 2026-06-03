import type { ReactNode } from 'react'

import { useModalElectron, type OpenModalElectronOptions } from '../hooks/useModalElectron'

export type ModalSize = 'sm' | 'md' | 'lg' | 'xl'

export interface ModalConfig {
	title: string
	body: ReactNode
	size?: ModalSize
}

/** Compatibilidad: delega en la ventana modal Electron (sin ocultar el motor). */
export function ModalProvider({ children }: { children: ReactNode }) {
	return <>{children}</>
}

export function useModal() {
	const electron = useModalElectron()
	return {
		openModal: (cfg: ModalConfig) => {
			void electron.openModal(cfg as OpenModalElectronOptions)
		},
		closeModal: electron.closeModal,
	}
}
