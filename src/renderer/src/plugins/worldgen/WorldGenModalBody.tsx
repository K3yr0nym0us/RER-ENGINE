import { useCallback, useEffect, useRef, useState } from 'react'
import { Accordion } from 'react-bootstrap'
import { ArrowClockwise, PlayFill, PlusLg, StopFill, Trash3 } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { useTraslate } from '@hooks'
import type { EngineCommand3D, EngineEvent, ModelInfo } from '@shared-types'

import {
  BIOME_PRIORITY,
  WORLDGEN_RADIUS_DEFAULT,
  WORLDGEN_SIZE_DEFAULT,
  WORLDGEN_SIZE_MAX,
  WORLDGEN_SIZE_MIN,
  clampBiomes,
  featureCaps,
  featureSizes,
  heightForRadius,
  maxBiomeSlots,
  MIN_BIOME_SHARE,
  normalizeBiomeShares,
  rebalanceBiomeShares,
  sizeToRadius,
  toggleBiomeShare,
  type BiomeShares,
} from './worldgenLimits'

/**
 * Modal del plugin world-gen (ventana Electron hija).
 *
 * Solo habla con el engine vía `window.engine3d.send3d` (los comandos
 * `world_gen_*` se gating en el proceso main) y con el estado de plugins vía
 * `pluginsGetState` para obtener la ruta de la DLL instalada.
 */

interface PanelState {
  seed: number
  size: number
  voxelSize: number
  mountains: number
  hills: number
  valleys: number
  plains: number
  cliffs: number
  lakes: number
  riversMaxCount: number
  cavesMaxCount: number
  waterfallsMaxCount: number
  biomes: { forest: boolean; desert: boolean; grassland: boolean; snow: boolean; volcanic: boolean }
  biomeShares: BiomeShares
  replaceWorld: boolean
}

interface ScatterRulesState {
  maxInstances: number
  density: number
  minDistance: number
  altitudeMin: number
  altitudeMax: number
  maxSlopeDeg: number
  scaleMin: number
  scaleMax: number
  randomRotation: boolean
  clusterEnabled: boolean
  clusterSize: number
}

function randomSeed(): number {
  const buffer = new Uint32Array(1)
  crypto.getRandomValues(buffer)
  return buffer[0]
}

const DEFAULT_BIOMES = clampBiomes(
  { forest: true, desert: true, grassland: true, snow: true, volcanic: true },
  WORLDGEN_RADIUS_DEFAULT,
)

const DEFAULT_PANEL: PanelState = {
  seed: 1,
  size: WORLDGEN_SIZE_DEFAULT,
  voxelSize: 1,
  mountains: 4,
  hills: 8,
  valleys: 3,
  plains: 2,
  cliffs: 0,
  lakes: 2,
  riversMaxCount: 2,
  cavesMaxCount: 6,
  waterfallsMaxCount: 2,
  biomes: DEFAULT_BIOMES,
  biomeShares: normalizeBiomeShares(
    { grassland: 0, forest: 0, snow: 0, desert: 0, volcanic: 0 },
    DEFAULT_BIOMES,
  ),
  replaceWorld: false,
}

const DEFAULT_SCATTER: ScatterRulesState = {
  maxInstances: 200,
  density: 50,
  minDistance: 4,
  altitudeMin: 0,
  altitudeMax: 200,
  maxSlopeDeg: 35,
  scaleMin: 80,
  scaleMax: 130,
  randomRotation: true,
  clusterEnabled: false,
  clusterSize: 12,
}

interface ScatterEntry {
  id: string
  modelPath: string
  rules: ScatterRulesState
}

/** Id estable de la blueprint sintetizada para una entrada de scatter. */
function scatterBlueprintId(entryId: string): string {
  return `worldgen:${entryId}`
}

function StackedNumber({
  id,
  label,
  title,
  value,
  min,
  max,
  step,
  onChange,
}: {
  id: string
  label: string
  title?: string
  value: number
  min?: number
  max?: number
  step?: number
  onChange: (value: number) => void
}) {
  return (
    <div className="min-w-0" title={title}>
      <label className="form-label m-0" htmlFor={id}>{label}</label>
      <input
        type="number"
        id={id}
        step={step}
        min={min}
        max={max}
        className="form-control form-control-sm"
        aria-label={label}
        value={value}
        onChange={(e) => onChange(Number(e.target.value) || 0)}
      />
    </div>
  )
}

