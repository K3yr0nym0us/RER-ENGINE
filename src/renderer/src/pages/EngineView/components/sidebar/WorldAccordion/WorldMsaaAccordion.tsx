import { Accordion } from 'react-bootstrap'
import { ColumnsGap } from 'react-bootstrap-icons'

import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'

const TIERS = [
	{ key: 'low' as const, labelKey: 'Low' },
	{ key: 'medium' as const, labelKey: 'Medium' },
	{ key: 'high' as const, labelKey: 'High' },
	{ key: 'ultra' as const, labelKey: 'Ultra' },
]

export default function WorldMsaaAccordion() {
	const { t } = useTraslate()
	const { engineReady, worldConfig, setMsaaTier } = useContextEngine()

	const activeTier = worldConfig.msaaTier ?? 'low'

	return (
		<Accordion.Item eventKey="world-msaa">
			<Accordion.Header>
				<ColumnsGap className="me-2" />
				{t('MSAA')}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-2">
				<p className="text-secondary mb-2" style={{ fontSize: '0.72rem' }}>
					{t('World msaa tier hint')}
				</p>

				<label className="form-label small text-secondary mb-1">{t('MSAA tier')}</label>
				<div className="d-flex flex-wrap gap-1 mb-0">
					{TIERS.map(({ key, labelKey }) => (
						<button
							key={key}
							type="button"
							className={`btn btn-sm ${
								activeTier === key ? 'btn-info' : 'btn-outline-secondary'
							}`}
							disabled={!engineReady}
							onClick={() => setMsaaTier(key)}
						>
							{t(labelKey)}
						</button>
					))}
				</div>
			</Accordion.Body>
		</Accordion.Item>
	)
}
