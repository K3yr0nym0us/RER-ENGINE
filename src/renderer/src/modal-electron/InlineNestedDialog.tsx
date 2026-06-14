import type { ReactNode } from 'react'

export interface InlineNestedDialogProps {
	title: string
	children: ReactNode
	footer?: ReactNode
	onClose: () => void
	size?: 'sm' | 'lg'
}

/** Diálogo anidado dentro de una ventana modal Electron (sustituye react-bootstrap Modal). */
export function InlineNestedDialog({
	title,
	children,
	footer,
	onClose,
	size = 'sm',
}: InlineNestedDialogProps) {
	const maxWidth = size === 'lg' ? 800 : 400

	return (
		<div
			className="position-absolute top-0 start-0 w-100 h-100 d-flex align-items-center justify-content-center"
			style={{ zIndex: 20, backgroundColor: 'rgba(0, 0, 0, 0.55)' }}
			role="dialog"
			aria-modal="true"
			aria-label={title}
		>
			<div
				className="section-card rounded-3 shadow bg-body"
				style={{ width: '90%', maxWidth, maxHeight: '85%', overflow: 'auto' }}
			>
				<div className="d-flex justify-content-between align-items-center px-3 pt-3 pb-2 border-bottom border-secondary">
					<h6 className="mb-0">{title}</h6>
					<button type="button" className="btn-close" aria-label="Close" onClick={onClose} />
				</div>
				<div className="p-3">{children}</div>
				{footer ? <div className="px-3 pb-3">{footer}</div> : null}
			</div>
		</div>
	)
}