function ScatterFields({
  entryId,
  rules,
  onChange,
}: {
  entryId: string
  rules: ScatterRulesState
  onChange: (next: ScatterRulesState) => void
}) {
  const { t } = useTraslate()
  const fieldId = (name: string) => `worldgen-${entryId}-${name}`
  return (
    <>
      <div className="d-grid gap-2" style={{ gridTemplateColumns: 'repeat(4, minmax(0, 1fr))' }}>
        <div className="min-w-0">
          <label className="form-label m-0" htmlFor={fieldId('max-instances')}>{t('Max instances')}</label>
          <input
            type="number"
            id={fieldId('max-instances')}
            className="form-control form-control-sm"
            min={0}
            value={rules.maxInstances}
            onChange={(e) =>
              onChange({ ...rules, maxInstances: Math.max(0, Number(e.target.value) || 0) })
            }
          />
        </div>
        <div className="min-w-0">
          <label className="form-label m-0" htmlFor={fieldId('min-distance')}>{t('Min distance (m)')}</label>
          <input
            type="number"
            id={fieldId('min-distance')}
            step={0.5}
            className="form-control form-control-sm"
            value={rules.minDistance}
            onChange={(e) => onChange({ ...rules, minDistance: Number(e.target.value) || 0 })}
          />
        </div>
        <div className="min-w-0">
          <label className="form-label m-0" htmlFor={fieldId('density')}>{t('Density %')}</label>
          <input
            type="number"
            id={fieldId('density')}
            className="form-control form-control-sm"
            min={0}
            max={100}
            value={rules.density}
            onChange={(e) =>
              onChange({ ...rules, density: Math.max(0, Number(e.target.value) || 0) })
            }
          />
        </div>
        <div className="min-w-0">
          <label className="form-label m-0" htmlFor={fieldId('max-slope')}>{t('Max slope (°)')}</label>
          <input
            type="number"
            id={fieldId('max-slope')}
            className="form-control form-control-sm"
            min={0}
            max={90}
            value={rules.maxSlopeDeg}
            onChange={(e) => onChange({ ...rules, maxSlopeDeg: Number(e.target.value) || 0 })}
          />
        </div>
      </div>
      <div className="d-grid gap-2 mt-2" style={{ gridTemplateColumns: 'repeat(4, minmax(0, 1fr))' }}>
        <div className="min-w-0">
          <label className="form-label m-0" htmlFor={fieldId('altitude-min')}>{t('Min altitude (m)')}</label>
          <input
            type="number"
            id={fieldId('altitude-min')}
            className="form-control form-control-sm"
            value={rules.altitudeMin}
            onChange={(e) => onChange({ ...rules, altitudeMin: Number(e.target.value) || 0 })}
          />
        </div>
        <div className="min-w-0">
          <label className="form-label m-0" htmlFor={fieldId('altitude-max')}>{t('Max altitude (m)')}</label>
          <input
            type="number"
            id={fieldId('altitude-max')}
            className="form-control form-control-sm"
            value={rules.altitudeMax}
            onChange={(e) => onChange({ ...rules, altitudeMax: Number(e.target.value) || 0 })}
          />
        </div>
        <div className="min-w-0">
          <label className="form-label m-0" htmlFor={fieldId('scale-min')}>{t('Min scale %')}</label>
          <input
            type="number"
            id={fieldId('scale-min')}
            className="form-control form-control-sm"
            value={rules.scaleMin}
            onChange={(e) => onChange({ ...rules, scaleMin: Number(e.target.value) || 100 })}
          />
        </div>
        <div className="min-w-0">
          <label className="form-label m-0" htmlFor={fieldId('scale-max')}>{t('Max scale %')}</label>
          <input
            type="number"
            id={fieldId('scale-max')}
            className="form-control form-control-sm"
            value={rules.scaleMax}
            onChange={(e) => onChange({ ...rules, scaleMax: Number(e.target.value) || 100 })}
          />
        </div>
      </div>
      <div className="d-flex flex-wrap gap-3 mt-2">
        <div className="form-check form-switch m-0">
          <input
            className="form-check-input"
            type="checkbox"
            checked={rules.randomRotation}
            onChange={(e) => onChange({ ...rules, randomRotation: e.target.checked })}
            id={fieldId('random-rotation')}
          />
          <label className="form-check-label small mb-0" htmlFor={fieldId('random-rotation')}>
            {t('Random rotation')}
          </label>
        </div>
        <div className="form-check form-switch m-0">
          <input
            className="form-check-input"
            type="checkbox"
            checked={rules.clusterEnabled}
            onChange={(e) => onChange({ ...rules, clusterEnabled: e.target.checked })}
            id={fieldId('cluster')}
          />
          <label className="form-check-label small mb-0" htmlFor={fieldId('cluster')}>
            {t('Clusters')}
          </label>
        </div>
        {rules.clusterEnabled && (
          <div className="d-flex align-items-center gap-2">
            <label className="m-0" htmlFor={fieldId('cluster-size')}>{t('Cluster size')}</label>
            <input
              type="number"
              id={fieldId('cluster-size')}
              className="form-control form-control-sm"
              style={{ width: 76 }}
              min={2}
              value={rules.clusterSize}
              onChange={(e) =>
                onChange({ ...rules, clusterSize: Math.max(2, Number(e.target.value) || 2) })
              }
            />
          </div>
        )}
      </div>
    </>
  )
}

