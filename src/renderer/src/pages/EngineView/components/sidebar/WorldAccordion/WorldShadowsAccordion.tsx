import { Accordion } from 'react-bootstrap'
import { Moon } from 'react-bootstrap-icons'

import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'
import { DEFAULT_SHADOW_DARKNESS } from '@shared-types'

const TIERS = [
	{ key: 'off' as const, labelKey: 'Off' },
	{ key: 'low' as const, labelKey: 'Low' },
	{ key: 'medium' as const, labelKey: 'Medium' },
	{ key: 'high' as const, labelKey: 'High' },
	{ key: 'ultra' as const, labelKey: 'Ultra' },
]

export default function WorldShadowsAccordion() {
	const { t } = useTraslate()
	const { engineReady, worldConfig, setShadowTier, setDirectionalLight } = useContextEngine()

	const activeTier = worldConfig.shadowTier ?? 'low'

	return (
		<Accordion.Item eventKey="world-shadows">
			<Accordion.Header>
				<Moon className="me-2" />
				{t('Shadows')}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-2">
				<p className="text-secondary mb-2" style={{ fontSize: '0.72rem' }}>
					{t('World shadows tier hint')}
				</p>

				<label className="form-label small text-secondary mb-1">{t('Shadow tier')}</label>
				<div className="d-flex flex-wrap gap-1 mb-2">
					{TIERS.map(({ key, labelKey }) => (
						<button
							key={key}
							type="button"
							className={`btn btn-sm ${
								activeTier === key ? 'btn-info' : 'btn-outline-secondary'
							}`}
							disabled={!engineReady}
							onClick={() => setShadowTier(key)}
						>
							{t(labelKey)}
						</button>
					))}
				</div>

				<label
					className="form-label small text-secondary mb-1 d-flex justify-content-between"
					htmlFor="shadow-darkness-range"
				>
					<span>{t('Shadow darkness')}</span>
					<span className="text-info fw-bold">
						{(worldConfig.shadowDarkness ?? DEFAULT_SHADOW_DARKNESS).toFixed(2)}
					</span>
				</label>
				<input
					id="shadow-darkness-range"
					type="range"
					className="form-range mb-0"
					min={0.02}
					max={0.85}
					step={0.01}
					value={worldConfig.shadowDarkness ?? DEFAULT_SHADOW_DARKNESS}
					disabled={!engineReady}
					onChange={(e) => setDirectionalLight({ shadowDarkness: parseFloat(e.target.value) })}
				/>
			</Accordion.Body>
		</Accordion.Item>
	)
}
