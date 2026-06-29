import { Accordion } from 'react-bootstrap'
import { Sun } from 'react-bootstrap-icons'

import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'
import { DEFAULT_LIGHT_AMBIENT, DEFAULT_LIGHT_INTENSITY } from '@shared-types'

export default function WorldLightingAccordion() {
	const { t } = useTraslate()
	const { engineReady, worldConfig, setDirectionalLight } = useContextEngine()

	return (
		<Accordion.Item eventKey="world-lighting">
			<Accordion.Header>
				<Sun className="me-2" />
				{t('Sun and lighting')}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-2">
				<p className="text-secondary mb-2" style={{ fontSize: '0.72rem' }}>
					{t('Move the sun gizmo in the viewport. These values tune brightness.')}
				</p>

				<label
					className="form-label small text-secondary mb-1 d-flex justify-content-between"
					htmlFor="light-ambient-range"
				>
					<span>{t('Ambient light')}</span>
					<span className="text-info fw-bold">
						{(worldConfig.lightAmbient ?? DEFAULT_LIGHT_AMBIENT).toFixed(2)}
					</span>
				</label>
				<input
					id="light-ambient-range"
					type="range"
					className="form-range mb-2"
					min={0}
					max={0.45}
					step={0.01}
					value={worldConfig.lightAmbient ?? DEFAULT_LIGHT_AMBIENT}
					disabled={!engineReady}
					onChange={(e) => setDirectionalLight({ ambient: parseFloat(e.target.value) })}
				/>

				<label
					className="form-label small text-secondary mb-1 d-flex justify-content-between"
					htmlFor="light-intensity-range"
				>
					<span>{t('Light intensity')}</span>
					<span className="text-info fw-bold">
						{(worldConfig.lightIntensity ?? DEFAULT_LIGHT_INTENSITY).toFixed(2)}
					</span>
				</label>
				<input
					id="light-intensity-range"
					type="range"
					className="form-range mb-2"
					min={0.2}
					max={2.5}
					step={0.05}
					value={worldConfig.lightIntensity ?? DEFAULT_LIGHT_INTENSITY}
					disabled={!engineReady}
					onChange={(e) => setDirectionalLight({ intensity: parseFloat(e.target.value) })}
				/>
			</Accordion.Body>
		</Accordion.Item>
	)
}