/** Cantidad 0 apaga el rasgo; cualquier valor mayor lo incluye con ese máximo. */
function countedFeature(count: number): { enabled: boolean; maxCount: number } {
  const maxCount = Math.max(0, Math.round(count))
  return { enabled: maxCount > 0, maxCount }
}

function clampPanelToSize(panel: PanelState, size: number): PanelState {
  const next = Math.min(WORLDGEN_SIZE_MAX, Math.max(WORLDGEN_SIZE_MIN, size))
  const nextCaps = featureCaps(sizeToRadius(next))
  const biomes = clampBiomes(panel.biomes, sizeToRadius(next))
  return {
    ...panel,
    size: next,
    mountains: Math.min(panel.mountains, nextCaps.mountains),
    hills: Math.min(panel.hills, nextCaps.hills),
    valleys: Math.min(panel.valleys, nextCaps.valleys),
    plains: Math.min(panel.plains, nextCaps.plains),
    cliffs: Math.min(panel.cliffs, nextCaps.cliffs),
    lakes: Math.min(panel.lakes, nextCaps.lakes),
    riversMaxCount: Math.min(panel.riversMaxCount, nextCaps.riversMaxCount),
    cavesMaxCount: Math.min(panel.cavesMaxCount, nextCaps.cavesMaxCount),
    waterfallsMaxCount: Math.min(panel.waterfallsMaxCount, nextCaps.waterfallsMaxCount),
    biomes,
    biomeShares: normalizeBiomeShares(panel.biomeShares, biomes),
  }
}

