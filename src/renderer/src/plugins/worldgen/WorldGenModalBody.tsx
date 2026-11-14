import { useCallback, useEffect, useState } from 'react'

import { useTraslate } from '@hooks'
import type { EngineCommand3D, EngineEvent, ModelInfo } from '@shared-types'

/**
 * Modal del plugin world-gen (ventana Electron hija).
 *
 * Solo habla con el engine vía `window.engine3d.send3d` (los comandos
 * `world_gen_*` se gating en el proceso main) y con el estado de plugins vía
 * `pluginsGetState` para obtener la ruta de la DLL instalada.
 */

interface FeatureState {
  enabled: boolean
  maxCount: number
}

interface PanelState {
  seed: number
  width: number
  length: number
  height: number
  voxelSize: number
  mountains: FeatureState
  hills: FeatureState
  valleys: FeatureState
  plains: FeatureState
  cliffs: FeatureState
  lakes: FeatureState
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

const DEFAULT_PANEL: PanelState = {
  seed: 1,
  width: 512,
  length: 512,
  height: 128,
  voxelSize: 1,
  mountains: { enabled: true, maxCount: 15 },
  hills: { enabled: true, maxCount: 30 },
  valleys: { enabled: true, maxCount: 8 },
  plains: { enabled: true, maxCount: 10 },
  cliffs: { enabled: false, maxCount: 12 },
  lakes: { enabled: true, maxCount: 8 },
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

/** Id estable de la blueprint sintetizada para un modelo del proyecto. */
function scatterBlueprintId(modelPath: string): string {
  return `worldgen:${modelPath}`
}

function FeatureRow({
  label,
  value,
  onChange,
}: {
  label: string
  value: FeatureState
  onChange: (next: FeatureState) => void
}) {
  const { t } = useTraslate()
  return (
    <div className="d-flex align-items-center gap-2 py-1">
      <div className="form-check form-switch m-0">
        <input
          className="form-check-input"
          type="checkbox"
          checked={value.enabled}
          onChange={(e) => onChange({ ...value, enabled: e.target.checked })}
          id={`worldgen-feature-${label}`}
        />
      </div>
      <label className="flex-grow-1 small mb-0" htmlFor={`worldgen-feature-${label}`}>
        {label}
      </label>
      <input
        type="number"
        className="form-control form-control-sm"
        style={{ width: 76 }}
        min={0}
        value={value.maxCount}
        disabled={!value.enabled}
        title={t('Maximum count')}
        onChange={(e) => onChange({ ...value, maxCount: Math.max(0, Number(e.target.value) || 0) })}
      />
    </div>
  )
}

export function WorldGenModalBody() {
  const { t } = useTraslate()
  const [panel, setPanel] = useState<PanelState>(DEFAULT_PANEL)
  const [scatter, setScatter] = useState<ScatterRulesState>(DEFAULT_SCATTER)
  const [models, setModels] = useState<ModelInfo[]>([])
  const [scatterModelPath, setScatterModelPath] = useState('')
  const [status, setStatus] = useState('')
  const [plugin, setPlugin] = useState<{ libraryPath: string | null; enabled: boolean } | null>(null)

  // Ref estable para window.engine3d.off.
  const onEngineEvent = useCallback((event: EngineEvent) => {
    if (event.event === 'models_list') {
      setModels((event as { models?: ModelInfo[] }).models ?? [])
    }
  }, [])

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
        enabled: state.enabled.includes('world-gen'),
      })
    }
    void check()
  }, [])

  const send = (cmd: EngineCommand3D) => {
    window.engine3d.send(cmd)
  }

  /** Guard del gating del proceso main: plugin instalado Y activado. */
  const resolvePlugin = async (): Promise<boolean> => {
    const state = await window.electronAPI.pluginsGetState()
    const record = state.installed['world-gen']
    const libraryPath = record?.libraryPath ?? null
    setPlugin({
      libraryPath,
      enabled: state.enabled.includes('world-gen'),
    })
    if (!libraryPath) {
      setStatus(t('Install the World Generator plugin first'))
      return false
    }
    if (!state.enabled.includes('world-gen')) {
      setStatus(t('The World Generator plugin is installed but disabled. Enable it in Plugins.'))
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
        enabled: p.mountains.enabled,
        maxCount: p.mountains.maxCount,
        minSize: 60,
        maxSize: 180,
        minSeparation: 120,
        maxHeight: 90,
        sharpness: 3,
      },
      hills: {
        enabled: p.hills.enabled,
        maxCount: p.hills.maxCount,
        minSize: 40,
        maxSize: 120,
        minSeparation: 80,
        maxHeight: 24,
      },
      valleys: {
        enabled: p.valleys.enabled,
        maxCount: p.valleys.maxCount,
        minSize: 80,
        maxSize: 200,
        minSeparation: 150,
        maxDepth: 30,
      },
      plains: {
        enabled: p.plains.enabled,
        maxCount: p.plains.maxCount,
        minSize: 100,
        maxSize: 260,
        minSeparation: 120,
        maxRoughness: 1.5,
      },
      cliffs: {
        enabled: p.cliffs.enabled,
        maxCount: p.cliffs.maxCount,
        minSize: 30,
        maxSize: 120,
        minSeparation: 100,
        dropHeight: 18,
      },
      lakes: {
        enabled: p.lakes.enabled,
        maxCount: p.lakes.maxCount,
        minSize: 20,
        maxSize: 90,
        minSeparation: 100,
        maxDepth: 12,
      },
      rivers: {
        enabled: p.riversMaxCount > 0,
        maxCount: p.riversMaxCount,
        lengthMin: 150,
        lengthMax: 600,
        width: 4,
        depth: 3,
      },
      caves: {
        enabled: p.cavesMaxCount > 0,
        maxCount: p.cavesMaxCount,
        depthMin: 10,
        depthMax: 50,
        radius: 3,
        roomChance: 0.2,
        segments: 18,
      },
      waterfalls: {
        enabled: p.waterfallsMaxCount > 0,
        maxCount: p.waterfallsMaxCount,
        minDrop: 6,
      },
      biomes: p.biomes,
    })
  }

  const onGenerate = async () => {
    if (!(await resolvePlugin())) return
    send({ cmd: 'world_gen_set_config', config_json: buildConfigJson() })
    send({ cmd: 'world_gen_start' })
    setStatus(t('Generating world…'))
  }

  const onStop = () => {
    send({ cmd: 'world_gen_stop' })
    setStatus(t('Generation cancelled'))
  }

  const onReset = () => {
    send({ cmd: 'world_gen_reset' })
    setStatus(t('World cleared'))
  }

  const onScatter = async () => {
    if (!scatterModelPath) {
      setStatus(t('Select a model to distribute'))
      return
    }
    if (!(await resolvePlugin())) return
    const model = models.find((entry) => entry.path === scatterModelPath)
    const blueprintId = scatterBlueprintId(scatterModelPath)
    // El scatter usa la vía normal de blueprints: registra la sintetizada
    // para que el engine resuelva el modelo desde su caché compartida.
    send({
      cmd: 'register_blueprint',
      blueprint: {
        blueprint_id: blueprintId,
        model: scatterModelPath,
        template_name: model?.name ?? scatterModelPath,
        category: 'object',
        colision: false,
        physics_type: 'dynamic',
      },
    })
    const s = scatter
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
    setStatus(t('Distributing blueprints…'))
  }

  const pluginReady = plugin?.libraryPath && plugin.enabled

  return (
    <div className="d-flex flex-column gap-2 small">
      {/* Estado del plugin */}
      {pluginReady ? (
        <div className="text-success">
          ✓ {t('Plugin ready')}: {plugin?.libraryPath?.split(/[\\/]/).pop()}
        </div>
      ) : plugin?.libraryPath ? (
        <div className="alert alert-warning py-2 mb-0">
          {t('The World Generator plugin is installed but disabled. Enable it in Plugins.')}
        </div>
      ) : (
        <div className="alert alert-danger py-2 mb-0">
          {t('Install the World Generator plugin first')}
        </div>
      )}

      {/* Terreno */}
      <fieldset className="border rounded p-2">
        <legend className="small fw-semibold px-1 mb-1 float-none w-auto">{t('Terrain')}</legend>
        <div className="row g-2">
          <div className="col-4">
            <label className="form-label m-0" htmlFor="worldgen-seed">{t('Seed')}</label>
            <input
              type="number"
              id="worldgen-seed"
              className="form-control form-control-sm"
              min={0}
              value={panel.seed}
              onChange={(e) => setPanel({ ...panel, seed: Math.max(0, Number(e.target.value) || 0) })}
            />
          </div>
          <div className="col-8">
            <label className="form-label m-0">{t('Max size (W × L × H, m)')}</label>
            <div className="d-flex gap-1">
              <input
                type="number"
                className="form-control form-control-sm"
                aria-label={t('Width')}
                value={panel.width}
                onChange={(e) => setPanel({ ...panel, width: Number(e.target.value) || 0 })}
              />
              <input
                type="number"
                className="form-control form-control-sm"
                aria-label={t('Length')}
                value={panel.length}
                onChange={(e) => setPanel({ ...panel, length: Number(e.target.value) || 0 })}
              />
              <input
                type="number"
                className="form-control form-control-sm"
                aria-label={t('Height')}
                value={panel.height}
                onChange={(e) => setPanel({ ...panel, height: Number(e.target.value) || 0 })}
              />
            </div>
          </div>
        </div>
        <div className="row g-2 mt-1">
          <div className="col-4">
            <label className="form-label m-0" htmlFor="worldgen-voxel">{t('Voxel size (m)')}</label>
            <input
              type="number"
              id="worldgen-voxel"
              step={0.25}
              min={0.25}
              max={4}
              className="form-control form-control-sm"
              value={panel.voxelSize}
              onChange={(e) => setPanel({ ...panel, voxelSize: Number(e.target.value) || 1 })}
            />
          </div>
        </div>

        <div className="row g-2 mt-1">
          <div className="col-md-6">
            <FeatureRow
              label={t('Mountains')}
              value={panel.mountains}
              onChange={(next) => setPanel({ ...panel, mountains: next })}
            />
            <FeatureRow
              label={t('Hills')}
              value={panel.hills}
              onChange={(next) => setPanel({ ...panel, hills: next })}
            />
            <FeatureRow
              label={t('Valleys')}
              value={panel.valleys}
              onChange={(next) => setPanel({ ...panel, valleys: next })}
            />
          </div>
          <div className="col-md-6">
            <FeatureRow
              label={t('Plains')}
              value={panel.plains}
              onChange={(next) => setPanel({ ...panel, plains: next })}
            />
            <FeatureRow
              label={t('Cliffs')}
              value={panel.cliffs}
              onChange={(next) => setPanel({ ...panel, cliffs: next })}
            />
            <FeatureRow
              label={t('Lakes')}
              value={panel.lakes}
              onChange={(next) => setPanel({ ...panel, lakes: next })}
            />
          </div>
        </div>

        <div className="row g-2 mt-1">
          {(
            [
              ['Rivers', 'riversMaxCount'],
              ['Caves', 'cavesMaxCount'],
              ['Waterfalls', 'waterfallsMaxCount'],
            ] as const
          ).map(([label, key]) => (
            <div className="col-4" key={key}>
              <label className="form-label m-0" htmlFor={`worldgen-count-${key}`}>{t(label)}</label>
              <input
                type="number"
                id={`worldgen-count-${key}`}
                className="form-control form-control-sm"
                min={0}
                value={panel[key]}
                onChange={(e) =>
                  setPanel({ ...panel, [key]: Math.max(0, Number(e.target.value) || 0) })
                }
              />
            </div>
          ))}
        </div>

        <div className="mt-2">
          <span className="fw-semibold">{t('Biomes')}</span>
          <div className="d-flex flex-wrap gap-3 mt-1">
            {(['grassland', 'forest', 'desert', 'snow', 'volcanic'] as const).map((biome) => (
              <div className="form-check form-switch m-0" key={biome}>
                <input
                  className="form-check-input"
                  type="checkbox"
                  checked={panel.biomes[biome]}
                  onChange={(e) =>
                    setPanel({ ...panel, biomes: { ...panel.biomes, [biome]: e.target.checked } })
                  }
                  id={`worldgen-biome-${biome}`}
                />
                <label className="form-check-label small mb-0" htmlFor={`worldgen-biome-${biome}`}>
                  {t(biome)}
                </label>
              </div>
            ))}
          </div>
        </div>
      </fieldset>

      {/* Acciones de terreno */}
      <div className="d-flex gap-2">
        <button
          type="button"
          className="btn btn-primary btn-sm flex-grow-1"
          onClick={() => void onGenerate()}
        >
          {t('Generate world')}
        </button>
        <button type="button" className="btn btn-outline-secondary btn-sm" onClick={onStop}>
          {t('Stop')}
        </button>
        <button type="button" className="btn btn-outline-danger btn-sm" onClick={onReset}>
          {t('Clear')}
        </button>
      </div>

      {/* Distribución de blueprints */}
      <fieldset className="border rounded p-2">
        <legend className="small fw-semibold px-1 mb-1 float-none w-auto">
          {t('Distribute blueprints')}
        </legend>
        <label className="form-label m-0" htmlFor="worldgen-scatter-model">{t('Model')}</label>
        <select
          id="worldgen-scatter-model"
          className="form-select form-select-sm mb-2"
          value={scatterModelPath}
          onChange={(e) => setScatterModelPath(e.target.value)}
        >
          <option value="">{t('Select model…')}</option>
          {models.map((model) => (
            <option key={model.path} value={model.path}>
              {model.name}
            </option>
          ))}
        </select>
        {models.length === 0 && (
          <div className="text-muted mb-2">{t('No models in the project yet')}</div>
        )}
        <div className="row g-2">
          <div className="col-6">
            <label className="form-label m-0" htmlFor="worldgen-max-instances">
              {t('Max instances')}
            </label>
            <input
              type="number"
              id="worldgen-max-instances"
              className="form-control form-control-sm"
              min={0}
              value={scatter.maxInstances}
              onChange={(e) =>
                setScatter({ ...scatter, maxInstances: Math.max(0, Number(e.target.value) || 0) })
              }
            />
          </div>
          <div className="col-6">
            <label className="form-label m-0" htmlFor="worldgen-min-distance">
              {t('Min distance (m)')}
            </label>
            <input
              type="number"
              id="worldgen-min-distance"
              step={0.5}
              className="form-control form-control-sm"
              value={scatter.minDistance}
              onChange={(e) =>
                setScatter({ ...scatter, minDistance: Number(e.target.value) || 0 })
              }
            />
          </div>
          <div className="col-6">
            <label className="form-label m-0" htmlFor="worldgen-density">{t('Density %')}</label>
            <input
              type="number"
              id="worldgen-density"
              className="form-control form-control-sm"
              min={0}
              max={100}
              value={scatter.density}
              onChange={(e) =>
                setScatter({ ...scatter, density: Math.max(0, Number(e.target.value) || 0) })
              }
            />
          </div>
          <div className="col-6">
            <label className="form-label m-0" htmlFor="worldgen-max-slope">
              {t('Max slope (°)')}
            </label>
            <input
              type="number"
              id="worldgen-max-slope"
              className="form-control form-control-sm"
              min={0}
              max={90}
              value={scatter.maxSlopeDeg}
              onChange={(e) =>
                setScatter({ ...scatter, maxSlopeDeg: Number(e.target.value) || 0 })
              }
            />
          </div>
          <div className="col-6">
            <label className="form-label m-0" htmlFor="worldgen-altitude-min">
              {t('Min altitude (m)')}
            </label>
            <input
              type="number"
              id="worldgen-altitude-min"
              className="form-control form-control-sm"
              value={scatter.altitudeMin}
              onChange={(e) =>
                setScatter({ ...scatter, altitudeMin: Number(e.target.value) || 0 })
              }
            />
          </div>
          <div className="col-6">
            <label className="form-label m-0" htmlFor="worldgen-altitude-max">
              {t('Max altitude (m)')}
            </label>
            <input
              type="number"
              id="worldgen-altitude-max"
              className="form-control form-control-sm"
              value={scatter.altitudeMax}
              onChange={(e) =>
                setScatter({ ...scatter, altitudeMax: Number(e.target.value) || 0 })
              }
            />
          </div>
          <div className="col-6">
            <label className="form-label m-0" htmlFor="worldgen-scale-min">
              {t('Min scale %')}
            </label>
            <input
              type="number"
              id="worldgen-scale-min"
              className="form-control form-control-sm"
              value={scatter.scaleMin}
              onChange={(e) =>
                setScatter({ ...scatter, scaleMin: Number(e.target.value) || 100 })
              }
            />
          </div>
          <div className="col-6">
            <label className="form-label m-0" htmlFor="worldgen-scale-max">
              {t('Max scale %')}
            </label>
            <input
              type="number"
              id="worldgen-scale-max"
              className="form-control form-control-sm"
              value={scatter.scaleMax}
              onChange={(e) =>
                setScatter({ ...scatter, scaleMax: Number(e.target.value) || 100 })
              }
            />
          </div>
        </div>
        <div className="d-flex flex-wrap gap-3 mt-2">
          <div className="form-check form-switch m-0">
            <input
              className="form-check-input"
              type="checkbox"
              checked={scatter.randomRotation}
              onChange={(e) => setScatter({ ...scatter, randomRotation: e.target.checked })}
              id="worldgen-random-rotation"
            />
            <label className="form-check-label small mb-0" htmlFor="worldgen-random-rotation">
              {t('Random rotation')}
            </label>
          </div>
          <div className="form-check form-switch m-0">
            <input
              className="form-check-input"
              type="checkbox"
              checked={scatter.clusterEnabled}
              onChange={(e) => setScatter({ ...scatter, clusterEnabled: e.target.checked })}
              id="worldgen-cluster"
            />
            <label className="form-check-label small mb-0" htmlFor="worldgen-cluster">
              {t('Clusters')}
            </label>
          </div>
          {scatter.clusterEnabled && (
            <div className="d-flex align-items-center gap-2">
              <label className="m-0" htmlFor="worldgen-cluster-size">{t('Cluster size')}</label>
              <input
                type="number"
                id="worldgen-cluster-size"
                className="form-control form-control-sm"
                style={{ width: 76 }}
                min={2}
                value={scatter.clusterSize}
                onChange={(e) =>
                  setScatter({ ...scatter, clusterSize: Math.max(2, Number(e.target.value) || 2) })
                }
              />
            </div>
          )}
        </div>
        <button
          type="button"
          className="btn btn-outline-info btn-sm w-100 mt-2"
          onClick={() => void onScatter()}
        >
          {t('Distribute in world')}
        </button>
      </fieldset>

      {status && <div className="text-secondary small">{status}</div>}
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
