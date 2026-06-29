import { useEffect, useState } from 'react'
import { Accordion } from 'react-bootstrap'
import { AspectRatio } from 'react-bootstrap-icons'

import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'
import type { ProjectType } from '@shared-types'

const MIN_WORLD_RADIUS_3D = 5
const MAX_WORLD_RADIUS_3D = 500

export default function WorldWorkspaceAccordion({ projectType = '2D' }: { projectType?: ProjectType }) {
	const { t } = useTraslate()
	const { engineReady, worldConfig, setWorldSize, setWorldRadius } = useContextEngine()
	const is3dProject = projectType === '3D'
	const [widthStr, setWidthStr] = useState(String(worldConfig.worldWidth))
	const [heightStr, setHeightStr] = useState(String(worldConfig.worldHeight))
	const [radiusStr, setRadiusStr] = useState(String(worldConfig.worldRadius))

	useEffect(() => {
		setWidthStr(String(worldConfig.worldWidth))
		setHeightStr(String(worldConfig.worldHeight))
		setRadiusStr(String(worldConfig.worldRadius))
	}, [worldConfig.worldWidth, worldConfig.worldHeight, worldConfig.worldRadius])

	const commitSize2d = () => {
		const w = parseFloat(widthStr)
		const h = parseFloat(heightStr)
		if (!isNaN(w) && !isNaN(h) && w > 0 && h > 0) {
			setWorldSize(w, h)
		}
	}

	const commitRadius = (raw: number) => {
		if (!Number.isFinite(raw)) return
		const radius = Math.max(MIN_WORLD_RADIUS_3D, Math.min(MAX_WORLD_RADIUS_3D, raw))
		setRadiusStr(String(radius))
		setWorldRadius(radius)
	}

	const commitRadiusFromInput = () => {
		commitRadius(parseFloat(radiusStr))
	}

	const handleKey2d = (e: React.KeyboardEvent) => {
		if (e.key === 'Enter') commitSize2d()
	}

	const handleKey3d = (e: React.KeyboardEvent) => {
		if (e.key === 'Enter') commitRadiusFromInput()
	}

	if (is3dProject) {
		const radius = worldConfig.worldRadius
		return (
			<Accordion.Item eventKey="world-workspace">
				<Accordion.Header>
					<AspectRatio className="me-2" />
					{t('Workspace')}
				</Accordion.Header>
				<Accordion.Body className="py-2 px-2">
					<label
						className="form-label small text-secondary mb-1 d-flex justify-content-between"
						htmlFor="world-radius-range"
					>
						<span>{t('Sphere radius (u)')}</span>
						<span className="text-info fw-bold">{radius.toFixed(1)} u</span>
					</label>
					<input
						id="world-radius-range"
						type="range"
						className="form-range mb-2"
						min={MIN_WORLD_RADIUS_3D}
						max={MAX_WORLD_RADIUS_3D}
						step={1}
						value={radius}
						disabled={!engineReady}
						onChange={(e) => commitRadius(parseFloat(e.target.value))}
					/>
					<label className="form-label small text-secondary mb-0" htmlFor="world-radius">
						{t('Radius (u)')}
					</label>
					<input
						id="world-radius"
						type="number"
						className="form-control form-control-sm bg-dark text-light border-secondary"
						min={MIN_WORLD_RADIUS_3D}
						max={MAX_WORLD_RADIUS_3D}
						step={1}
						value={radiusStr}
						disabled={!engineReady}
						onChange={(e) => setRadiusStr(e.target.value)}
						onBlur={commitRadiusFromInput}
						onKeyDown={handleKey3d}
					/>
				</Accordion.Body>
			</Accordion.Item>
		)
	}

	return (
		<Accordion.Item eventKey="world-workspace">
			<Accordion.Header>
				<AspectRatio className="me-2" />
				{t('Workspace')}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-2">
				<div className="d-flex gap-1">
					<div className="flex-fill">
						<label className="form-label small text-secondary mb-0" htmlFor="world-width">
							{t('Width (u)')}
						</label>
						<input
							id="world-width"
							type="number"
							className="form-control form-control-sm bg-dark text-light border-secondary"
							min={1}
							step={1}
							value={widthStr}
							disabled={!engineReady}
							onChange={(e) => setWidthStr(e.target.value)}
							onBlur={commitSize2d}
							onKeyDown={handleKey2d}
						/>
					</div>
					<div className="flex-fill">
						<label className="form-label small text-secondary mb-0" htmlFor="world-height">
							{t('Height (u)')}
						</label>
						<input
							id="world-height"
							type="number"
							className="form-control form-control-sm bg-dark text-light border-secondary"
							min={1}
							step={1}
							value={heightStr}
							disabled={!engineReady}
							onChange={(e) => setHeightStr(e.target.value)}
							onBlur={commitSize2d}
							onKeyDown={handleKey2d}
						/>
					</div>
				</div>
			</Accordion.Body>
		</Accordion.Item>
	)
}
