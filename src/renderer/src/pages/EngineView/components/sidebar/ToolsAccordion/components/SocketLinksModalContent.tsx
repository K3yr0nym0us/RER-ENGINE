import { useEffect } from 'react'

import { Link45deg, XCircle } from 'react-bootstrap-icons'

import type {
	SocketLinksModalAction,
	SocketLinksModalState,
} from '../../../../../../modal-electron/socketLinksModalTypes'
import { useTraslate } from '@hooks'

export interface SocketLinksModalContentProps {
	state: SocketLinksModalState
	onAction: (action: SocketLinksModalAction) => void
	onRequestHostPick?: () => void
}

function socketNameMatches(a: string, b: string): boolean {
	return a.trim().toLowerCase() === b.trim().toLowerCase()
}

export function SocketLinksModalContent({ state, onAction, onRequestHostPick }: SocketLinksModalContentProps) {
	const { t } = useTraslate()
	const {
		hostEntityId,
		hostEntityName,
		sockets,
		attachments,
		pickPhase,
		pendingSocketName,
		statusMessage,
	} = state

	useEffect(() => {
		if (hostEntityId == null) return
		onAction({ action: 'requestSockets' })
		// eslint-disable-next-line react-hooks/exhaustive-deps -- al fijar host
	}, [hostEntityId])

	if (hostEntityId == null) {
		return (
			<div className="d-flex flex-column gap-2">
				<button
					type="button"
					className="btn btn-sm btn-outline-primary"
					onClick={() => onRequestHostPick?.()}
				>
					{t('Select host entity')}
				</button>
				<p className="small text-warning mb-0">
					{statusMessage === 'invalid_host'
						? t('This entity has no sockets. Click the character or prop that owns the sockets.')
						: t('Click the entity with sockets in the viewport (e.g. the player).')}
				</p>
			</div>
		)
	}

	return (
		<div>
			<p className="small text-secondary mb-2">
				{t('Host')}: <span className="fw-semibold text-light">{hostEntityName}</span>
			</p>

			{pickPhase === 'child' && pendingSocketName && (
				<div className="alert alert-info py-2 px-2 small mb-3 d-flex justify-content-between align-items-start gap-2">
					<span>
						{t('Click the object in the viewport to link it to socket')}{' '}
						<strong>{pendingSocketName}</strong>.
					</span>
					<button
						type="button"
						className="btn btn-sm btn-link text-info p-0"
						onClick={() => onAction({ action: 'cancelPick' })}
						aria-label={t('Cancel')}
					>
						<XCircle size={16} />
					</button>
				</div>
			)}

			{statusMessage === 'invalid_child' && (
				<p className="small text-danger mb-2">
					{t('That entity cannot be linked to this socket. Click another object.')}
				</p>
			)}

			{sockets.length === 0 ? (
				<p className="small text-secondary mb-0">
					{t('No sockets on this entity. Use Create Sockets in Tools first.')}
				</p>
			) : (
				<div className="d-flex flex-column gap-2">
					{sockets.map((socket) => {
						const linked = attachments.filter((entry) =>
							socketNameMatches(entry.socketName, socket.name),
						)
						return (
							<div
								key={socket.name}
								className="border border-secondary rounded p-2 bg-dark"
							>
								<div className="d-flex justify-content-between align-items-start gap-2 mb-1">
									<div>
										<div className="small fw-semibold text-light">{socket.name}</div>
										<div className="small text-secondary">{socket.bone_name}</div>
									</div>
									<button
										type="button"
										className="btn btn-sm btn-outline-primary"
										disabled={pickPhase === 'child'}
										onClick={() => onAction({ action: 'startLink', socketName: socket.name })}
									>
										<Link45deg className="me-1" />
										{t('Link object')}
									</button>
								</div>
								{linked.length === 0 ? (
									<div className="small text-muted">{t('No linked objects')}</div>
								) : (
									<ul className="list-unstyled mb-0 small">
										{linked.map((entry) => (
											<li
												key={entry.childId}
												className="d-flex justify-content-between align-items-center gap-2 py-1 border-top border-secondary-subtle"
											>
												<span className="text-light text-truncate">{entry.childName}</span>
												<button
													type="button"
													className="btn btn-sm btn-outline-danger flex-shrink-0"
													onClick={() =>
														onAction({ action: 'detach', childId: entry.childId })
													}
												>
													{t('Unlink')}
												</button>
											</li>
										))}
									</ul>
								)}
							</div>
						)
					})}
				</div>
			)}
		</div>
	)
}
