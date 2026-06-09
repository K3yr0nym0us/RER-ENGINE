import { useEffect, useState } from 'react'
import { Accordion } from 'react-bootstrap'
import { AspectRatio } from 'react-bootstrap-icons'

import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'
import type { ProjectType } from '@shared-types'

export default function WorldWorkspaceAccordion({ projectType = '2D' }: { projectType?: ProjectType }) {
	const { t } = useTraslate()
	const { engineReady, worldConfig, setWorldSize } = useContextEngine()
	const is3dProject = projectType === '3D'
	const [widthStr, setWidthStr] = useState(String(worldConfig.worldWidth))
	const [heightStr, setHeightStr] = useState(String(worldConfig.worldHeight))
	const [depthStr, setDepthStr] = useState(String(worldConfig.worldDepth))

	useEffect(() => {
		setWidthStr(String(worldConfig.worldWidth))
		setHeightStr(String(worldConfig.worldHeight))
		setDepthStr(String(worldConfig.worldDepth))
	}, [worldConfig.worldWidth, worldConfig.worldHeight, worldConfig.worldDepth])

	const commitSize = () => {
		const w = parseFloat(widthStr)
		const h = parseFloat(heightStr)
		const d = parseFloat(depthStr)
		const hasValid2dSize = !isNaN(w) && !isNaN(h) && w > 0 && h > 0
		const hasValid3dSize = hasValid2dSize && !isNaN(d) && d > 0
		if (!is3dProject && hasValid2dSize) {
			setWorldSize(w, h)
		} else if (is3dProject && hasValid3dSize) {
			setWorldSize(w, h, d)
		}
	}

	const handleKey = (e: React.KeyboardEvent) => {
		if (e.key === 'Enter') commitSize()
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
							onBlur={commitSize}
							onKeyDown={handleKey}
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
							onBlur={commitSize}
							onKeyDown={handleKey}
						/>
					</div>
					{is3dProject && (
						<div className="flex-fill">
							<label className="form-label small text-secondary mb-0" htmlFor="world-depth">
								{t('Depth (u)')}
							</label>
							<input
								id="world-depth"
								type="number"
								className="form-control form-control-sm bg-dark text-light border-secondary"
								min={1}
								step={1}
								value={depthStr}
								disabled={!engineReady}
								onChange={(e) => setDepthStr(e.target.value)}
								onBlur={commitSize}
								onKeyDown={handleKey}
							/>
						</div>
					)}
				</div>
			</Accordion.Body>
		</Accordion.Item>
	)
}
