import { useEffect, useState } from 'react'

import { CheckLg } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { useTraslate } from '@hooks'
import type {
	EmbeddedTextureVariant,
	EntityMaterialTextures,
	EntityPropertiesAction,
	GraphicsTextureTier,
} from '../../../../../modal-electron/entityPropertiesTypes'

const TIERS: { key: GraphicsTextureTier; labelKey: string }[] = [
	{ key: 'low', labelKey: 'Low' },
	{ key: 'medium', labelKey: 'Medium' },
	{ key: 'high', labelKey: 'High' },
	{ key: 'ultra', labelKey: 'Ultra' },
]

function hasDuplicateSizes(variants: EmbeddedTextureVariant[]): boolean {
	const seen = new Set<string>()
	for (const v of variants) {
		const key = `${v.width}x${v.height}`
		if (seen.has(key)) return true
		seen.add(key)
	}
	return false
}

function variantLabel(
	imageIndex: number,
	width: number,
	height: number,
	variants: EmbeddedTextureVariant[],
): string {
	const size = `${width}×${height}`
	if (!hasDuplicateSizes(variants)) return size
	return `#${imageIndex} · ${size}`
}

function defaultImageIndexForTier(mat: EntityMaterialTextures, tier: GraphicsTextureTier): number | undefined {
	const assigned = mat.tierImageIndex[tier]
	if (assigned !== undefined) return assigned
	return mat.variants[0]?.imageIndex
}

export interface EntityTexturesPanelProps {
	entityId: number
	materials: EntityMaterialTextures[] | null
	texturesLoaded: boolean
	/** Nivel gráfico activo en el motor (Rhai / nodos / juego). */
	activeGraphicsTextureTier: GraphicsTextureTier
	onAction: (action: EntityPropertiesAction) => void
}

export function EntityTexturesPanel({
	entityId,
	materials,
	texturesLoaded,
	activeGraphicsTextureTier,
	onAction,
}: EntityTexturesPanelProps) {
	const { t } = useTraslate()
	const [assignmentTier, setAssignmentTier] = useState<GraphicsTextureTier>(activeGraphicsTextureTier)
	const [pendingSelection, setPendingSelection] = useState<Record<number, number>>({})

	useEffect(() => {
		setAssignmentTier(activeGraphicsTextureTier)
	}, [entityId, activeGraphicsTextureTier])

	useEffect(() => {
		if (!materials) {
			setPendingSelection({})
			return
		}
		const next: Record<number, number> = {}
		for (const mat of materials) {
			const imageIndex = defaultImageIndexForTier(mat, assignmentTier)
			if (imageIndex !== undefined) {
				next[mat.materialIndex] = imageIndex
			}
		}
		setPendingSelection(next)
	}, [materials, assignmentTier, entityId])

	const handleApply = (materialIndex: number) => {
		const imageIndex = pendingSelection[materialIndex]
		if (imageIndex === undefined) return
		onAction({
			action: 'send',
			cmd: {
				cmd: 'set_entity_texture_lod',
				id: entityId,
				material_index: materialIndex,
				tier: assignmentTier,
				image_index: imageIndex,
			},
		})
	}

	if (!texturesLoaded) {
		return (
			<p className="text-secondary small mb-0">
				{t('Loading embedded textures…')}
			</p>
		)
	}

	if (!materials || materials.length === 0) {
		return (
			<p className="text-secondary small mb-0">
				{t('No embedded textures')}
			</p>
		)
	}

	return (
		<div className="entity-textures-panel">
			<div className="mb-3">
				<label className="form-label text-light small mb-1">{t('Texture level')}</label>
				<div className="d-flex flex-wrap gap-1">
					{TIERS.map(({ key, labelKey }) => (
						<button
							key={key}
							type="button"
							className={`btn btn-sm ${assignmentTier === key ? 'btn-info' : 'btn-outline-secondary'}`}
							onClick={() => {
								setAssignmentTier(key)
								onAction({
									action: 'send',
									cmd: {
										cmd: 'set_entity_texture_preview_tier',
										id: entityId,
										tier: key,
									},
								})
							}}
						>
							{t(labelKey)}
						</button>
					))}
				</div>
			</div>

			<div className="row g-2">
				{materials.map((mat) => {
					const selectedIndex = pendingSelection[mat.materialIndex]
					const hasVariants = mat.variants.length > 0

					return (
						<div key={mat.materialIndex} className="col-6 entity-textures-row">
							<label className="form-label text-light small mb-1 text-truncate d-block">
								{mat.materialName}
							</label>
							<div className="d-flex align-items-center gap-1">
								<select
									className="form-select form-select-sm flex-grow-1"
									value={selectedIndex ?? ''}
									disabled={!hasVariants}
									onChange={(e) => {
										const imageIndex = Number(e.target.value)
										if (!Number.isFinite(imageIndex)) return
										setPendingSelection((prev) => ({
											...prev,
											[mat.materialIndex]: imageIndex,
										}))
									}}
								>
									{!hasVariants ? (
										<option value="">{t('No embedded textures')}</option>
									) : (
										mat.variants.map((v) => (
											<option key={v.imageIndex} value={v.imageIndex}>
												{variantLabel(
													v.imageIndex,
													v.width,
													v.height,
													mat.variants,
												)}
											</option>
										))
									)}
								</select>
								<AppTooltip content={t('Apply texture')} place="top">
									<button
										type="button"
										className="btn btn-sm btn-outline-success flex-shrink-0 px-2"
										disabled={!hasVariants || selectedIndex === undefined}
										onClick={() => handleApply(mat.materialIndex)}
										aria-label={t('Apply texture')}
									>
										<CheckLg className="text-success" />
									</button>
								</AppTooltip>
							</div>
						</div>
					)
				})}
			</div>
		</div>
	)
}
