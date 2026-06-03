import { createContext, useContext, type ReactNode } from 'react'

export interface ModalElectronCloseContextValue {
	closeModal: () => void
}

export const ModalElectronCloseContext = createContext<ModalElectronCloseContextValue | null>(null)

export function ModalElectronCloseProvider({
	closeModal,
	children,
}: {
	closeModal: () => void
	children: ReactNode
}) {
	return (
		<ModalElectronCloseContext.Provider value={{ closeModal }}>
			{children}
		</ModalElectronCloseContext.Provider>
	)
}

export function useModalElectronCloseContext(): ModalElectronCloseContextValue | null {
	return useContext(ModalElectronCloseContext)
}
