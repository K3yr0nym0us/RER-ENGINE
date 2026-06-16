import { useEffect, useMemo, useState } from 'react'

import { Nav, Tab } from 'react-bootstrap'
import {
	CircleSquare,
	Check2Square,
	Pencil,
	Trash,
	BoxSeam,
	PlayFill,
	StopFill,
	Link45deg,
} from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { InlineNestedDialog } from '../../../../../modal-electron/InlineNestedDialog'
import { TransformPanel, ScriptingPanelContent } from '.'
import type { TransformSendCommand } from './TransformPanel'
import { useTraslate } from '@hooks'
import type {
	EntityPropertiesAction,
	EntityPropertiesAnimation,
	EntityPropertiesState,
} from '../../../../../modal-electron/entityPropertiesTypes'

type PropertiesTab = 'transform' | 'animations' | 'scripting'

export interface EntityPropertiesModalContentProps {
	state: EntityPropertiesState
	onAction: (action: EntityPropertiesAction) => void | Promise<void>
}

export function EntityPropertiesModalContent({
	state,
	onAction,
}: EntityPropertiesModalContentProps) {
	const { t } = useTraslate()
	const {
		projectType,
		selectedEntity,
		multiSelectedIds,
		multiSelectAlreadyMerged,
		isCharacter,
		isEnvironment,
		isPlayer,
		isEditorCamera,
		isCollider,
		isExecutionArea,
		isFromBlueprint,
		linkedBlueprintName,
		scripts,
		animationPlayingIds,
	} = state

	const [entityNameDraft, setEntityNameDraft] = useState('')
	const [isEditingEntityName, setIsEditingEntityName] = useState(false)
	const [animations, setAnimations] = useState<EntityPropertiesAnimation[]>([])
	const [playingAnimationName, setPlayingAnimationName] = useState<string | null>(null)
	const [pendingConfirm, setPendingConfirm] = useState<{
		title: string
		message: React.ReactNode
		confirmLabel: string
		onConfirm: () => void
	} | null>(null)
	const [activeTab, setActiveTab] = useState<PropertiesTab>('transform')

	const is2D = projectType === '2D'
	const is3D = projectType === '3D'
	const isMultiSelect = multiSelectedIds.length > 1

	useEffect(() => {
		setEntityNameDraft(selectedEntity?.name ?? '')
		setIsEditingEntityName(false)
	}, [selectedEntity?.id, selectedEntity?.name])

	useEffect(() => {
		if (!selectedEntity?.id) {
			setAnimations([])
			return
		}
		setAnimations(selectedEntity.animations ?? [])
	}, [selectedEntity?.id, selectedEntity?.animations])

	useEffect(() => {
		setPlayingAnimationName(null)
	}, [selectedEntity?.id])

	const handleSend = (cmd: TransformSendCommand) => {
		onAction({ action: 'setTransform', cmd })
	}

	const physicsEnabled = selectedEntity?.physicsEnabled ?? false
	const physicsType = selectedEntity?.physicsType || 'dynamic'
	const hasEmbeddedModelClips =
		is3D && (animations.some((a) => a.embedded_in_model) ?? false)

	const showPhysicsSection = !isCollider && !isExecutionArea

	const tabs = useMemo(() => {
		const list: PropertiesTab[] = []
		list.push('transform')
		if (!isCollider && !isExecutionArea && (is2D || hasEmbeddedModelClips)) {
			list.push('animations')
		}
		if (!isCollider) list.push('scripting')
		return list
	}, [isCollider, isExecutionArea, is2D, hasEmbeddedModelClips])

	useEffect(() => {
		setActiveTab((prev) => (tabs.includes(prev) ? prev : (tabs[0] ?? 'transform')))
	}, [selectedEntity?.id, tabs])

	const openConfirm = (
		message: React.ReactNode,
		onConfirm: () => void,
		confirmLabel?: string,
		title?: string,
	) => {
		setPendingConfirm({
			title: title ?? t('Confirm action'),
			message,
			confirmLabel: confirmLabel ?? t('Confirm'),
			onConfirm,
		})
	}

	const syncAnimations = (next: EntityPropertiesAnimation[]) => {
		if (!selectedEntity?.id) {
			setAnimations(next)
			return next
		}
		onAction({ action: 'updateAnimations', id: selectedEntity.id, animations: next })
		setAnimations(next)
		return next
	}

	if (!selectedEntity && !isMultiSelect) {
		return (
			<p className="text-secondary fst-italic small mb-0 px-1">
				{t('Click on an object to view it')}
			</p>
		)
	}

	if (isMultiSelect) {
		return (
			<div className="position-relative">
				<p className="text-secondary fst-italic small mb-0 px-1">
					{multiSelectedIds.length} {t('entities selected')}
				</p>
				<div className="mt-3 pt-2 border-top border-secondary d-flex flex-column gap-2">
					{is3D &&
						(multiSelectAlreadyMerged ? (
							<p className="text-secondary small mb-0 px-1 fst-italic">
								{t('Entities already merged')}
							</p>
						) : (
							<button
								type="button"
								className="btn btn-sm btn-outline-primary w-100"
								onClick={() => onAction({ action: 'mergeEntities', ids: multiSelectedIds })}
							>
								<Link45deg className="me-2" />
								{t('Merge entities')}
							</button>
						))}
					<button
						type="button"
						className="btn btn-sm btn-outline-danger w-100"
						onClick={() =>
							openConfirm(
								<>
									<p className="mb-2">
										{t('Are you sure you want to')} {t('delete')}{' '}
										<strong>{multiSelectedIds.length}</strong> {t('entities?')}
									</p>
									<p className="text-danger small mb-0">{t('This action cannot be undone.')}</p>
								</>,
								() => onAction({ action: 'removeMultiple', ids: multiSelectedIds }),
								`${t('Yes,')} ${t('delete')}`,
							)
						}
					>
						<Trash className="me-2" />
						{t('Delete')} ({multiSelectedIds.length})
					</button>
				</div>
				{pendingConfirm && (
					<ConfirmSubModal
						{...pendingConfirm}
						onClose={() => setPendingConfirm(null)}
					/>
				)}
			</div>
		)
	}

	if (!selectedEntity) return null

	const trimmedEntityName = entityNameDraft.trim()
	const hasValidEntityName = trimmedEntityName.length > 0
	const canRename = hasValidEntityName && trimmedEntityName !== selectedEntity.name

	const handleRemove = () => {
		if (isPlayer) return
		openConfirm(
			<>
				<p className="mb-2">
					{t('Are you sure you want to')} {t('delete')} {t('this entity?')}
				</p>
				<p className="text-danger small mb-0">{t('This action cannot be undone.')}</p>
			</>,
			() => onAction({ action: 'removeEntity', id: selectedEntity.id }),
			`${t('Yes,')} ${t('delete')}`,
		)
	}

	const openCreateAnimationModal = () => {
		onAction({
			action: 'openNestedModal',
			kind: 'createAnimation',
			payload: { entityId: selectedEntity.id },
		})
	}

	const removeAnimation = (index: number) => {
		const anim = animations[index]
		if (!anim) return
		if (animationPlayingIds.includes(selectedEntity.id)) {
			onAction({ action: 'send', cmd: { cmd: 'stop_animation', id: selectedEntity.id } })
		}
		onAction({ action: 'send', cmd: { cmd: 'remove_animation', id: selectedEntity.id, name: anim.name } })
		syncAnimations(animations.filter((_, i) => i !== index))
	}

	const playAnimation = async (index: number) => {
		const anim = animations[index]
		if (!anim) return
		const canPlayEmbedded3D = is3D && !!anim.embedded_in_model
		if (!canPlayEmbedded3D && anim.frames.length === 0) return

		const isPlayingThis = playingAnimationName === anim.name
		if (isPlayingThis) {
			onAction({ action: 'send', cmd: { cmd: 'stop_animation', id: selectedEntity.id } })
			onAction({ action: 'setAnimationPlaying', id: selectedEntity.id, playing: false })
			setPlayingAnimationName(null)
			return
		}
		if (animationPlayingIds.includes(selectedEntity.id)) {
			onAction({ action: 'send', cmd: { cmd: 'stop_animation', id: selectedEntity.id } })
		}
		setPlayingAnimationName(anim.name)
		if (anim.loop) {
			onAction({
				action: 'send',
				cmd: { cmd: 'play_animation', id: selectedEntity.id, name: anim.name, loop: anim.loop },
			})
			onAction({ action: 'setAnimationPlaying', id: selectedEntity.id, playing: true })
		} else {
			onAction({ action: 'setAnimationPlaying', id: selectedEntity.id, playing: true })
			void onAction({
				action: 'sendAsync',
				cmd: { cmd: 'play_animation', id: selectedEntity.id, name: anim.name, loop: anim.loop },
				waitEvent: 'animation_finished',
			}).then(() => {
				setPlayingAnimationName(null)
			})
		}
	}

	const editAnimation = (index: number) => {
		const anim = animations[index]
		if (!anim) return
		const spritePath = anim.frames[0]?.path
		if (!spritePath) return
		onAction({
			action: 'openNestedModal',
			kind: 'editAnimation',
			payload: {
				entityId: selectedEntity.id,
				animationIndex: index,
				spritePath,
				animationName: anim.name,
				initialFrames: anim.frames.map((f) => ({
					x: f.src_x ?? 0,
					y: f.src_y ?? 0,
					width: f.src_w ?? anim.logical_w ?? 64,
					height: f.src_h ?? anim.logical_h ?? 64,
					pivot_x: f.pivot_x,
					pivot_y: f.pivot_y,
				})),
				initialFps: anim.fps,
				initialLoop: anim.loop,
				initialIsDefault: anim.is_default ?? false,
				initialIsCancelable: anim.is_cancelable ?? false,
				initialFacingRight: anim.facing_right ?? true,
				initialAudioPath: anim.audio_path,
				initialScripts: anim.scripts,
				initialSelectionMode: anim.selection_mode,
				initialGridSize: anim.grid_size,
				initialCellOffsetX: anim.cell_offset_x,
				initialCellOffsetY: anim.cell_offset_y,
			},
		})
	}

	const scriptingHandlers = {
		onNew: () =>
			onAction({
				action: 'openNestedModal',
				kind: 'scriptEditor',
				payload: { entityId: selectedEntity.id, title: t('New Rhai script') },
			}),
		onVisual: () => onAction({ action: 'openNestedModal', kind: 'visualScripting', payload: {} }),
		onEdit: (name: string) => {
			const existing = scripts.find((s) => s.name === name)
			if (!existing) return
			onAction({
				action: 'openNestedModal',
				kind: 'scriptEditor',
				payload: {
					entityId: selectedEntity.id,
					title: `${t('Edit script')}: ${name}`,
					initialData: existing,
					replacing: name,
				},
			})
		},
		onRemove: (name: string) =>
			openConfirm(
				<div className="text-center">
					<p>
						{t('Delete script confirm')} <strong>{name}</strong>?
					</p>
					<p className="text-danger small mb-0">{t('This action cannot be undone.')}</p>
				</div>,
				() => {
					const next = scripts.filter((s) => s.name !== name)
					onAction({ action: 'updateScripts', id: selectedEntity.id, scripts: next })
					if (next.length === 0) {
						onAction({ action: 'send', cmd: { cmd: 'unload_script', id: selectedEntity.id } })
					}
				},
				t('Yes, delete'),
			),
	}

	const blueprintTooltip = linkedBlueprintName
		? `${t('Based on blueprint')}: ${linkedBlueprintName}`
		: t('Based on blueprint')

	const showModelActions = !isCollider && !isExecutionArea

	return (
		<div className="position-relative">
			<div className="entity-props-toolbar d-flex align-items-stretch gap-1 mb-0 flex-nowrap">
				<div className="input-group input-group-sm flex-grow-1 min-w-0">
					<input
						type="text"
						value={entityNameDraft}
						onChange={(e) => setEntityNameDraft(e.target.value)}
						className="form-control bg-dark text-info border-secondary prop-input"
						aria-label={t('Entity name')}
						disabled={!isEditingEntityName}
					/>
					{!isEditingEntityName ? (
						<button
							type="button"
							className="btn btn-outline-secondary d-inline-flex align-items-center gap-1"
							onClick={() => setIsEditingEntityName(true)}
						>
							<Pencil />
							<span>{t('Edit')}</span>
						</button>
					) : (
						<AppTooltip content={t('Save changes')} place="bottom">
							<button
								type="button"
								className="btn btn-outline-info"
								disabled={!hasValidEntityName}
								onClick={() => {
									if (!hasValidEntityName) return
									if (canRename) {
										onAction({
											action: 'setEntityName',
											id: selectedEntity.id,
											name: trimmedEntityName,
										})
									}
									setIsEditingEntityName(false)
								}}
							>
								<Check2Square />
							</button>
						</AppTooltip>
					)}
				</div>
				{!isPlayer && (
					<AppTooltip content={t('Delete')} place="left">
						<button
							type="button"
							className="btn btn-sm btn-outline-danger flex-shrink-0"
							onClick={handleRemove}
						>
							<Trash />
						</button>
					</AppTooltip>
				)}
			</div>

			{showModelActions && (
				<div className="entity-props-model-actions d-flex gap-2 mt-2 w-100">
					{is3D && !isFromBlueprint && (
						<button
							type="button"
							className="btn btn-sm btn-outline-info entity-props-model-action-btn d-inline-flex align-items-center justify-content-center gap-1"
							onClick={() =>
								onAction({
									action: 'openNestedModal',
									kind: 'replaceModel',
									payload: {
										hintText: isPlayer
											? t('Replace model player hint')
											: t('Replace model entity hint'),
										isPlayer,
										isCharacter,
										isEnvironment,
									},
								})
							}
						>
							<BoxSeam />
							<span className="text-truncate">{t('Replace model')}</span>
						</button>
					)}
					{isFromBlueprint ? (
						<button
							type="button"
							className="btn btn-sm btn-outline-secondary entity-props-model-action-btn d-inline-flex align-items-center justify-content-center gap-1"
							disabled
							aria-label={blueprintTooltip}
						>
							<CircleSquare />
							<span className="text-truncate">{t('Based on blueprint')}</span>
						</button>
					) : (
						<button
							type="button"
							className="btn btn-sm btn-outline-primary entity-props-model-action-btn d-inline-flex align-items-center justify-content-center gap-1"
							onClick={() =>
								onAction({ action: 'openNestedModal', kind: 'convertBlueprint', payload: {} })
							}
						>
							<CircleSquare />
							<span className="text-truncate">{t('Convert to Blueprint')}</span>
						</button>
					)}
				</div>
			)}

			{showPhysicsSection && (
				<div className="entity-props-physics">
					{isEnvironment ? (
						<div className="d-flex align-items-center gap-2">
							<input
								type="checkbox"
								id="environment-collision"
								className="form-check-input"
								checked={physicsEnabled}
								onChange={(e) => {
									onAction({
										action: 'setPhysics',
										id: selectedEntity.id,
										enabled: e.target.checked,
										bodyType: 'static',
									})
								}}
							/>
							<label htmlFor="environment-collision" className="form-check-label text-light small mb-0">
								{t('With collision')}
							</label>
						</div>
					) : isPlayer ? (
						<>
							<div className="d-flex align-items-center gap-2">
								<input type="checkbox" id="player-physics" className="form-check-input" checked disabled readOnly />
								<label htmlFor="player-physics" className="form-check-label text-light small mb-0">
									{t('Enable physics')}
								</label>
							</div>
							<select
								value="dynamic"
								className="form-select form-select-sm bg-dark text-light border-secondary"
								disabled
							>
								<option value="dynamic">{t('Dynamic (gravity)')}</option>
							</select>
						</>
					) : (
						<>
							<div className="d-flex align-items-center gap-2">
								<input
									type="checkbox"
									id="physics-enabled"
									className="form-check-input"
									checked={physicsEnabled}
									onChange={(e) => {
										const next = e.target.checked
										const bodyType = next && isCharacter ? 'kinematic' : physicsType
										onAction({
											action: 'setPhysics',
											id: selectedEntity.id,
											enabled: next,
											bodyType,
										})
									}}
								/>
								<label htmlFor="physics-enabled" className="form-check-label text-light small mb-0">
									{t('Enable physics')}
								</label>
							</div>
							{physicsEnabled && (
								<select
									value={physicsType}
									className="form-select form-select-sm bg-dark text-light border-secondary"
									onChange={(e) => {
										onAction({
											action: 'setPhysics',
											id: selectedEntity.id,
											enabled: true,
											bodyType: e.target.value,
										})
									}}
								>
									<option value="dynamic">{t('Dynamic (gravity)')}</option>
									<option value="static">{t('Static (does not move)')}</option>
									<option value="kinematic">{t('Kinematic (by code)')}</option>
								</select>
							)}
						</>
					)}
				</div>
			)}

			<Tab.Container activeKey={activeTab} onSelect={(k) => k && setActiveTab(k as PropertiesTab)}>
				<Nav variant="tabs" className="entity-props-nav entity-props-nav--spaced mb-3">
					{tabs.includes('transform') && (
						<Nav.Item>
							<Nav.Link eventKey="transform">{t('Transformations')}</Nav.Link>
						</Nav.Item>
					)}
					{tabs.includes('animations') && (
						<Nav.Item>
							<Nav.Link eventKey="animations">{t('Animations')}</Nav.Link>
						</Nav.Item>
					)}
					{tabs.includes('scripting') && (
						<Nav.Item>
							<Nav.Link eventKey="scripting">{t('Program entity')}</Nav.Link>
						</Nav.Item>
					)}
				</Nav>
				<Tab.Content className="entity-props-tab-content">
					{tabs.includes('transform') && (
						<Tab.Pane eventKey="transform" className="py-1 px-1">
							<TransformPanel
								entity={selectedEntity}
								is2D={is2D}
								isPlayCharacter={isPlayer && is3D && !isCollider && !isExecutionArea}
								isEditorCamera={isEditorCamera && is3D && !isCollider && !isExecutionArea}
								onSend={handleSend}
							/>
						</Tab.Pane>
					)}
					{tabs.includes('animations') && (
						<Tab.Pane eventKey="animations" className="py-1 px-1">
							{!is3D && (
								<button
									type="button"
									className="btn btn-outline-success btn-sm w-100 fw-bold mb-2"
									onClick={openCreateAnimationModal}
								>
									{t('+ New animation')}
								</button>
							)}
							{!is3D && animations.length === 0 && (
								<div className="alert alert-secondary py-1 text-center small mb-0" role="alert">
									{t('No animations. Add a new one to start.')}
								</div>
							)}
							{animations.length > 0 && (
								<div className="d-flex flex-column gap-1">
									{animations.map((anim, idx) => {
										const canPlay = is3D ? !!anim.embedded_in_model : anim.frames.length > 0
										const isPlayingThis = playingAnimationName === anim.name
										return (
											<div
												key={anim.id ?? `${anim.name}-${idx}`}
												className="d-flex align-items-center gap-2 p-1 border border-secondary rounded bg-dark"
											>
												<span className="small text-light flex-fill text-truncate">{anim.name}</span>
												<span
													role="button"
													tabIndex={canPlay ? 0 : -1}
													className={isPlayingThis ? 'text-danger' : 'text-success'}
													style={{ cursor: canPlay ? 'pointer' : 'not-allowed', opacity: canPlay ? 1 : 0.5 }}
													onClick={canPlay ? () => void playAnimation(idx) : undefined}
												>
													{isPlayingThis ? <StopFill /> : <PlayFill />}
												</span>
												{!is3D && (
													<span
														role="button"
														tabIndex={canPlay ? 0 : -1}
														className="text-warning"
														style={{ cursor: canPlay ? 'pointer' : 'not-allowed' }}
														onClick={canPlay ? () => editAnimation(idx) : undefined}
													>
														<Pencil />
													</span>
												)}
												{!is3D && (
													<span
														role="button"
														tabIndex={0}
														className="text-danger"
														style={{ cursor: 'pointer' }}
														onClick={() =>
															openConfirm(
																<>
																	{t('Are you sure you want to delete the animation')}{' '}
																	<strong>{anim.name}</strong>?
																</>,
																() => removeAnimation(idx),
															)
														}
													>
														<Trash />
													</span>
												)}
											</div>
										)
									})}
								</div>
							)}
						</Tab.Pane>
					)}
					{tabs.includes('scripting') && (
						<Tab.Pane eventKey="scripting" className="py-0">
							<ScriptingPanelContent scripts={scripts} {...scriptingHandlers} />
						</Tab.Pane>
					)}
				</Tab.Content>
			</Tab.Container>

			{pendingConfirm && (
				<ConfirmSubModal {...pendingConfirm} onClose={() => setPendingConfirm(null)} />
			)}
		</div>
	)
}

function ConfirmSubModal({
	title,
	message,
	confirmLabel,
	onConfirm,
	onClose,
}: {
	title: string
	message: React.ReactNode
	confirmLabel: string
	onConfirm: () => void
	onClose: () => void
}) {
	const { t } = useTraslate()
	return (
		<InlineNestedDialog
			title={title}
			onClose={onClose}
			footer={
				<div className="d-flex justify-content-end gap-2">
					<button type="button" className="btn btn-secondary btn-sm" onClick={onClose}>
						{t('Cancel')}
					</button>
					<button
						type="button"
						className="btn btn-danger btn-sm"
						onClick={() => {
							onConfirm()
							onClose()
						}}
					>
						{confirmLabel}
					</button>
				</div>
			}
		>
			{message}
		</InlineNestedDialog>
	)
}

export default EntityPropertiesModalContent
