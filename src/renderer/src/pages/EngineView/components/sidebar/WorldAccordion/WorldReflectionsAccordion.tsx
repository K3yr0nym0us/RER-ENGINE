import { Accordion } from 'react-bootstrap'
import { Stars } from 'react-bootstrap-icons'

import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'

const TIERS = [
	{ key: 'off' as const, labelKey: 'Off' },
	{ key: 'low' as const, labelKey: 'Low' },
	{ key: 'medium' as const, labelKey: 'Medium' },
	{ key: 'high' as const, labelKey: 'High' },
	{ key: 'ultra' as const, labelKey: 'Ultra' },
]

const DEBUG_VIEWS = [
	{ key: 'final', labelKey: 'Reflection debug final' },
	{ key: 'normals', labelKey: 'Reflection debug normals' },
	{ key: 'depth', labelKey: 'Reflection debug depth' },
	{ key: 'roughness', labelKey: 'Reflection debug roughness' },
	{ key: 'metallic', labelKey: 'Reflection debug metallic' },
	{ key: 'ssr_hits', labelKey: 'Reflection debug ssr hits' },
	{ key: 'reflection_mask', labelKey: 'Reflection debug mask' },
	{ key: 'cubemap', labelKey: 'Reflection debug cubemap' },
	{ key: 'reflection_color', labelKey: 'Reflection debug reflection color' },
	{ key: 'recon_world', labelKey: 'Reflection debug recon world' },
	{ key: 'recon_ndc', labelKey: 'Reflection debug recon ndc' },
	{ key: 'recon_view', labelKey: 'Reflection debug recon view' },
	{ key: 'reproject_uv', labelKey: 'Reflection debug reproject uv' },
	{ key: 'ssr_view_vector', labelKey: 'Reflection debug ssr view vector' },
	{ key: 'ssr_reflection_vector', labelKey: 'Reflection debug ssr reflection vector' },
	{ key: 'ssr_raymarch_path', labelKey: 'Reflection debug ssr raymarch path' },
	{ key: 'ssr_hit_depth_delta', labelKey: 'Reflection debug ssr hit depth delta' },
	{ key: 'ssr_hit_uv', labelKey: 'Reflection debug ssr hit uv' },
	{ key: 'ssr_hit_color_raw', labelKey: 'Reflection debug ssr hit color raw' },
	{ key: 'ssr_hit_color_blurred', labelKey: 'Reflection debug ssr hit color blurred' },
	{ key: 'ssr_no_blur', labelKey: 'Reflection debug ssr no blur' },
	{ key: 'ssr_final_composite', labelKey: 'Reflection debug ssr final composite' },
	{ key: 'ssr_hit_uv_world_screen', labelKey: 'Reflection debug ssr hit uv world screen' },
	{ key: 'ssr_hit_uv_world_screen_delta', labelKey: 'Reflection debug ssr hit uv world screen delta' },
	{ key: 'ssr_hit_uv_world_screen_split', labelKey: 'Reflection debug ssr hit uv world screen split' },
] as const

export default function WorldReflectionsAccordion() {
	const { t } = useTraslate()
	const { engineReady, worldConfig, setReflectionTier, setReflectionDebugView } =
		useContextEngine()

	const activeTier = worldConfig.reflectionTier ?? 'off'
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

				<label className="form-label small text-secondary mb-1">{t('Reflection tier')}</label>
				<div className="d-flex flex-wrap gap-1 mb-3">
					{TIERS.map(({ key, labelKey }) => (
						<button
							key={key}
							type="button"
							className={`btn btn-sm ${
								activeTier === key ? 'btn-info' : 'btn-outline-secondary'
							}`}
							disabled={!engineReady}
							onClick={() => setReflectionTier(key)}
						>
							{t(labelKey)}
						</button>
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
					className="form-select form-select-sm mb-0"
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
