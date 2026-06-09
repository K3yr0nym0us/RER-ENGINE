import { useEffect, useState } from 'react'
import { Accordion } from 'react-bootstrap'
import { EyeFill, EyeSlashFill, Grid3x3, Lock, Unlock } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'
import type { ProjectType } from '@shared-types'

export default function WorldGridAccordion({ projectType = '2D' }: { projectType?: ProjectType }) {
	const { t } = useTraslate()
	const { engineReady, worldConfig, setGridVisible, setGridCellSize } = useContextEngine()
	const is3dProject = projectType === '3D'
	const [gridCellStr, setGridCellStr] = useState(String(worldConfig.gridCellSize))
	const [gridSizeLocked, setGridSizeLocked] = useState(false)

	useEffect(() => {
		setGridCellStr(String(worldConfig.gridCellSize))
	}, [worldConfig.gridCellSize])

	const commitGridCell = () => {
		const size = parseFloat(gridCellStr)
		if (!isNaN(size) && size > 0) {
			setGridCellSize(size)
		}
	}

	const handleGridCellKey = (e: React.KeyboardEvent) => {
		if (e.key === 'Enter') commitGridCell()
	}

	const handleGridCellChange = (rawValue: string) => {
		setGridCellStr(rawValue)
		const size = parseFloat(rawValue)
		if (!isNaN(size) && size > 0) {
			setGridCellSize(size)
		}
	}

	return (
		<Accordion.Item eventKey="world-grid">
			<Accordion.Header>
				<Grid3x3 className="me-2" />
				{t('Grid')}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-2">
				{!is3dProject && (
					<div className="d-flex align-items-center justify-content-end mb-2">
						<AppTooltip content={worldConfig.gridVisible ? t('Hide grid') : t('Show grid')} place="top">
							<button
								className={`btn btn-sm ${worldConfig.gridVisible ? 'btn-info' : 'btn-outline-secondary'}`}
								disabled={!engineReady}
								onClick={() => setGridVisible(!worldConfig.gridVisible)}
							>
								{worldConfig.gridVisible ? <EyeFill /> : <EyeSlashFill />}
							</button>
						</AppTooltip>
					</div>
				)}

				{is3dProject && (
					<p className="text-secondary mb-2" style={{ fontSize: '0.72rem' }}>
						{t('Cell size aligns ground checker and object placement snap (Ctrl).')}
					</p>
				)}

				<div className="form-label small text-secondary mb-1 d-flex align-items-center justify-content-between gap-2">
					<span>{t('Cell size')}</span>
					<div className="d-flex align-items-center gap-2">
						<input
							id="grid-cell-size-number"
							type="number"
							className="form-control form-control-sm bg-dark text-light border-secondary"
							style={{ width: 55 }}
							min={0.05}
							step={0.01}
							value={gridCellStr}
							disabled={!engineReady || gridSizeLocked}
							onChange={(e) => handleGridCellChange(e.target.value)}
							onBlur={commitGridCell}
							onKeyDown={handleGridCellKey}
							aria-label={t('Exact cell size')}
						/>
						<AppTooltip content={gridSizeLocked ? t('Unlock grid size') : t('Lock grid size')} place="top">
							<button
								type="button"
								className={`btn btn-sm ${gridSizeLocked ? 'btn-info' : 'btn-outline-secondary'}`}
								onClick={() => setGridSizeLocked((v) => !v)}
								aria-pressed={gridSizeLocked}
								disabled={!engineReady}
							>
								{gridSizeLocked ? <Lock size={13} /> : <Unlock size={13} />}
							</button>
						</AppTooltip>
					</div>
				</div>
				<input
					id="grid-cell-size-range"
					type="range"
					className="form-range mb-0"
					min={0.25}
					max={10}
					step={0.25}
					value={worldConfig.gridCellSize}
					disabled={!engineReady || gridSizeLocked}
					onChange={(e) => {
						setGridCellStr(e.target.value)
						setGridCellSize(parseFloat(e.target.value))
					}}
				/>
			</Accordion.Body>
		</Accordion.Item>
	)
}
