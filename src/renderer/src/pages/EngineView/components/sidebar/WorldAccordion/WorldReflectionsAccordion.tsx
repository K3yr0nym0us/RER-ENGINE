import { Accordion, Form } from 'react-bootstrap'
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
	{ key: 'ssr_debug', labelKey: 'Reflection debug ssr debug' },
	{ key: 'ssr_miss_green', labelKey: 'Reflection debug ssr miss green' },
	{ key: 'ssr_exit_reason', labelKey: 'Reflection debug ssr exit reason' },
	{ key: 'ssr_vector_rgb', labelKey: 'Reflection debug ssr vector rgb' },
] as const

export default function WorldReflectionsAccordion() {
	const { t } = useTraslate()
	const {
		engineReady,
		worldConfig,
		setReflectionTier,
		setReflectionProbes,
		spawnReflectionProbe,
		setReflectionDebugView,
	} = useContextEngine()

	const activeTier = worldConfig.reflectionTier ?? 'off'
	const probesEnabled = worldConfig.reflectionProbes ?? false
	const effectiveTier = worldConfig.reflectionTierEffective
	const tierDegraded =
		effectiveTier != null
		&& effectiveTier !== activeTier
		&& (activeTier === 'high' || activeTier === 'ultra')
	const activeDebugView = worldConfig.reflectionDebugView ?? 'final'
	const reflectionsActive = activeTier !== 'off'

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

				<div className="d-flex align-items-start gap-2 mb-1">
					<Form.Check
						type="switch"
						id="reflection-probes-enabled"
						className="mt-1"
						checked={probesEnabled}
						disabled={!engineReady || !reflectionsActive}
						onChange={(e) => setReflectionProbes(e.target.checked)}
					/>
					<div>
						<label className="form-label small text-secondary mb-0" htmlFor="reflection-probes-enabled">
							{t('Reflection probes')}
						</label>
						<p className="text-secondary mb-0" style={{ fontSize: '0.72rem' }}>
							{reflectionsActive
								? t('Reflection probes hint')
								: t('Reflection probes tier off hint')}
						</p>
					</div>
				</div>

				<button
					type="button"
					className="btn btn-sm btn-outline-info mb-2 mt-2"
					disabled={!engineReady}
					onClick={() => spawnReflectionProbe()}
				>
					{t('Insert reflection probe')}
				</button>
				<p className="text-secondary mb-3" style={{ fontSize: '0.72rem' }}>
					{t('Insert reflection probe hint')}
				</p>

				<label className="form-label small text-secondary mb-1" htmlFor="reflection-debug-view">
					{t('Reflection debug view')}
				</label>
				<p className="text-secondary mb-2" style={{ fontSize: '0.72rem' }}>
					{t('Reflection debug view hint ssr')}
				</p>
				<select
					id="reflection-debug-view"
					className="form-select form-select-sm bg-dark text-light border-secondary mb-0"
					value={activeDebugView}
					disabled={!engineReady}
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
