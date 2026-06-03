import { useState } from 'react'
import { useTraslate } from '@hooks'
import { useModalClose } from '../../../modal-electron/useModalClose'

export function CreateSceneModalBody({
	defaultName,
	onCancel,
	onCreate,
}: {
	defaultName: string
	onCancel?: () => void
	onCreate: (name: string) => void
}) {
	const { t } = useTraslate()
	const closeModal = useModalClose()
	const cancel = onCancel ?? closeModal
	const [draftName, setDraftName] = useState(defaultName)

	return (
		<div className="d-flex flex-column gap-3">
			<div>
				<label htmlFor="scene-name-create" className="form-label mb-1">
					{t('Scene name')}
				</label>
				<input
					id="scene-name-create"
					type="text"
					value={draftName}
					className="form-control"
					onChange={(event) => setDraftName(event.target.value)}
				/>
			</div>
			<div className="d-flex justify-content-end gap-2">
				<button className="btn btn-secondary" onClick={cancel} type="button">
					{t('Cancel')}
				</button>
				<button
					className="btn btn-success"
					onClick={() => {
						onCreate(draftName.trim() || defaultName)
						closeModal()
					}}
					type="button"
				>
					{t('Create scene')}
				</button>
			</div>
		</div>
	)
}

export function SceneRenameModalBody({
	defaultName,
	onRename,
}: {
	defaultName: string
	onRename: (name: string) => void
}) {
	const { t } = useTraslate()
	const closeModal = useModalClose()
	const [draftName, setDraftName] = useState(defaultName)

	return (
		<div className="d-flex flex-column gap-3">
			<div>
				<label htmlFor="scene-name-rename" className="form-label mb-1">
					{t('Scene name')}
				</label>
				<input
					id="scene-name-rename"
					type="text"
					value={draftName}
					className="form-control"
					onChange={(event) => setDraftName(event.target.value)}
				/>
			</div>
			<div className="d-flex gap-2 flex-wrap">
				<button
					className="btn btn-success"
					onClick={() => {
						onRename(draftName.trim() || defaultName)
						closeModal()
					}}
					type="button"
				>
					{t('Save name')}
				</button>
				<button className="btn btn-secondary" onClick={closeModal} type="button">
					{t('Cancel')}
				</button>
			</div>
		</div>
	)
}

export function DeleteBlockedBody() {
	const { t } = useTraslate()

	return (
		<div className="d-flex flex-column gap-2">
			<p className="mb-0">
				{t('You cannot delete this scene because it is the only one in the project.')}
			</p>
			<small className="text-secondary">
				{t('There must be at least one scene to keep the editor in a valid state.')}
			</small>
		</div>
	)
}

export function DeleteConfirmBody({ onConfirm }: { onConfirm: () => void }) {
	const { t } = useTraslate()
	const closeModal = useModalClose()

	return (
		<div className="d-flex flex-column gap-3">
			<p className="mb-0">{t('This action will delete the selected scene.')}</p>
			<div className="d-flex justify-content-end gap-2">
				<button className="btn btn-secondary" onClick={closeModal} type="button">
					{t('Cancel')}
				</button>
				<button
					className="btn btn-danger"
					onClick={() => {
						onConfirm()
						closeModal()
					}}
					type="button"
				>
					{t('Delete')}
				</button>
			</div>
		</div>
	)
}
