import { useEffect } from 'react'

import { Trash } from 'react-bootstrap-icons'

import type { EntityBonePhysics3D } from '@shared-types'
import type {
	EntityPropertiesAction,
	EntityPropertiesBonePhysicsUi,
} from '../../../../../modal-electron/entityPropertiesTypes'
import { useTraslate } from '@hooks'

const MODE_OPTIONS: EntityBonePhysics3D['mode'][] = [
	'none',
	'inherit',
	'static',
	'dynamic',
	'kinematic',
]

function modeLabelKey(mode: EntityBonePhysics3D['mode']): string {
	switch (mode) {
		case 'none':
			return 'None'
		case 'inherit':
			return 'Inherit from entity'
		case 'static':
			return 'Static'
		case 'dynamic':
			return 'Dynamic'
		case 'kinematic':
			return 'Kinematic'
	}
}

export interface EntityPropertiesBonesPanelProps {
	bonePhysics: EntityPropertiesBonePhysicsUi
	onAction: (action: EntityPropertiesAction) => void | Promise<void>
}

export function EntityPropertiesBonesPanel({ bonePhysics, onAction }: EntityPropertiesBonesPanelProps) {
	const { t } = useTraslate()
	const { entries, selectedBoneName, draftMode, bonePickActive } = bonePhysics
	const isEditing = bonePickActive || Boolean(selectedBoneName)

	useEffect(() => {
		onAction({ action: 'requestBonePhysicsList' })
		// eslint-disable-next-line react-hooks/exhaustive-deps -- al abrir la pestaña
	}, [])

	const toggleBonePick = () => onAction({ action: 'setBonePickMode', active: !bonePickActive })

	return (
		<div>
			{isEditing ? (
				<div className="border border-secondary rounded p-2 mb-3">
					<label className="form-label small mb-1">{t('Bone')}</label>
					<div className="input-group input-group-sm mb-2">
						<input
							type="text"
							className="form-control"
							value={selectedBoneName ?? ''}
							readOnly
							placeholder={t('No bone selected')}
						/>
						<button
							type="button"
							className={`btn ${bonePickActive ? 'btn-warning' : 'btn-outline-primary'}`}
							onClick={toggleBonePick}
						>
							{bonePickActive ? t('Cancel bone selection') : t('Select bone')}
						</button>
					</div>
					{bonePickActive && (
						<p className="text-warning small mb-2">
							{t('Click a bone in the viewport to assign bone physics.')}
						</p>
					)}
					{selectedBoneName && (
						<>
							<label className="form-label small mb-1" htmlFor="entity-bone-physics-mode">
								{t('Physics type')}
							</label>
							<select
								id="entity-bone-physics-mode"
								className="form-select form-select-sm mb-2"
								value={draftMode}
								onChange={(e) =>
									onAction({
										action: 'setBoneDraftMode',
										mode: e.target.value as EntityBonePhysics3D['mode'],
									})
								}
							>
								{MODE_OPTIONS.map((mode) => (
									<option key={mode} value={mode}>
										{t(modeLabelKey(mode))}
									</option>
								))}
							</select>
							<button
								type="button"
								className="btn btn-sm btn-primary w-100"
								onClick={() => onAction({ action: 'applyBonePhysics' })}
							>
								{t('Apply')}
							</button>
						</>
					)}
				</div>
			) : (
				<button
					type="button"
					className="btn btn-sm btn-outline-primary w-100 mb-3"
					onClick={toggleBonePick}
				>
					{t('Select bone')}
				</button>
			)}

			{entries.length > 0 && (
				<div>
					<p className="small text-secondary mb-2">{t('Configured bones')}</p>
					<ul className="list-group list-group-flush mb-0">
						{entries.map((entry) => (
							<li
								key={entry.bone_name}
								className="list-group-item bg-transparent text-light border-secondary px-0 py-2"
							>
								<div className="d-flex align-items-center gap-2">
									<div
										className="small fw-semibold text-truncate flex-shrink-0"
										style={{ width: '40%' }}
										title={entry.bone_name}
									>
										{entry.bone_name}
									</div>
									<select
										className="form-select form-select-sm flex-grow-1 min-w-0"
										value={entry.mode}
										aria-label={`${t('Physics type')}: ${entry.bone_name}`}
										onChange={(e) =>
											onAction({
												action: 'setBoneEntryMode',
												boneName: entry.bone_name,
												mode: e.target.value as EntityBonePhysics3D['mode'],
											})
										}
									>
										{MODE_OPTIONS.map((mode) => (
											<option key={mode} value={mode}>
												{t(modeLabelKey(mode))}
											</option>
										))}
									</select>
									<button
										type="button"
										className="btn btn-sm btn-outline-danger flex-shrink-0"
										title={t('Remove')}
										onClick={() =>
											onAction({ action: 'removeBonePhysics', boneName: entry.bone_name })
										}
									>
										<Trash />
									</button>
								</div>
							</li>
						))}
					</ul>
				</div>
			)}
		</div>
	)
}
