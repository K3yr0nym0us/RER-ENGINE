import type { ReactNode } from 'react'
import { CircleSquare } from 'react-bootstrap-icons'

import { useTraslate } from '@hooks'
import type { ModalConfirmMessageLine, ModalConfirmMessageSpec } from './modalConfirmMessageSpec'
import { useModalClose } from './useModalClose'

export interface ModalConfirmBodyProps {
	/** Solo en renderer principal (no IPC). */
	message?: ReactNode
	/** Mensaje serializable para ventana modal Electron. */
	messageSpec?: ModalConfirmMessageSpec
	confirmLabel?: string
	cancelLabel?: string
	confirmVariant?: 'danger' | 'primary' | 'success'
	buttonSize?: 'sm' | undefined
	onConfirm: () => void
	onClose?: () => void
}

function renderLine(line: ModalConfirmMessageLine, index: number) {
	const className = line.className ?? 'mb-2'
	return (
		<p key={index} className={className}>
			{line.parts.map((part, i) =>
				part.type === 'bold' ? (
					<strong key={i}>{part.value}</strong>
				) : (
					<span key={i}>{part.value}</span>
				),
			)}
		</p>
	)
}

function MessageFromSpec({ spec }: { spec: ModalConfirmMessageSpec }) {
	const { t } = useTraslate()

	if (spec.template === 'convertBlueprint') {
		const name = spec.entityName ?? ''
		return (
			<div className="text-center">
				<CircleSquare size={40} className="text-primary mb-3" />
				<p>
					{t('The entity will be converted to a Blueprint')}{' '}
					<strong>{name}</strong>.
				</p>
				<p className="text-secondary small">
					{t(
						'The Blueprint will save the current entity configuration: transformations, physics, animations and scripts.',
					)}
				</p>
				<p className="text-secondary small mb-0">
					{t('The created Blueprint will be available in the Quick Build tool')}
				</p>
			</div>
		)
	}

	if (spec.lines?.length) {
		return (
			<>
				{spec.lines.map((line, index) => renderLine(line, index))}
			</>
		)
	}

	return null
}

export function ModalConfirmBody({
	message,
	messageSpec,
	confirmLabel,
	cancelLabel,
	confirmVariant = 'danger',
	buttonSize,
	onConfirm,
	onClose,
}: ModalConfirmBodyProps) {
	const { t } = useTraslate()
	const defaultClose = useModalClose()
	const closeModal = onClose ?? defaultClose
	const btnClass = buttonSize === 'sm' ? 'btn-sm' : ''

	const handleConfirm = () => {
		onConfirm()
		closeModal()
	}

	const content =
		messageSpec != null ? <MessageFromSpec spec={messageSpec} /> : message

	return (
		<div>
			<div className="mb-3">{content}</div>
			<div className={`d-flex justify-content-end gap-2 ${buttonSize === 'sm' ? '' : 'mt-2'}`}>
				<button
					className={`btn btn-secondary ${btnClass}`.trim()}
					type="button"
					onClick={closeModal}
				>
					{cancelLabel ?? t('Cancel')}
				</button>
				<button
					className={`btn btn-${confirmVariant} ${btnClass}`.trim()}
					type="button"
					onClick={handleConfirm}
				>
					{confirmLabel ?? t('Delete')}
				</button>
			</div>
		</div>
	)
}
