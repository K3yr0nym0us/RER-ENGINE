import { Accordion, Form } from 'react-bootstrap'
import { ShieldCheck } from 'react-bootstrap-icons'

import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'

export default function WorldTaaAccordion() {
	const { t } = useTraslate()
	const { engineReady, worldConfig, setTaaEnabled, setTaaParams } = useContextEngine()

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
				<div className="d-flex flex-wrap gap-1 mb-3">
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

				<div className="mb-2">
					<label className="form-label small text-secondary mb-0">
						{t('Blend')} <span className="text-info">{worldConfig.taaBlend.toFixed(2)}</span>
					</label>
					<Form.Range
						min={0}
						max={1}
						step={0.01}
						value={worldConfig.taaBlend}
						disabled={!engineReady}
						onChange={(e) => setTaaParams({ blend: Number(e.target.value), jitterScale: worldConfig.taaJitterScale, enabled: worldConfig.taaEnabled })}
					/>
					<div className="d-flex justify-content-between text-secondary" style={{ fontSize: '0.65rem' }}>
						<span>{t('Sharp')}</span>
						<span>{t('Smooth')}</span>
					</div>
				</div>

				<div className="mb-1">
					<label className="form-label small text-secondary mb-0">
						{t('Jitter')} <span className="text-info">{worldConfig.taaJitterScale.toFixed(2)}</span>
					</label>
					<Form.Range
						min={0}
						max={2}
						step={0.05}
						value={worldConfig.taaJitterScale}
						disabled={!engineReady}
						onChange={(e) => setTaaParams({ blend: worldConfig.taaBlend, jitterScale: Number(e.target.value), enabled: worldConfig.taaEnabled })}
					/>
					<div className="d-flex justify-content-between text-secondary" style={{ fontSize: '0.65rem' }}>
						<span>{t('Less')}</span>
						<span>{t('More')}</span>
					</div>
				</div>
			</Accordion.Body>
		</Accordion.Item>
	)
}
