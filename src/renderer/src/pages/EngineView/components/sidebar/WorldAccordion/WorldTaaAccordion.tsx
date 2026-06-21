import { Accordion } from 'react-bootstrap'
import { ShieldCheck } from 'react-bootstrap-icons'

import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'

export default function WorldTaaAccordion() {
	const { t } = useTraslate()
	const { engineReady, worldConfig, setTaaEnabled } = useContextEngine()

	return (
		<Accordion.Item eventKey="world-taa">
			<Accordion.Header>
				<ShieldCheck className="me-2" />
				{t('TAA')}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-2">
				<p className="text-secondary mb-2" style={{ fontSize: '0.72rem' }}>
					{t('Temporal antialiasing smooths edges and stabilizes shadows across frames.')}
				</p>

				<label className="form-label small text-secondary mb-1">{t('TAA')}</label>
				<div className="d-flex flex-wrap gap-1 mb-1">
					<button
						type="button"
						className={`btn btn-sm ${worldConfig.taaEnabled ? 'btn-info' : 'btn-outline-secondary'}`}
						disabled={!engineReady}
						onClick={() => setTaaEnabled(true)}
					>
						{t('On')}
					</button>
					<button
						type="button"
						className={`btn btn-sm ${!worldConfig.taaEnabled ? 'btn-info' : 'btn-outline-secondary'}`}
						disabled={!engineReady}
						onClick={() => setTaaEnabled(false)}
					>
						{t('Off')}
					</button>
				</div>
			</Accordion.Body>
		</Accordion.Item>
	)
}
