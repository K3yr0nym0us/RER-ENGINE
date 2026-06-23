import { Accordion } from 'react-bootstrap'
import { Stars } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'

import { ReflectionTierTooltipNote } from './ReflectionTierTooltipNote'

const TIERS = [
	{
		key: 'off' as const,
		labelKey: 'Off',
		descKey: 'Reflections off: no SSR, probes, or RT.',
		tooltipPlace: 'top' as const,
	},
	{
		key: 'low' as const,
		labelKey: 'Low',
		descKey: 'SSR on screen + environment probes (128px). No temporal smoothing.',
		tooltipPlace: 'top' as const,
	},
	{
		key: 'medium' as const,
		labelKey: 'Medium',
		descKey: 'SSR + temporal accumulation + probes (256px). No ray tracing.',
		tooltipPlace: 'top' as const,
	},
	{
		key: 'high' as const,
		labelKey: 'High',
		descKey: 'Medium + hardware RT on static meshes (1 bounce).',
		tooltipPlace: 'top-end' as const,
	},
	{
		key: 'ultra' as const,
		labelKey: 'Ultra',
		descKey: 'High + skinned meshes in RT, 2nd bounce, dielectrics, SSIL.',
		tooltipPlace: 'top-end' as const,
	},
] as const

const DEBUG_VIEWS = [
	{ key: 'final', labelKey: 'Reflection debug final' },
	{ key: 'normals', labelKey: 'Reflection debug normals' },
	{ key: 'roughness', labelKey: 'Reflection debug roughness' },
	{ key: 'ssr_hits', labelKey: 'Reflection debug ssr hits' },
	{ key: 'reflection_mask', labelKey: 'Reflection debug mask' },
	{ key: 'rt_instances', labelKey: 'RT instances (diagnostic)' },
] as const

export default function WorldReflectionsAccordion() {
	const { t } = useTraslate()
	const { engineReady, worldConfig, setReflectionTier, setReflectionDebugView } =
		useContextEngine()

	const activeTier = worldConfig.reflectionTier ?? 'off'
	const effectiveTier = worldConfig.reflectionTierEffective
	const tierDegraded =
		effectiveTier != null
		&& effectiveTier !== activeTier
		&& (activeTier === 'high' || activeTier === 'ultra')
	const activeDebugView = worldConfig.reflectionDebugView ?? 'final'

	return (
		<Accordion.Item eventKey="world-reflections">
			<Accordion.Header>
				<Stars className="me-2" />
				{t('Reflections')}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-2">
				<p className="text-secondary mb-2" style={{ fontSize: '0.72rem' }}>
					{t('World reflections tier hint')}
				</p>

				{tierDegraded && (
					<p className="text-warning mb-2" style={{ fontSize: '0.72rem' }}>
						{t('Reflection tier degraded hint')
							.replace('{{requested}}', activeTier)
							.replace('{{effective}}', effectiveTier ?? 'medium')}
					</p>
				)}

				<label className="form-label small text-secondary mb-1">{t('Reflection tier')}</label>
				<div className="d-flex flex-wrap gap-1 mb-3">
					{TIERS.map(({ key, labelKey, descKey, tooltipPlace }) => (
						<AppTooltip
							key={key}
							content={<ReflectionTierTooltipNote descKey={descKey} />}
							place={tooltipPlace}
							tooltipClassName="app-tooltip--compact app-tooltip--tier-hint"
						>
							<span className="d-inline-block">
								<button
									type="button"
									className={`btn btn-sm ${
										activeTier === key ? 'btn-info' : 'btn-outline-secondary'
									}`}
									disabled={!engineReady}
									onClick={() => setReflectionTier(key)}
								>
									{t(labelKey)}
								</button>
							</span>
						</AppTooltip>
					))}
				</div>

				<label className="form-label small text-secondary mb-1" htmlFor="reflection-debug-view">
					{t('Reflection debug view')}
				</label>
				<p className="text-secondary mb-2" style={{ fontSize: '0.72rem' }}>
					{t('Reflection debug view hint')}
				</p>
				<select
					id="reflection-debug-view"
					className="form-select form-select-sm bg-dark text-light border-secondary mb-0"
					value={activeDebugView}
					disabled={!engineReady || activeTier === 'off'}
					onChange={(e) => setReflectionDebugView(e.target.value)}
				>
					{DEBUG_VIEWS.map(({ key, labelKey }) => (
						<option key={key} value={key}>
							{t(labelKey)}
						</option>
					))}
				</select>
			</Accordion.Body>
		</Accordion.Item>
	)
}
