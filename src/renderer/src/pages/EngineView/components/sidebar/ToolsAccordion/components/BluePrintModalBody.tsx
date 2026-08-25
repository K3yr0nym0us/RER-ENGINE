import { useState, useRef, useEffect } from 'react'

import { Nav } from 'react-bootstrap'
import { Grid3x3GapFill, TrashFill, BoxSeam } from 'react-bootstrap-icons'
import { useContextEngine } from '@engine'
import { useModalClose } from '../../../../../../modal-electron/useModalClose'
import { InlineNestedDialog } from '../../../../../../modal-electron/InlineNestedDialog'
import { useQuickBuild } from '../../../../../../context/QuickBuildContext'
import { useSpritePreviewImage } from '@hooks'
import type { BlueprintTabCategory, BluePrintEntry } from '@shared-types'
import { useTraslate } from '@hooks'
import { blueprintUsesModel3D, resolveBlueprintCategory, resolveBlueprintModelPath } from '../../../../../../utils/blueprintModelPath'
import {
	deleteBlueprintKeepEntities,
	deleteBlueprintWithEntities,
	type BluePrintModalActionDeps,
} from './bluePrintModalActions'

export interface BluePrintModalContentProps {
	blueprints: BluePrintEntry[]
	onSelect: (bp: BluePrintEntry) => void
	onDeleteWithEntities: (bp: BluePrintEntry) => void
	onDeleteKeepEntities: (bp: BluePrintEntry) => void
	getLinkedEntityCount: (blueprintId: string) => number
}

export function BluePrintModalContent({
	blueprints,
	onSelect,
	onDeleteWithEntities,
	onDeleteKeepEntities,
	getLinkedEntityCount,
}: BluePrintModalContentProps) {
	const { t } = useTraslate()
	const [activeCategory, setActiveCategory] = useState<BlueprintTabCategory>('character')
	const [pendingDelete, setPendingDelete] = useState<BluePrintEntry | null>(null)

	const filtered = blueprints.filter((bp) => resolveBlueprintCategory(bp) === activeCategory)
	const linkedCount = pendingDelete ? getLinkedEntityCount(pendingDelete.id) : 0

	const handleDeleteRequest = (bp: BluePrintEntry) => {
		setPendingDelete(bp)
	}

	const handleDeleteWithEntities = () => {
		if (!pendingDelete) return
		onDeleteWithEntities(pendingDelete)
		setPendingDelete(null)
	}

	const handleDeleteKeepEntities = () => {
		if (!pendingDelete) return
		onDeleteKeepEntities(pendingDelete)
		setPendingDelete(null)
	}

	return (
		<div className="position-relative">
			<div>
				<Nav
					variant="tabs"
					className="mb-3"
					activeKey={activeCategory}
					onSelect={(k) => setActiveCategory((k ?? 'character') as BlueprintTabCategory)}
				>
					<Nav.Item>
						<Nav.Link eventKey="character">{t('Character')}</Nav.Link>
					</Nav.Item>
					<Nav.Item>
						<Nav.Link eventKey="environment">{t('Environment')}</Nav.Link>
					</Nav.Item>
					<Nav.Item>
						<Nav.Link eventKey="object">{t('Objects')}</Nav.Link>
					</Nav.Item>
				</Nav>

				{filtered.length === 0 ? (
					<p className="text-secondary fst-italic small text-center py-4 mb-0">
						{t('No blueprints in this category')}
					</p>
				) : (
					<>
						<p className="text-secondary small mb-2">
							{t('Select a blueprint to activate quick build mode.')}
						</p>
						<div className="d-flex flex-wrap gap-2">
							{filtered.map((bp) => (
								<BluePrintCard
									key={bp.id}
									bp={bp}
									onSelect={onSelect}
									onDeleteRequest={handleDeleteRequest}
								/>
							))}
						</div>
					</>
				)}
			</div>

			{pendingDelete !== null && (
				<InlineNestedDialog
					title={t('Delete blueprint')}
					onClose={() => setPendingDelete(null)}
				>
					<p className="mb-2">
						{t('This action will delete the blueprint')}{' '}
						<strong>{pendingDelete.name}</strong>.
					</p>
					{linkedCount > 0 ? (
						<>
							<p className="mb-4">
								{t('There are')} <strong>{linkedCount}</strong>{' '}
								{t('entities based on this blueprint. What do you want to do with them?')}
							</p>
							<div className="d-flex flex-column gap-2">
								<button className="btn btn-danger" onClick={handleDeleteWithEntities}>
									{t('Delete all entities')} ({linkedCount})
								</button>
								<button className="btn btn-warning text-dark" onClick={handleDeleteKeepEntities}>
									{t('Convert to standalone entities')}
								</button>
								<button className="btn btn-secondary" onClick={() => setPendingDelete(null)}>
									{t('Cancel')}
								</button>
							</div>
						</>
					) : (
						<>
							<p className="text-secondary small mb-4">{t('Cannot be undone.')}</p>
							<div className="d-flex justify-content-end gap-2">
								<button className="btn btn-secondary" onClick={() => setPendingDelete(null)}>
									{t('Cancel')}
								</button>
								<button className="btn btn-danger" onClick={handleDeleteWithEntities}>
									{t('Delete')}
								</button>
							</div>
						</>
					)}
				</InlineNestedDialog>
			)}
		</div>
	)
}

