import { useCallback, useEffect, useState } from 'react'

import type { ModalElectronOpenRequest } from '@shared-types'
import { SocketConfigModalContent } from '../pages/EngineView/components/sidebar/ToolsAccordion/components/SocketConfigModalContent'
import type { SocketConfigModalAction, SocketConfigModalState } from './socketConfigModalTypes'

interface SocketConfigModalElectronHostProps {
	payload: ModalElectronOpenRequest
}

const EMPTY_STATE: SocketConfigModalState = {
	activeTab: 'create',
	create: {
		entityId: null,
		entityName: '',
		sockets: [],
		socketBonePicked: null,
		statusMessage: 'awaiting_entity',
	},
	links: {
		hostEntityId: null,
		hostEntityName: '',
		sockets: [],
		attachments: [],
		pickPhase: 'host',
		pendingSocketName: null,
		statusMessage: 'awaiting_host',
	},
}

export function SocketConfigModalElectronHost({ payload }: SocketConfigModalElectronHostProps) {
	const [state, setState] = useState<SocketConfigModalState>(
		(payload.socketConfigModalState as SocketConfigModalState | undefined) ?? EMPTY_STATE,
	)

	useEffect(() => {
		const initial = payload.socketConfigModalState as SocketConfigModalState | undefined
		if (initial) setState(initial)
	}, [payload.handlerId, payload.socketConfigModalState])

	useEffect(() => {
		const remove = window.electronAPI.onModalElectronPatch((data) => {
			if (data.handlerId !== payload.handlerId) return
			if (data.socketConfigModalState) {
				setState(data.socketConfigModalState as SocketConfigModalState)
			}
		})
		return remove
	}, [payload.handlerId])

	const delegate = useCallback(
		(action: SocketConfigModalAction) =>
			window.electronAPI.socketConfigModalAction(payload.handlerId, action),
		[payload.handlerId],
	)

	return <SocketConfigModalContent state={state} onAction={delegate} />
}

/** Cuerpo registrable para openModal (clave de componente). */
export function SocketConfigModalBody() {
	return null
}
