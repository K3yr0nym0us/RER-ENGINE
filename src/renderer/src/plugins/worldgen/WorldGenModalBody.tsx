import { useCallback, useEffect, useRef, useState } from 'react'
import { Accordion } from 'react-bootstrap'
import { ArrowClockwise, PlayFill, PlusLg, StopFill, Trash3 } from 'react-bootstrap-icons'

import { useTraslate } from '@hooks'
import type { EngineCommand3D, EngineEvent, ModelInfo } from '@shared-types'

/**
 * Modal del plugin world-gen (ventana Electron hija).
 *
 * Solo habla con el engine vía `window.engine3d.send3d` (los comandos
 * `world_gen_*` se gating en el proceso main) y con el estado de plugins vía
 * `pluginsGetState` para obtener la ruta de la DLL instalada.
 */

interface PanelState {
  seed: number
  width: number
  length: number
  height: number
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

const DEFAULT_PANEL: PanelState = {
  seed: 1,
  width: 512,
  length: 512,
  height: 128,
  voxelSize: 1,
  mountains: 15,
  hills: 30,
  valleys: 8,
  plains: 10,
  cliffs: 0,
  lakes: 8,
  riversMaxCount: 5,
  cavesMaxCount: 20,
  waterfallsMaxCount: 3,
  biomes: { forest: true, desert: false, grassland: true, snow: false, volcanic: false },
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

export function WorldGenModalBody() {
  const { t } = useTraslate()
  const [panel, setPanel] = useState<PanelState>(() => ({ ...DEFAULT_PANEL, seed: randomSeed() }))
  const [scatterEntries, setScatterEntries] = useState<ScatterEntry[]>([])
  const [openScatterId, setOpenScatterId] = useState<string | null>(null)
  const [selectedModelPath, setSelectedModelPath] = useState('')
  const [models, setModels] = useState<ModelInfo[]>([])
  const [status, setStatus] = useState('')
  const [generation, setGeneration] = useState<{ completed: number; total: number } | null>(null)
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
      setGeneration({
        completed: Number(event.completed ?? 0),
        total: Number(event.total ?? 0),
      })
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
    return JSON.stringify({
      seed: p.seed,
      bounds: { width: p.width, length: p.length, height: p.height },
      resolution: { voxelSize: p.voxelSize, lodEnabled: true },
      mountains: {
        ...countedFeature(p.mountains),
        minSize: 60,
        maxSize: 180,
        minSeparation: 120,
        maxHeight: 90,
        sharpness: 3,
      },
      hills: {
        ...countedFeature(p.hills),
        minSize: 40,
        maxSize: 120,
        minSeparation: 80,
        maxHeight: 24,
      },
      valleys: {
        ...countedFeature(p.valleys),
        minSize: 80,
        maxSize: 200,
        minSeparation: 150,
        maxDepth: 30,
      },
      plains: {
        ...countedFeature(p.plains),
        minSize: 100,
        maxSize: 260,
        minSeparation: 120,
        maxRoughness: 1.5,
      },
      cliffs: {
        ...countedFeature(p.cliffs),
        minSize: 30,
        maxSize: 120,
        minSeparation: 100,
        dropHeight: 18,
      },
      lakes: {
        ...countedFeature(p.lakes),
        minSize: 20,
        maxSize: 90,
        minSeparation: 100,
        maxDepth: 12,
      },
      rivers: {
        ...countedFeature(p.riversMaxCount),
        lengthMin: 150,
        lengthMax: 600,
        width: 4,
        depth: 3,
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
      biomes: p.biomes,
    })
  }

  const onGenerate = async () => {
    if (!(await resolvePlugin())) return
    send({ cmd: 'world_gen_set_config', config_json: buildConfigJson() })
    send({ cmd: 'world_gen_start' })
    finishHandled.current = false
    pendingScatter.current = 0
    setDistributing(false)
    setStatus('')
    setGeneration({ completed: 0, total: 0 })
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

  const maxSizeTitle = t('Max size (W × L × H, m)')
  let generateLabel = t('Generate world and distribute blueprints')
  if (distributing) {
    generateLabel = t('Distributing blueprints…')
  } else if (generation) {
    generateLabel = generation.total > 0
      ? `${t('Generating world…')} ${generation.completed}/${generation.total}`
      : t('Scanning columns…')
  }

  return (
    <div className="d-flex flex-column gap-2 small">
      {plugin?.libraryPath ? null : (
        <div className="alert alert-danger py-2 mb-0">
          {t('Install the World Generator plugin first')}
        </div>
      )}

      {/* Terreno */}
      <fieldset className="border rounded p-2">
        <legend className="fs-5 fw-semibold px-1 mb-1 float-none w-auto mx-auto d-block text-center text-uppercase">{t('Terrain configuration')}</legend>
        <div className="row g-3 mt-1">
          <div className="col-6">
            <fieldset className="border rounded p-2 h-100">
              <legend className="fs-5 fw-semibold px-1 mb-2 float-none w-auto">{t('Size settings')}</legend>
            <div className="d-grid gap-2" style={{ gridTemplateColumns: 'repeat(2, minmax(0, 1fr))' }}>
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
              <StackedNumber
                id="worldgen-width"
                label={t('Width')}
                title={maxSizeTitle}
                value={panel.width}
                onChange={(width) => setPanel({ ...panel, width })}
              />
              <StackedNumber
                id="worldgen-length"
                label={t('Length')}
                title={maxSizeTitle}
                value={panel.length}
                onChange={(length) => setPanel({ ...panel, length })}
              />
              <StackedNumber
                id="worldgen-height"
                label={t('Height')}
                title={maxSizeTitle}
                value={panel.height}
                onChange={(height) => setPanel({ ...panel, height })}
              />
              <div className="min-w-0" style={{ gridColumn: '1 / -1' }}>
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
                  value={panel[key]}
                  onChange={(count) => setPanel({ ...panel, [key]: Math.max(0, count) })}
                />
              ))}
            </div>
            </fieldset>
          </div>
        </div>

        <div className="mt-3 ps-3 pb-3">
          <span className="fw-semibold fs-5">{t('Biomes')}</span>
          <div className="d-flex flex-wrap gap-4 mt-2">
            {(['grassland', 'forest', 'desert', 'snow', 'volcanic'] as const).map((biome) => (
              <div className="form-check form-switch m-0" key={biome} style={{ fontSize: '1.2rem' }}>
                <input
                  className="form-check-input"
                  type="checkbox"
                  checked={panel.biomes[biome]}
                  onChange={(e) =>
                    setPanel({ ...panel, biomes: { ...panel.biomes, [biome]: e.target.checked } })
                  }
                  id={`worldgen-biome-${biome}`}
                />
                <label className="form-check-label mb-0" htmlFor={`worldgen-biome-${biome}`}>
                  {t(biome)}
                </label>
              </div>
            ))}
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
        <button type="button" className="btn btn-outline-danger btn-sm" onClick={onReset}>
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
