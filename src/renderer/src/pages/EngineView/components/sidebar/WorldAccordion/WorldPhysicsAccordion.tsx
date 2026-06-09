import { Accordion } from 'react-bootstrap'
import { ArrowDownCircle } from 'react-bootstrap-icons'

import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'
import { DEFAULT_GRAVITY_MAGNITUDE } from '@shared-types'

export default function WorldPhysicsAccordion() {
	const { t } = useTraslate()
	const { engineReady, worldConfig, setGravity } = useContextEngine()

	return (
		<Accordion.Item eventKey="world-physics">
			<Accordion.Header>
				<ArrowDownCircle className="me-2" />
				{t('Gravity')}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-2">
				<label
					className="form-label small text-secondary mb-1 d-flex justify-content-between"
					htmlFor="gravity-range"
				>
					<span>{t('Gravity')}</span>
					<span className="text-info fw-bold">
						{(worldConfig.gravity ?? DEFAULT_GRAVITY_MAGNITUDE).toFixed(2)} u/s²
					</span>
				</label>
				<input
					id="gravity-range"
					type="range"
					className="form-range mb-0"
					min={0}
					max={50}
					step={0.5}
					value={worldConfig.gravity ?? DEFAULT_GRAVITY_MAGNITUDE}
					disabled={!engineReady}
					onChange={(e) => setGravity(parseFloat(e.target.value))}
				/>
			</Accordion.Body>
		</Accordion.Item>
	)
}
