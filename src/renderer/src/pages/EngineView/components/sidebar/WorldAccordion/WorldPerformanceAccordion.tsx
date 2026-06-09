import { useEffect, useState } from 'react'
import { Accordion } from 'react-bootstrap'
import { CheckLg, Speedometer2 } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'

const FPS_OPTIONS = [60, 120, 144, 240] as const

export default function WorldPerformanceAccordion() {
	const { t } = useTraslate()
	const { engineReady, worldConfig, setTargetFps } = useContextEngine()
	const [selectedTargetFps, setSelectedTargetFps] = useState(String(worldConfig.targetFps))

	useEffect(() => {
		setSelectedTargetFps(String(worldConfig.targetFps))
	}, [worldConfig.targetFps])

	const handleApplyTargetFps = () => {
		const parsed = Number.parseInt(selectedTargetFps, 10)
		if (!Number.isFinite(parsed) || parsed < 1 || parsed > 1000) return
		setTargetFps(parsed)
	}

	return (
		<Accordion.Item eventKey="world-performance">
			<Accordion.Header>
				<Speedometer2 className="me-2" />
				{t('FPS limit')}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-2">
				<div className="d-flex gap-1">
					<select
						className="form-select form-select-sm bg-dark text-light border-secondary flex-fill"
						value={selectedTargetFps}
						disabled={!engineReady}
						onChange={(e) => setSelectedTargetFps(e.target.value)}
					>
						{FPS_OPTIONS.map((fps) => (
							<option key={fps} value={fps}>
								{fps} {t('FPS')}
							</option>
						))}
					</select>
					<AppTooltip content={t('Apply FPS limit')} place="top">
						<button
							className="btn btn-sm btn-outline-info"
							disabled={!engineReady}
							onClick={handleApplyTargetFps}
						>
							<CheckLg />
						</button>
					</AppTooltip>
				</div>
			</Accordion.Body>
		</Accordion.Item>
	)
}
