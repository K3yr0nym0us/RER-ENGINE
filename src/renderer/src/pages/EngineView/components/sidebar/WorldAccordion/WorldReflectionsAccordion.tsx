import { Accordion, Form } from 'react-bootstrap'
import { Stars } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'
import { normalizeReflectionDebugView } from '../../../../../context/useContextEngine/types'

const DEBUG_VIEWS = [
	{ key: 'final', labelKey: 'Reflection debug final' },
	{ key: 'ssr_debug', labelKey: 'Reflection debug ssr debug' },
	{ key: 'ssr_miss_green', labelKey: 'Reflection debug ssr miss green' },
	{ key: 'ssr_exit_reason', labelKey: 'Reflection debug ssr exit reason' },
	{ key: 'ssr_vector_rgb', labelKey: 'Reflection debug ssr vector rgb' },
	{ key: 'ssr_hit_class', labelKey: 'Reflection debug ssr hit class' },
	{ key: 'ssr_path_px', labelKey: 'Reflection debug ssr path px' },
	{ key: 'ssr_march_refl_dir', labelKey: 'Reflection debug ssr march refl dir' },
	{ key: 'ssr_hit_uv', labelKey: 'Reflection debug ssr hit uv' },
	{ key: 'ssr_hit_sample_color', labelKey: 'Reflection debug ssr hit sample color' },
	{ key: 'ssr_proj_depth_delta', labelKey: 'Reflection debug ssr proj depth delta' },
] as const

export default function WorldReflectionsAccordion() {
	const { t } = useTraslate()
	const {
		engineReady,
		worldConfig,
		setReflectionTier,
		setReflectionProbes,
		setReflectionRaytracing,
		spawnReflectionProbe,
		setReflectionDebugView,
	} = useContextEngine()

	const activeTier = worldConfig.reflectionTier ?? 'off'
	const probesEnabled = worldConfig.reflectionProbes ?? false
	const raytracingEnabled = worldConfig.reflectionRaytracing ?? false
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
				{tierDegraded && (
					<p className="text-warning mb-2" style={{ fontSize: '0.72rem' }}>
						{t('Reflection tier degraded hint')
							.replace('{{requested}}', activeTier)
							.replace('{{effective}}', effectiveTier ?? 'medium')}
					</p>
				)}

				<div className="d-flex justify-content-between align-items-center gap-2 mb-1">
					<label className="form-label small text-secondary mb-0 fw-bold">{t('Reflection tier')}</label>
					<div className="d-inline-flex align-items-center gap-1 flex-shrink-0">
						<span
							className={`small mb-0 ${raytracingEnabled ? 'text-secondary' : 'text-info'}`}
							style={{ fontSize: '0.68rem' }}
						>
							{t('RT OFF')}
						</span>
						<Form.Check
							type="switch"
							id="reflection-raytracing-enabled"
							className="mb-0"
							checked={raytracingEnabled}
							disabled={!engineReady || !reflectionsActive}
							onChange={(e) => setReflectionRaytracing(e.target.checked)}
							aria-label={t('Reflection ray tracing')}
						/>
						<span
							className={`small mb-0 ${raytracingEnabled ? 'text-info' : 'text-secondary'}`}
							style={{ fontSize: '0.68rem' }}
						>
							{t('RT ON')}
						</span>
					</div>
				</div>
				<div className="d-flex flex-wrap gap-1 mb-3">
					<AppTooltip
						content={t('Disables SSR and reflection composite.')}
						place="right"
						tooltipClassName="app-tooltip--compact app-tooltip--tier-hint"
					>
						<button
							type="button"
							className={`btn btn-sm ${activeTier === 'off' ? 'btn-info' : 'btn-outline-secondary'}`}
							disabled={!engineReady || activeTier === 'off'}
							onClick={() => setReflectionTier('off')}
						>
							{t('Off')}
						</button>
					</AppTooltip>
					<AppTooltip
						content={t('SSR at half resolution, minimal temporal.')}
						place="right"
						tooltipClassName="app-tooltip--compact app-tooltip--tier-hint"
					>
						<button
							type="button"
							className={`btn btn-sm ${activeTier === 'low' ? 'btn-info' : 'btn-outline-secondary'}`}
							disabled={!engineReady || activeTier === 'low'}
							onClick={() => setReflectionTier('low')}
						>
							{t('Low')}
						</button>
					</AppTooltip>
					<AppTooltip
						content={t('SSR at half resolution with temporal accumulation.')}
						place="top"
						tooltipClassName="app-tooltip--compact app-tooltip--tier-hint"
					>
						<button
							type="button"
							className={`btn btn-sm ${activeTier === 'medium' ? 'btn-info' : 'btn-outline-secondary'}`}
							disabled={!engineReady || activeTier === 'medium'}
							onClick={() => setReflectionTier('medium')}
						>
							{t('Medium')}
						</button>
					</AppTooltip>
					<AppTooltip
						content={t('SSR at 75% resolution, stronger temporal, longer trace.')}
						place="top-end"
						tooltipClassName="app-tooltip--compact app-tooltip--tier-hint"
					>
						<button
							type="button"
							className={`btn btn-sm ${activeTier === 'high' ? 'btn-info' : 'btn-outline-secondary'}`}
							disabled={!engineReady || activeTier === 'high'}
							onClick={() => setReflectionTier('high')}
						>
							{t('High')}
						</button>
					</AppTooltip>
					<AppTooltip
						content={t('SSR at full resolution, max temporal and roughness trace.')}
						place="top-end"
						tooltipClassName="app-tooltip--compact app-tooltip--tier-hint"
					>
						<button
							type="button"
							className={`btn btn-sm ${activeTier === 'ultra' ? 'btn-info' : 'btn-outline-secondary'}`}
							disabled={!engineReady || activeTier === 'ultra'}
							onClick={() => setReflectionTier('ultra')}
						>
							{t('Ultra')}
						</button>
					</AppTooltip>
				</div>

				<div className="d-flex flex-wrap align-items-center gap-2 mb-3">
					<AppTooltip
						content={
							reflectionsActive
								? t('Enables reflection probes in the render. Placing a probe in the scene is not enough; turn this switch on to use them.')
								: t('Select a reflection tier other than Off to enable probes in the render.')
						}
						place="right"
					>
						<span className="d-inline-flex">
							<Form.Check
								type="switch"
								id="reflection-probes-enabled"
								className="mb-0"
								checked={probesEnabled}
								disabled={!engineReady || !reflectionsActive}
								onChange={(e) => setReflectionProbes(e.target.checked)}
								aria-label={t('Reflection probes')}
							/>
						</span>
					</AppTooltip>
					<button
						type="button"
						className="btn btn-sm btn-outline-info"
						disabled={!engineReady}
						onClick={() => spawnReflectionProbe()}
					>
						{t('Insert reflection probe')}
					</button>
				</div>

				<label className="form-label small text-secondary mb-2" htmlFor="reflection-debug-view">
					{t('Reflection debug view')}
				</label>
				<select
					id="reflection-debug-view"
					className="form-select form-select-sm bg-dark text-light border-secondary mb-0"
					value={activeDebugView}
					disabled={!engineReady}
					onChange={(e) => setReflectionDebugView(normalizeReflectionDebugView(e.target.value))}
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