export function BluePrintModalBody() {
	const {
		blueprints,
		setBlueprints,
		entityMetaRef,
		entityTransformsRef,
		removeScenario,
		removeCharacter,
		removeEntity,
		removeCollider,
		removeExecutionArea,
	} = useContextEngine()
	const { activeBluePrint, setActiveBluePrint } = useQuickBuild()
	const closeModal = useModalClose()

	const deps: BluePrintModalActionDeps = {
		blueprints,
		setBlueprints,
		activeBluePrint,
		setActiveBluePrint,
		entityMetaRef,
		entityTransformsRef,
		removeScenario,
		removeCharacter,
		removeEntity,
		removeCollider,
		removeExecutionArea,
	}

	const getLinkedEntityCount = (bpId: string) =>
		Object.entries(entityMetaRef.current)
			.filter(([, meta]) => meta.blueprintId === bpId)
			.length

	return (
		<BluePrintModalContent
			blueprints={blueprints}
			getLinkedEntityCount={getLinkedEntityCount}
			onSelect={(bp) => {
				setActiveBluePrint(bp)
				closeModal()
			}}
			onDeleteWithEntities={(bp) => {
				deleteBlueprintWithEntities(bp, deps)
			}}
			onDeleteKeepEntities={(bp) => {
				deleteBlueprintKeepEntities(bp, deps)
			}}
		/>
	)
}

// ---------------------------------------------------------------------------
// Tarjeta individual - muestra solo el primer frame como preview
// ---------------------------------------------------------------------------

const PREVIEW_SIZE = 48

function BluePrintCard({
	bp,
	onSelect,
	onDeleteRequest,
}: {
	bp: BluePrintEntry
	onSelect: (bp: BluePrintEntry) => void
	onDeleteRequest: (bp: BluePrintEntry) => void
}) {
	const { t } = useTraslate()
	const canvasRef = useRef<HTMLCanvasElement>(null)

	const firstFrame = bp.animations?.[0]?.frames?.[0]
	const framePath = firstFrame?.path ?? resolveBlueprintModelPath(bp)
	const isModel3D = blueprintUsesModel3D(bp)

	const { imageSrc } = useSpritePreviewImage(isModel3D ? '' : framePath)

	useEffect(() => {
		if (isModel3D) return
		const canvas = canvasRef.current
		if (!canvas || !imageSrc) return
		const ctx = canvas.getContext('2d')
		if (!ctx) return

		const img = new window.Image()
		img.onload = () => {
			ctx.clearRect(0, 0, PREVIEW_SIZE, PREVIEW_SIZE)

			const hasCrop = firstFrame?.src_w != null && firstFrame?.src_h != null
			if (hasCrop && firstFrame) {
				const { src_x = 0, src_y = 0, src_w = img.width, src_h = img.height } = firstFrame
				ctx.drawImage(img, src_x, src_y, src_w, src_h, 0, 0, PREVIEW_SIZE, PREVIEW_SIZE)
			} else {
				const scale = Math.min(PREVIEW_SIZE / img.width, PREVIEW_SIZE / img.height)
				const dw = img.width * scale
				const dh = img.height * scale
				ctx.drawImage(img, (PREVIEW_SIZE - dw) / 2, (PREVIEW_SIZE - dh) / 2, dw, dh)
			}
		}
		img.src = imageSrc
	}, [imageSrc, firstFrame, isModel3D])

	return (
		<div className="position-relative" style={{ width: 80, height: 80 }}>
			<button
				className="btn btn-outline-secondary d-flex flex-column align-items-center justify-content-center gap-1 p-1"
				style={{ width: 80, height: 80, borderRadius: 8, overflow: 'hidden' }}
				title={bp.name}
				onClick={() => onSelect(bp)}
			>
				{isModel3D ? (
					<BoxSeam size={24} className="flex-shrink-0" />
				) : framePath && imageSrc ? (
					<canvas
						ref={canvasRef}
						width={PREVIEW_SIZE}
						height={PREVIEW_SIZE}
						style={{ flexShrink: 0, imageRendering: 'pixelated' }}
					/>
				) : (
					<Grid3x3GapFill size={24} className="flex-shrink-0" />
				)}
				<span style={{ fontSize: 10, lineHeight: 1.2 }} className="text-truncate w-100 text-center">
					{bp.name}
				</span>
			</button>

			<button
				type="button"
				className="btn btn-sm btn-danger position-absolute d-flex align-items-center justify-content-center"
				style={{ top: 4, right: 4, width: 20, height: 20, borderRadius: 999, padding: 0 }}
				title={t('Delete blueprint')}
				onClick={(e) => {
					e.stopPropagation()
					onDeleteRequest(bp)
				}}
			>
				<TrashFill size={10} />
			</button>
		</div>
	)
}