export function WorldGenModalBody() {
  const { t } = useTraslate()
  const [panel, setPanel] = useState<PanelState>(() => ({ ...DEFAULT_PANEL, seed: randomSeed() }))
  const [scatterEntries, setScatterEntries] = useState<ScatterEntry[]>([])
  const [openScatterId, setOpenScatterId] = useState<string | null>(null)
  const [selectedModelPath, setSelectedModelPath] = useState('')
  const [models, setModels] = useState<ModelInfo[]>([])
  const [status, setStatus] = useState('')
  const [generation, setGeneration] = useState<{
    completed: number
    total: number
    phase: 'scan' | 'mesh'
  } | null>(null)
  const [distributing, setDistributing] = useState(false)
  const [plugin, setPlugin] = useState<{ libraryPath: string | null } | null>(null)
  const finishHandled = useRef(false)
  const pendingScatter = useRef(0)
  const distributeRef = useRef<() => void>(() => {})

  // Ref estable para window.engine3d.off.
  const onEngineEvent = useCallback((event: EngineEvent) => {
    if (event.event === 'models_list') {
      setModels((event as { models?: ModelInfo[] }).models ?? [])
      return
    }
    if (event.event === 'world_gen_progress') {
      const phase = event.phase === 'scan' ? 'scan' : 'mesh'
      const completed = Number(event.completed ?? 0)
      const total = Number(event.total ?? 0)
      setGeneration({ completed, total, phase })
      // Red de seguridad: si se pierde `world_gen_completed`, la cola de malla
      // ya terminó cuando completed alcanza el total. Se limpia el spinner
      // siempre, aunque el flag de cierre ya estuviera marcado.
      if (phase === 'mesh' && total > 0 && completed >= total) {
        setGeneration(null)
        if (!finishHandled.current) {
          finishHandled.current = true
          distributeRef.current()
        }
      }
      return
    }
    if (event.event === 'world_gen_error') {
      finishHandled.current = true
      pendingScatter.current = 0
      setGeneration(null)
      setDistributing(false)
      setStatus(String(event.message ?? t('World generation failed')))
      return
    }
    if (event.event === 'world_gen_completed' || (event.event === 'world_gen_status' && String(event.state ?? '') === 'complete')) {
      // Limpiar siempre el spinner: aunque el cierre ya se hubiera marcado,
      // la modal debe reflejar que la generación terminó.
      setGeneration(null)
      if (finishHandled.current) return
      finishHandled.current = true
      distributeRef.current()
      return
    }
    if (event.event === 'world_gen_scatter_done') {
      const count = Number(event.count ?? 0)
      pendingScatter.current = Math.max(0, pendingScatter.current - 1)
      if (pendingScatter.current === 0) {
        setDistributing(false)
        setStatus(t('World ready'))
      } else {
        setStatus(`${t('Distributing blueprints…')} (${count})`)
      }
      return
    }
    if (event.event === 'world_gen_status') {
      const state = String(event.state ?? '')
      if (state === 'library_error') {
        finishHandled.current = true
        pendingScatter.current = 0
        setGeneration(null)
        setDistributing(false)
        setStatus(t('World generation failed'))
      }
    }
  }, [t])

  useEffect(() => {
    // Lista de modelos del proyecto para el scatter (el engine responde con
    // el evento `models_list`, reenviado a esta ventana por el proceso main).
    window.engine3d.send({ cmd: 'get_models_list' })
    window.engine3d.on(onEngineEvent)
    return () => {
      window.engine3d.off(onEngineEvent)
    }
  }, [onEngineEvent])

  useEffect(() => {
    const check = async () => {
      const state = await window.electronAPI.pluginsGetState()
      const record = state.installed['world-gen']
      setPlugin({
        libraryPath: record?.libraryPath ?? null,
      })
    }
    void check()
  }, [])

  const send = (cmd: EngineCommand3D) => {
    window.engine3d.send(cmd)
  }

  /** Guard del gating del proceso main: el plugin tiene que estar instalado. */
  const resolvePlugin = async (): Promise<boolean> => {
    const state = await window.electronAPI.pluginsGetState()
    const record = state.installed['world-gen']
    const libraryPath = record?.libraryPath ?? null
    setPlugin({ libraryPath })
    if (!libraryPath) {
      setStatus(t('Install the World Generator plugin first'))
      return false
    }
    send({ cmd: 'world_gen_set_library_path', path: libraryPath })
    return true
  }

  /** JSON camelCase de `WorldGenConfig` del crate worldgen. */
  const buildConfigJson = (): string => {
    const p = panel
    const radius = sizeToRadius(p.size)
    const sizes = featureSizes(radius)
    return JSON.stringify({
      seed: p.seed,
      bounds: { width: p.size, length: p.size, height: heightForRadius(radius) },
      resolution: { voxelSize: p.voxelSize, lodEnabled: true },
      mountains: {
        ...countedFeature(p.mountains),
        ...sizes.mountains,
      },
      hills: {
        ...countedFeature(p.hills),
        ...sizes.hills,
      },
      valleys: {
        ...countedFeature(p.valleys),
        ...sizes.valleys,
      },
      plains: {
        ...countedFeature(p.plains),
        ...sizes.plains,
      },
      cliffs: {
        ...countedFeature(p.cliffs),
        ...sizes.cliffs,
      },
      lakes: {
        ...countedFeature(p.lakes),
        ...sizes.lakes,
      },
      rivers: {
        ...countedFeature(p.riversMaxCount),
        ...sizes.rivers,
      },
      caves: {
        ...countedFeature(p.cavesMaxCount),
        depthMin: 10,
        depthMax: 50,
        radius: 3,
        roomChance: 0.2,
        segments: 18,
      },
      waterfalls: {
        ...countedFeature(p.waterfallsMaxCount),
        minDrop: 6,
      },
      biomes: { ...p.biomes, shares: p.biomeShares },
    })
  }

  const onGenerate = async () => {
    if (!(await resolvePlugin())) return
    // Limpiar el estado ANTES de enviar: si el motor responde con un error en
    // el mismo tick, no debe quedar un spinner sin futuros eventos.
    finishHandled.current = false
    pendingScatter.current = 0
    setDistributing(false)
    setStatus('')
    setGeneration({ completed: 0, total: 0, phase: 'scan' })
    send({ cmd: 'world_gen_set_config', config_json: buildConfigJson() })
    send({ cmd: 'world_gen_start', replace_world: panel.replaceWorld })
  }

  const onStop = () => {
    send({ cmd: 'world_gen_stop' })
    finishHandled.current = true
    pendingScatter.current = 0
    setDistributing(false)
    setGeneration(null)
    setStatus(t('Generation cancelled'))
  }

  const onReset = () => {
    send({ cmd: 'world_gen_reset' })
    setStatus(t('World cleared'))
  }

  const addScatterModel = (modelPath: string) => {
    const id = crypto.randomUUID()
    setScatterEntries((prev) => [...prev, { id, modelPath, rules: { ...DEFAULT_SCATTER } }])
    setOpenScatterId(id)
  }

  const updateScatterRules = (id: string, rules: ScatterRulesState) => {
    setScatterEntries((prev) => prev.map((entry) => (entry.id === id ? { ...entry, rules } : entry)))
  }

  const removeScatterModel = (id: string) => {
    setScatterEntries((prev) => prev.filter((entry) => entry.id !== id))
    setOpenScatterId((current) => (current === id ? null : current))
  }

  distributeRef.current = () => {
    setGeneration(null)
    if (scatterEntries.length === 0) {
      setDistributing(false)
      setStatus(t('World ready'))
      return
    }
    pendingScatter.current = scatterEntries.length
    setDistributing(true)
    setStatus(t('Distributing blueprints…'))
    for (const entry of scatterEntries) {
      const model = models.find((item) => item.path === entry.modelPath)
      const blueprintId = scatterBlueprintId(entry.id)
      const s = entry.rules
      // El scatter usa la vía normal de blueprints: registra la sintetizada
      // para que el engine resuelva el modelo desde su caché compartida.
      send({
        cmd: 'register_blueprint',
        blueprint: {
          blueprint_id: blueprintId,
          model: entry.modelPath,
          template_name: model?.name ?? entry.modelPath,
          category: 'object',
          colision: false,
          physics_type: 'dynamic',
        },
      })
      send({
        cmd: 'world_gen_scatter',
        rules_json: JSON.stringify({
          blueprintId,
          biomes: [] as string[],
          altitudeMin: s.altitudeMin,
          altitudeMax: s.altitudeMax,
          maxSlopeDeg: s.maxSlopeDeg,
          maxInstances: s.maxInstances,
          density: s.density / 100,
          scaleMin: s.scaleMin / 100,
          scaleMax: s.scaleMax / 100,
          randomRotation: s.randomRotation,
          minDistance: s.minDistance,
          cluster: { enabled: s.clusterEnabled, size: s.clusterSize, radius: 0 },
        }),
      })
    }
  }

  const inputsLocked = generation !== null || distributing
  const radius = sizeToRadius(panel.size)
  const caps = featureCaps(radius)
  const biomeSlots = maxBiomeSlots(radius)
  const enabledBiomeCount = BIOME_PRIORITY.reduce(
    (count, id) => count + Number(panel.biomes[id]),
    0,
  )
  const sizeTitle = t('World size (m)')
  let generateLabel = t('Generate world and distribute blueprints')
  if (distributing) {
    generateLabel = t('Distributing blueprints…')
  } else if (generation) {
    if (generation.phase === 'scan') {
      generateLabel =
        generation.total > 0
          ? `${t('Scanning columns…')} ${generation.completed}/${generation.total}`
          : t('Scanning columns…')
    } else {
      generateLabel = `${t('Generating world…')} ${generation.completed}/${generation.total}`
    }
  }

  return (
    <div className="d-flex flex-column gap-2 small">
      {plugin?.libraryPath ? null : (
        <div className="alert alert-danger py-2 mb-0">
          {t('Install the World Generator plugin first')}
        </div>
      )}

      <fieldset
        disabled={inputsLocked}
        className="border-0 p-0 m-0"
        style={{ minInlineSize: 0, pointerEvents: inputsLocked ? 'none' : undefined }}
      >
      {/* Terreno */}
      <fieldset className="border rounded p-2">
        <legend className="fs-5 fw-semibold px-1 mb-1 float-none w-auto mx-auto d-block text-center text-uppercase">{t('Terrain configuration')}</legend>
        <div className="row g-3 mt-1">
          <div className="col-6">
            <fieldset className="border rounded p-2 h-100">
              <legend className="fs-5 fw-semibold px-1 mb-2 float-none w-auto">{t('Size settings')}</legend>
            <div className="d-grid gap-2">
              <div className="min-w-0">
                <label className="form-label m-0" htmlFor="worldgen-seed">{t('Seed')}</label>
                <div className="input-group input-group-sm">
                  <input
                    type="number"
                    id="worldgen-seed"
                    className="form-control"
                    min={0}
                    value={panel.seed}
                    onChange={(e) => setPanel({ ...panel, seed: Math.max(0, Number(e.target.value) || 0) })}
                  />
                  <button
                    type="button"
                    className="btn btn-outline-secondary"
                    aria-label={t('New random seed')}
                    title={t('New random seed')}
                    onClick={() => setPanel({ ...panel, seed: randomSeed() })}
                  >
                    <ArrowClockwise size={14} aria-hidden />
                  </button>
                </div>
              </div>
              <div className="min-w-0">
                <div className="d-flex justify-content-between align-items-baseline">
                  <label className="form-label m-0" htmlFor="worldgen-size">{t('World size (m)')}</label>
                  <span className="text-secondary">{panel.size}</span>
                </div>
                <input
                  id="worldgen-size"
                  type="range"
                  className="form-range"
                  title={sizeTitle}
                  min={WORLDGEN_SIZE_MIN}
                  max={WORLDGEN_SIZE_MAX}
                  step={8}
                  value={panel.size}
                  onChange={(e) =>
                    setPanel(clampPanelToSize(panel, Number(e.target.value) || WORLDGEN_SIZE_MIN))
                  }
                />
              </div>
              <div className="min-w-0">
                <div className="d-flex justify-content-between align-items-baseline">
                  <label className="form-label m-0" htmlFor="worldgen-voxel">{t('Voxel size (m)')}</label>
                  <span className="text-secondary">{panel.voxelSize}</span>
                </div>
                <input
                  id="worldgen-voxel"
                  type="range"
                  className="form-range"
                  min={0.25}
                  max={4}
                  step={0.25}
                  value={panel.voxelSize}
                  onChange={(e) => setPanel({ ...panel, voxelSize: Number(e.target.value) || 1 })}
                />
              </div>
            </div>
            </fieldset>
          </div>
          <div className="col-6">
            <fieldset className="border rounded p-2 h-100">
              <legend className="fs-5 fw-semibold px-1 mb-2 float-none w-auto">{t('Terrain elements')}</legend>
            <div className="d-grid gap-2" style={{ gridTemplateColumns: 'repeat(3, minmax(0, 1fr))' }}>
              {(
                [
                  ['Mountains', 'mountains'],
                  ['Hills', 'hills'],
                  ['Valleys', 'valleys'],
                  ['Plains', 'plains'],
                  ['Cliffs', 'cliffs'],
                  ['Lakes', 'lakes'],
                  ['Rivers', 'riversMaxCount'],
                  ['Caves', 'cavesMaxCount'],
                  ['Waterfalls', 'waterfallsMaxCount'],
                ] as const
              ).map(([label, key]) => (
                <StackedNumber
                  key={key}
                  id={`worldgen-count-${key}`}
                  label={t(label)}
                  min={0}
                  max={caps[key]}
                  value={panel[key]}
                  onChange={(count) =>
                    setPanel({ ...panel, [key]: Math.max(0, Math.min(caps[key], count)) })
                  }
                />
              ))}
            </div>
            </fieldset>
          </div>
        </div>

        <div className="mt-3 ps-3 pb-3">
          <span className="fw-semibold fs-5">{t('Biomes')}</span>
          <div className="text-secondary small mt-1">
            {t('This size fits up to {n} biomes').replace('{n}', String(biomeSlots))}
          </div>
          <div
            className="d-grid gap-3 mt-2"
            style={{ gridTemplateColumns: `repeat(${BIOME_PRIORITY.length}, minmax(0, 1fr))` }}
          >
            {BIOME_PRIORITY.map((biome) => {
              const enabled = panel.biomes[biome]
              const atCap = !enabled && enabledBiomeCount >= biomeSlots
              const shareLocked = !enabled || enabledBiomeCount <= 1
              return (
                <div className="min-w-0" key={biome}>
                  <div className="form-check form-switch m-0" style={{ fontSize: '1.2rem' }}>
                    <input
                      className="form-check-input"
                      type="checkbox"
                      checked={enabled}
                      disabled={atCap}
                      onChange={(e) => {
                        const biomes = clampBiomes({ ...panel.biomes, [biome]: e.target.checked }, radius)
                        setPanel({
                          ...panel,
                          biomes,
                          biomeShares: toggleBiomeShare(panel.biomeShares, biomes, biome),
                        })
                      }}
                      id={`worldgen-biome-${biome}`}
                    />
                    <label className="form-check-label mb-0" htmlFor={`worldgen-biome-${biome}`}>
                      {t(biome)}
                    </label>
                  </div>
                  <div className="d-flex align-items-center gap-2 mt-1">
                    <input
                      type="range"
                      className="form-range"
                      id={`worldgen-biome-share-${biome}`}
                      aria-label={`${t(biome)} — ${t('World share %')}`}
                      title={t('World share %')}
                      min={0}
                      max={100}
                      step={1}
                      disabled={shareLocked}
                      value={panel.biomeShares[biome]}
                      onChange={(e) =>
                        setPanel({
                          ...panel,
                          biomeShares: rebalanceBiomeShares(
                            panel.biomeShares,
                            panel.biomes,
                            biome,
                            Number(e.target.value),
                          ),
                        })
                      }
                    />
                    <span className="text-secondary text-nowrap" style={{ minWidth: '2.6rem' }}>
                      {panel.biomeShares[biome]}%
                    </span>
                  </div>
                </div>
              )
            })}
          </div>
          <div className="text-secondary small mt-1">
            {t('Enabled biomes always add up to 100% of the world, at least {n}% each').replace(
              '{n}',
              String(MIN_BIOME_SHARE),
            )}
          </div>
        </div>
      </fieldset>

      {/* Distribución de blueprints */}
      <fieldset className="border rounded p-2 mt-3">
        <legend className="fs-5 fw-semibold px-1 mb-2 float-none w-auto">
          {t('Distribute blueprints')}
        </legend>
        {models.length === 0 ? (
          <div className="text-muted">{t('No models in the project yet')}</div>
        ) : (
          <div className="input-group input-group-sm">
            <select
              id="worldgen-scatter-model"
              className="form-select"
              value={selectedModelPath}
              aria-label={t('Select model…')}
              onChange={(e) => setSelectedModelPath(e.target.value)}
            >
              <option value="">{t('Select model…')}</option>
              {models.map((model) => (
                <option key={model.path} value={model.path}>
                  {model.name}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="btn btn-outline-info"
              disabled={!selectedModelPath}
              onClick={() => addScatterModel(selectedModelPath)}
            >
              <PlusLg size={14} className="me-1" aria-hidden />
              {t('Add model')}
            </button>
          </div>
        )}
        {scatterEntries.length > 0 && (
          <div className="row g-2 mt-2">
            {scatterEntries.map((entry) => {
              const modelName = models.find((model) => model.path === entry.modelPath)?.name ?? entry.modelPath
              return (
                <div className="col-6" key={entry.id}>
                  <Accordion
                    className="sidebar-accordion sidebar-accordion-sub"
                    activeKey={openScatterId === entry.id ? entry.id : undefined}
                    onSelect={(key) => setOpenScatterId(typeof key === 'string' ? key : null)}
                  >
                    <Accordion.Item eventKey={entry.id}>
                      <Accordion.Header>
                        <span className="d-flex w-100 align-items-center justify-content-between pe-2">
                          <span className="text-truncate">{modelName}</span>
                          <AppTooltip content={t('Remove model')} place="top">
                            <span
                              role="button"
                              tabIndex={0}
                              className="btn btn-sm btn-outline-danger ms-2"
                              aria-label={t('Remove model')}
                              onClick={(event) => {
                                event.stopPropagation()
                                removeScatterModel(entry.id)
                              }}
                              onKeyDown={(event) => {
                                if (event.key !== 'Enter' && event.key !== ' ') return
                                event.preventDefault()
                                event.stopPropagation()
                                removeScatterModel(entry.id)
                              }}
                            >
                              <Trash3 size={12} aria-hidden />
                            </span>
                          </AppTooltip>
                        </span>
                      </Accordion.Header>
                      <Accordion.Body>
                        <ScatterFields
                          entryId={entry.id}
                          rules={entry.rules}
                          onChange={(rules) => updateScatterRules(entry.id, rules)}
                        />
                      </Accordion.Body>
                    </Accordion.Item>
                  </Accordion>
                </div>
              )
            })}
          </div>
        )}
      </fieldset>

      {status && <div className="text-secondary small">{status}</div>}

      <div className="form-check mt-2">
        <input
          className="form-check-input"
          type="checkbox"
          id="worldgen-replace-world"
          checked={panel.replaceWorld}
          onChange={(event) => setPanel({ ...panel, replaceWorld: event.target.checked })}
        />
        <label className="form-check-label" htmlFor="worldgen-replace-world">
          {t('Replace current world')}
        </label>
      </div>
      </fieldset>

      <div className="d-flex gap-2 mt-2">
        <button
          type="button"
          className="btn btn-primary btn-sm flex-grow-1"
          disabled={generation !== null || distributing}
          onClick={() => void onGenerate()}
        >
          {generation || distributing ? (
            <span className="spinner-border spinner-border-sm me-1" role="status" aria-hidden />
          ) : (
            <PlayFill size={14} className="me-1" aria-hidden />
          )}
          {generateLabel}
        </button>
        <button type="button" className="btn btn-outline-secondary btn-sm" onClick={onStop}>
          <StopFill size={14} className="me-1" aria-hidden />
          {t('Stop')}
        </button>
        <button type="button" className="btn btn-outline-danger btn-sm" onClick={onReset} disabled={inputsLocked}>
          <Trash3 size={14} className="me-1" aria-hidden />
          {t('Clear')}
        </button>
      </div>
    </div>
  )
}

/**
 * Abre esta modal en la ventana modal Electron (la instancia visible se
 * reemplaza). Pensado para el botón «Ejecutar plugin» de la tarjeta del
 * plugin dentro de la propia modal de plugins.
 */
export function openWorldGenPluginModal(title: string, locale: 'en' | 'es'): void {
  void window.electronAPI.openModalElectron({
    title,
    size: 'lg',
    handlerId: crypto.randomUUID(),
    componentKey: 'WorldGenModalBody',
    locale,
    props: {},
  })
}
