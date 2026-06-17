import { useState } from 'react'

import type { EntitySocket3D } from '@shared-types'
import { useTraslate } from '@hooks'
import { useModalClose } from '@hooks'

export interface AttachToSocketModalBodyProps {
	sockets: EntitySocket3D[]
	onConfirm: (socketName: string) => void
}

export function AttachToSocketModalBody({ sockets, onConfirm }: AttachToSocketModalBodyProps) {
	const { t } = useTraslate()
	const closeModal = useModalClose()
	const [selectedName, setSelectedName] = useState('')

	const handleConfirm = () => {
		if (!selectedName) return
		onConfirm(selectedName)
		closeModal()
	}

	return (
		<div>
			<label className="form-label small mb-1">{t('Socket')}</label>
			<select
				className="form-select form-select-sm mb-3"
				value={selectedName}
				onChange={(e) => setSelectedName(e.target.value)}
			>
				<option value="">{t('Select a socket')}</option>
				{sockets.map((socket) => (
					<option key={socket.name} value={socket.name}>
						{socket.name} — {socket.bone_name}
					</option>
				))}
			</select>
			<div className="d-flex justify-content-end gap-2">
				<button type="button" className="btn btn-secondary btn-sm" onClick={closeModal}>
					{t('Cancel')}
				</button>
				<button
					type="button"
					className="btn btn-primary btn-sm"
					disabled={!selectedName}
					onClick={handleConfirm}
				>
					{t('Confirm')}
				</button>
			</div>
		</div>
	)
}
