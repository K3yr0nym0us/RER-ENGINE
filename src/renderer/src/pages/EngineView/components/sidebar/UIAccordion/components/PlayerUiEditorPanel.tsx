import { useEffect, useState } from 'react'
import { Check2Square, Pencil } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { useTraslate } from '@hooks'
import type { PlayerUiEditorState } from '../../../../../../modal-electron/playerUiEditorTypes'
import EditingUiElementGroups from './EditingUiElementGroups'

export interface PlayerUiEditorPanelProps {
	state: PlayerUiEditorState
	onRename: (name: string) => void
	onAddText: () => void
	onAddImage: () => void
	onAddObject: () => void
	onCancelObjectDraw: () => void
	onRemoveText: (id: number, label: string) => void
	onRemoveImage: (id: number, label: string) => void
	onRemoveObject: (id: number, label: string) => void
	onSetElementProps: (
		kind: 'text' | 'image' | 'object',
		id: number,
		props: { locked?: boolean; z_index?: number },
	) => void
	onSave: () => void
	onCancel: () => void
}

export function PlayerUiEditorPanel({
	state,
	onRename,
	onAddText,
	onAddImage,
	onAddObject,
	onCancelObjectDraw,
	onRemoveText,
	onRemoveImage,
	onRemoveObject,
	onSetElementProps,
	onSave,
	onCancel,
}: PlayerUiEditorPanelProps) {
	const { t } = useTraslate()
	const [uiNameDraft, setUiNameDraft] = useState(state.screenName)
	const [isEditingUiName, setIsEditingUiName] = useState(false)

	useEffect(() => {
		setUiNameDraft(state.screenName)
		setIsEditingUiName(false)
	}, [state.screenId, state.screenName])

	return (
		<div>
			<div className="mb-2">
				<p className="prop-label small text-secondary mb-1">{t('UI name')}</p>
				<div className="input-group input-group-sm">
					<input
						type="text"
						value={uiNameDraft}
						onChange={(e) => setUiNameDraft(e.target.value)}
						className="form-control bg-dark text-info border-secondary prop-input"
						aria-label={t('UI name')}
						disabled={!isEditingUiName}
					/>
					{!isEditingUiName ? (
						<AppTooltip content={t('Edit name')} place="top">
							<button
								type="button"
								className="btn btn-outline-secondary"
								onClick={() => setIsEditingUiName(true)}
							>
								<Pencil />
							</button>
						</AppTooltip>
					) : (
						<AppTooltip content={t('Save changes')} place="top">
							<button
								type="button"
								className="btn btn-outline-info"
								disabled={!uiNameDraft.trim()}
								onClick={() => {
									const trimmed = uiNameDraft.trim()
									if (!trimmed) return
									onRename(trimmed)
									setIsEditingUiName(false)
								}}
							>
								<Check2Square />
							</button>
						</AppTooltip>
					)}
				</div>
			</div>

			<EditingUiElementGroups
				elements={state.elements}
				engineReady={state.engineReady}
				onAddText={onAddText}
				onAddImage={onAddImage}
				onAddObject={onAddObject}
				onCancelObjectDraw={onCancelObjectDraw}
				onRemoveText={onRemoveText}
				onRemoveImage={onRemoveImage}
				onRemoveObject={onRemoveObject}
				objectDrawActive={state.objectDrawActive}
				onSetElementProps={onSetElementProps}
				textEditHint={t(
					'Double-click a text box in the viewport to edit. Backspace removes characters. Hold Ctrl while dragging to snap to the grid.',
				)}
			/>

			<div className="d-flex gap-2 mt-3">
				<button className="btn btn-outline-secondary btn-sm flex-fill" type="button" onClick={onCancel}>
					{t('Cancel')}
				</button>
				<button className="btn btn-primary btn-sm flex-fill" type="button" onClick={onSave}>
					{t('Save')}
				</button>
			</div>
		</div>
	)
}

export default PlayerUiEditorPanel
