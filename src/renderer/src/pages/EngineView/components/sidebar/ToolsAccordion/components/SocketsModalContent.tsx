import { useEffect } from 'react'

import { SocketsPanel } from '../../PropertiesAccordion/SocketsPanel'
import type { SocketsModalAction, SocketsModalState } from '../../../../../../modal-electron/socketsModalTypes'
import { useTraslate } from '@hooks'

export interface SocketsModalContentProps {
	state: SocketsModalState
	onAction: (action: SocketsModalAction) => void
	onRequestEntityPick?: () => void
}

export function SocketsModalContent({ state, onAction, onRequestEntityPick }: SocketsModalContentProps) {
	const { t } = useTraslate()
	const { entityId, entityName, sockets, socketBonePicked, statusMessage } = state

	useEffect(() => {
		if (entityId == null) return
		onAction({ action: 'requestSockets' })
		// eslint-disable-next-line react-hooks/exhaustive-deps -- solo al abrir o cambiar entidad
	}, [entityId])

	if (entityId == null) {
		return (
			<div className="d-flex flex-column gap-2">
				<button
					type="button"
					className="btn btn-sm btn-outline-primary"
					onClick={() => onRequestEntityPick?.()}
				>
					{t('Select entity')}
				</button>
				<p className="small text-warning mb-0">
					{statusMessage === 'invalid_entity'
						? t('This entity cannot have sockets. Click another entity in the viewport.')
						: t('Click an entity in the viewport to manage its sockets.')}
				</p>
			</div>
		)
	}

	return (
		<div>
			<p className="small text-secondary mb-3">
				{t('Entity')}: <span className="fw-semibold text-light">{entityName}</span>
			</p>
			<SocketsPanel
				entityId={entityId}
				sockets={sockets}
				socketBonePicked={socketBonePicked}
				onSaveSocket={(socket) => onAction({ action: 'upsertSocket', socket })}
				onRemoveSocket={(name) => onAction({ action: 'removeSocket', name })}
				onSetBonePickMode={(active) => onAction({ action: 'setSocketBonePickMode', active })}
			/>
		</div>
	)
}
