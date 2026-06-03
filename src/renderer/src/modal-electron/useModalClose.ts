import { useCallback } from 'react'

import { useModalElectronCloseContext } from './ModalElectronCloseContext'

/**
 * Cierra la ventana modal Electron.
 * No usa useModalElectron/useContextEngine: la ventana hijo no tiene EngineProvider.
 */
export function useModalClose(): () => void {
	const electronCtx = useModalElectronCloseContext()

	return useCallback(() => {
		if (electronCtx) {
			electronCtx.closeModal()
			return
		}
		void window.electronAPI.closeModalElectron()
	}, [electronCtx])
}
