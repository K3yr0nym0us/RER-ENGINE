import { Accordion } from 'react-bootstrap'
import { Image } from 'react-bootstrap-icons'

import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'

const TIERS = [
	{ key: 'low' as const, labelKey: 'Low' },
	{ key: 'medium' as const, labelKey: 'Medium' },
	{ key: 'high' as const, labelKey: 'High' },
	{ key: 'ultra' as const, labelKey: 'Ultra' },
]

export default function WorldTexturesAccordion() {
	const { t } = useTraslate()
	const { engineReady, worldConfig, setGraphicsTextureTier, setTextureDetailDistance } =
		useContextEngine()

	const activeTier = worldConfig.graphicsTextureTier ?? 'medium'
	const detailDistance = worldConfig.textureDetailDistance ?? 10

	return (
		<Accordion.Item eventKey="world-textures">
			<Accordion.Header>
				<Image className="me-2" />
				{t('GLB textures')}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-2">
				<p className="text-secondary mb-2" style={{ fontSize: '0.72rem' }}>
					{t('World textures tier hint')}
				</p>

				<label className="form-label small text-secondary mb-1">{t('Graphics texture tier')}</label>
				<div className="d-flex flex-wrap gap-1 mb-3">
					{TIERS.map(({ key, labelKey }) => (
						<button
							key={key}
							type="button"
							className={`btn btn-sm ${
								activeTier === key ? 'btn-info' : 'btn-outline-secondary'
							}`}
							disabled={!engineReady}
							onClick={() => setGraphicsTextureTier(key)}
						>
							{t(labelKey)}
						</button>
					))}
				</div>

				<label
					className="form-label small text-secondary mb-1 d-flex justify-content-between"
					htmlFor="texture-detail-distance-range"
				>
					<span>{t('Texture detail distance')}</span>
					<span className="text-info fw-bold">{detailDistance.toFixed(0)} m</span>
				</label>
				<input
					id="texture-detail-distance-range"
					type="range"
					className="form-range mb-1"
					min={5}
					max={80}
					step={1}
					value={detailDistance}
					disabled={!engineReady}
					onChange={(e) => setTextureDetailDistance(parseFloat(e.target.value))}
				/>
				<p className="text-secondary mb-0" style={{ fontSize: '0.68rem' }}>
					{t('Texture detail distance hint')}
				</p>
			</Accordion.Body>
		</Accordion.Item>
	)
}
