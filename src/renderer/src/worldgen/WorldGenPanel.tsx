import { useMemo, useState } from 'react'

import { useTraslate } from '@hooks'
import { useContextEngine } from '@engine'
import type { BluePrintEntry } from '@shared-types'

/**
 * Panel del plugin world-gen: configuración del terreno, generación y
 * distribución de blueprints. Solo envía comandos `world_gen_*`; el gating de
 * permisos vive en el proceso main (`engineIpcRegistry.ts`).
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

const DEFAULT_STATE: PanelState = {
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

function featureBlock(
  t: (key: string) => string,
  key: 'mountains' | 'hills' | 'valleys' | 'plains' | 'cliffs' | 'lakes',
  label: string,
  value: FeatureState,
  onChange: (next: FeatureState) => void,
) {
  return (
    <div className="d-flex align-items-center gap-2 py-1" key={key}>
      <div className="form-check form-switch m-0">
        <input
          className="form-check-input"
          type="checkbox"
          checked={value.enabled}
          onChange={(e) => onChange({ ...value, enabled: e.target.checked })}
          id={`worldgen-${key}-enabled`}
        />
      </div>
      <label className="flex-grow-1 small" htmlFor={`worldgen-${key}-enabled`}>
        {t(label)}
      </label>
      <input
        type="number"
        className="form-control form-control-sm"
        style={{ width: 76 }}
        min={0}
        value={value.maxCount}
        disabled={!value.enabled}
        onChange={(e) => onChange({ ...value, maxCount: Math.max(0, Number(e.target.value) || 0) })}
        title={t('Maximum count')}
      />
    </div>
  )
}

export function WorldGenPanelBody() {
  const { t } = useTraslate()
  const { send, projectType, blueprints } = useContextEngine() as {
    send: (cmd: { cmd: string } & Record<string, unknown>) => void
    projectType?: '2D' | '3D'
    blueprints: BluePrintEntry[]
  }
  const [panel, setPanel] = useState<PanelState>(DEFAULT_STATE)
  const [status, setStatus] = useState<string>('')
  const [scatterBlueprintId, setScatterBlueprintId] = useState<string>('')
  const [scatterRules, setScatterRules] = useState({
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
  })

  const is3D = projectType === '3D'

  const blueprintOptions = useMemo(
    () => blueprints.map((b) => ({ id: b.id, name: b.name })),
    [blueprints],
  )

  const setFeature = (key: keyof PanelState) => (next: FeatureState) => {
    setPanel((prev) => ({ ...prev, [key]: next }))
  }

  /** Ruta de la DLL instalada (del estado del sistema de plugins). */
  const resolveLibraryPath = async (): Promise<string | null> => {
    const state = await window.electronAPI.pluginsGetState()
    return state.installed['world-gen']?.libraryPath ?? null
  }

  const buildConfigJson = (): string => {
    const p = panel
    const config = {
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
    }
    return JSON.stringify(config)
  }

  const onGenerate = async () => {
    if (!is3D) {
      setStatus(t('World generation is only available in 3D projects'))
      return
    }
    const libraryPath = await resolveLibraryPath()
    if (!libraryPath) {
      setStatus(t('Install the World Generator plugin first'))
      return
    }
    send({ cmd: 'world_gen_set_library_path', path: libraryPath })
    send({ cmd: 'world_gen_set_config', configJson: buildConfigJson() })
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
    if (!is3D || !scatterBlueprintId) {
      setStatus(t('Select a blueprint to distribute'))
      return
    }
    const libraryPath = await resolveLibraryPath()
    if (!libraryPath) {
      setStatus(t('Install the World Generator plugin first'))
      return
    }
    const s = scatterRules
    const rules = {
      blueprintId: scatterBlueprintId,
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
      cluster: {
        enabled: s.clusterEnabled,
        size: s.clusterSize,
        radius: 0,
      },
    }
    send({ cmd: 'world_gen_set_library_path', path: libraryPath })
    send({ cmd: 'world_gen_scatter', rulesJson: JSON.stringify(rules) })
    setStatus(t('Distributing blueprint…'))
  }

  return (
    <div className="p-2 d-flex flex-column gap-3 small" style={{ minHeight: 320 }}>
      {!is3D && <div className="alert alert-warning m-0 p-2">{t('World generation is only available in 3D projects')}</div>}

      {/* Terreno */}
      <fieldset className="border rounded p-2">
        <legend className="small fw-semibold px-1 mb-1">{t('Terrain')}</legend>
        <div className="row g-2">
          <div className="col-4">
            <label className="form-label m-0">{t('Seed')}</label>
            <input
              type="number"
              className="form-control form-control-sm"
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
                value={panel.width}
                onChange={(e) => setPanel({ ...panel, width: Number(e.target.value) || 0 })}
              />
              <input
                type="number"
                className="form-control form-control-sm"
                value={panel.length}
                onChange={(e) => setPanel({ ...panel, length: Number(e.target.value) || 0 })}
              />
              <input
                type="number"
                className="form-control form-control-sm"
                value={panel.height}
                onChange={(e) => setPanel({ ...panel, height: Number(e.target.value) || 0 })}
              />
            </div>
          </div>
        </div>
        <div className="row g-2 mt-1">
          <div className="col-6">
            <label className="form-label m-0">{t('Voxel size (m)')}</label>
            <input
              type="number"
              step={0.25}
              min={0.25}
              max={4}
              className="form-control form-control-sm"
              value={panel.voxelSize}
              onChange={(e) => setPanel({ ...panel, voxelSize: Number(e.target.value) || 1 })}
            />
          </div>
        </div>

        <div className="mt-2">
          {featureBlock(t, 'mountains', 'Mountains', panel.mountains, setFeature('mountains'))}
          {featureBlock(t, 'hills', 'Hills', panel.hills, setFeature('hills'))}
          {featureBlock(t, 'valleys', 'Valleys', panel.valleys, setFeature('valleys'))}
          {featureBlock(t, 'plains', 'Plains', panel.plains, setFeature('plains'))}
          {featureBlock(t, 'cliffs', 'Cliffs', panel.cliffs, setFeature('cliffs'))}
          {featureBlock(t, 'lakes', 'Lakes', panel.lakes, setFeature('lakes'))}
          <div className="d-flex align-items-center gap-2 py-1">
            <span className="flex-grow-1 small">{t('Rivers')}</span>
            <input
              type="number"
              className="form-control form-control-sm"
              style={{ width: 76 }}
              min={0}
              value={panel.riversMaxCount}
              onChange={(e) =>
                setPanel({ ...panel, riversMaxCount: Math.max(0, Number(e.target.value) || 0) })
              }
            />
          </div>
          <div className="d-flex align-items-center gap-2 py-1">
            <span className="flex-grow-1 small">{t('Caves')}</span>
            <input
              type="number"
              className="form-control form-control-sm"
              style={{ width: 76 }}
              min={0}
              value={panel.cavesMaxCount}
              onChange={(e) =>
                setPanel({ ...panel, cavesMaxCount: Math.max(0, Number(e.target.value) || 0) })
              }
            />
          </div>
          <div className="d-flex align-items-center gap-2 py-1">
            <span className="flex-grow-1 small">{t('Waterfalls')}</span>
            <input
              type="number"
              className="form-control form-control-sm"
              style={{ width: 76 }}
              min={0}
              value={panel.waterfallsMaxCount}
              onChange={(e) =>
                setPanel({ ...panel, waterfallsMaxCount: Math.max(0, Number(e.target.value) || 0) })
              }
            />
          </div>
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
                    setPanel({
                      ...panel,
                      biomes: { ...panel.biomes, [biome]: e.target.checked },
                    })
                  }
                  id={`worldgen-biome-${biome}`}
                />
                <label className="form-check-label small" htmlFor={`worldgen-biome-${biome}`}>
                  {t(biome)}
                </label>
              </div>
            ))}
          </div>
        </div>
      </fieldset>

      {/* Acciones de terreno */}
      <div className="d-flex gap-2">
        <button type="button" className="btn btn-primary btn-sm flex-grow-1" onClick={onGenerate} disabled={!is3D}>
          {t('Generate world')}
        </button>
        <button type="button" className="btn btn-outline-secondary btn-sm" onClick={onStop} disabled={!is3D}>
          {t('Stop')}
        </button>
        <button type="button" className="btn btn-outline-danger btn-sm" onClick={onReset} disabled={!is3D}>
          {t('Clear')}
        </button>
      </div>

      {/* Scatter de blueprints */}
      <fieldset className="border rounded p-2">
        <legend className="small fw-semibold px-1 mb-1">{t('Distribute blueprints')}</legend>
        {blueprintOptions.length === 0 ? (
          <div className="text-muted">{t('No blueprints in the project yet')}</div>
        ) : (
          <>
            <select
              className="form-select form-select-sm mb-2"
              value={scatterBlueprintId}
              onChange={(e) => setScatterBlueprintId(e.target.value)}
            >
              <option value="">{t('Select blueprint…')}</option>
              {blueprintOptions.map((b) => (
                <option key={b.id} value={b.id}>
                  {b.name}
                </option>
              ))}
            </select>
            <div className="row g-2">
              <div className="col-6">
                <label className="form-label m-0">{t('Max instances')}</label>
                <input
                  type="number"
                  className="form-control form-control-sm"
                  min={0}
                  value={scatterRules.maxInstances}
                  onChange={(e) =>
                    setScatterRules({ ...scatterRules, maxInstances: Math.max(0, Number(e.target.value) || 0) })
                  }
                />
              </div>
              <div className="col-6">
                <label className="form-label m-0">{t('Min distance (m)')}</label>
                <input
                  type="number"
                  step={0.5}
                  className="form-control form-control-sm"
                  value={scatterRules.minDistance}
                  onChange={(e) =>
                    setScatterRules({ ...scatterRules, minDistance: Number(e.target.value) || 0 })
                  }
                />
              </div>
              <div className="col-6">
                <label className="form-label m-0">{t('Density %')}</label>
                <input
                  type="number"
                  className="form-control form-control-sm"
                  min={0}
                  max={100}
                  value={scatterRules.density}
                  onChange={(e) =>
                    setScatterRules({ ...scatterRules, density: Math.max(0, Number(e.target.value) || 0) })
                  }
                />
              </div>
              <div className="col-6">
                <label className="form-label m-0">{t('Max slope (°)')}</label>
                <input
                  type="number"
                  className="form-control form-control-sm"
                  min={0}
                  max={90}
                  value={scatterRules.maxSlopeDeg}
                  onChange={(e) =>
                    setScatterRules({ ...scatterRules, maxSlopeDeg: Number(e.target.value) || 0 })
                  }
                />
              </div>
              <div className="col-6">
                <label className="form-label m-0">{t('Min altitude (m)')}</label>
                <input
                  type="number"
                  className="form-control form-control-sm"
                  value={scatterRules.altitudeMin}
                  onChange={(e) =>
                    setScatterRules({ ...scatterRules, altitudeMin: Number(e.target.value) || 0 })
                  }
                />
              </div>
              <div className="col-6">
                <label className="form-label m-0">{t('Max altitude (m)')}</label>
                <input
                  type="number"
                  className="form-control form-control-sm"
                  value={scatterRules.altitudeMax}
                  onChange={(e) =>
                    setScatterRules({ ...scatterRules, altitudeMax: Number(e.target.value) || 0 })
                  }
                />
              </div>
              <div className="col-6">
                <label className="form-label m-0">{t('Min scale %')}</label>
                <input
                  type="number"
                  className="form-control form-control-sm"
                  value={scatterRules.scaleMin}
                  onChange={(e) =>
                    setScatterRules({ ...scatterRules, scaleMin: Number(e.target.value) || 100 })
                  }
                />
              </div>
              <div className="col-6">
                <label className="form-label m-0">{t('Max scale %')}</label>
                <input
                  type="number"
                  className="form-control form-control-sm"
                  value={scatterRules.scaleMax}
                  onChange={(e) =>
                    setScatterRules({ ...scatterRules, scaleMax: Number(e.target.value) || 100 })
                  }
                />
              </div>
            </div>
            <div className="d-flex flex-wrap gap-3 mt-2">
              <div className="form-check form-switch m-0">
                <input
                  className="form-check-input"
                  type="checkbox"
                  checked={scatterRules.randomRotation}
                  onChange={(e) => setScatterRules({ ...scatterRules, randomRotation: e.target.checked })}
                  id="worldgen-random-rotation"
                />
                <label className="form-check-label small" htmlFor="worldgen-random-rotation">
                  {t('Random rotation')}
                </label>
              </div>
              <div className="form-check form-switch m-0">
                <input
                  className="form-check-input"
                  type="checkbox"
                  checked={scatterRules.clusterEnabled}
                  onChange={(e) => setScatterRules({ ...scatterRules, clusterEnabled: e.target.checked })}
                  id="worldgen-cluster"
                />
                <label className="form-check-label small" htmlFor="worldgen-cluster">
                  {t('Cluster')}
                </label>
              </div>
            </div>
            <button type="button" className="btn btn-outline-primary btn-sm w-100 mt-2" onClick={onScatter} disabled={!is3D || !scatterBlueprintId}>
              {t('Distribute in world')}
            </button>
          </>
        )}
      </fieldset>

      {status && <div className="text-muted">{status}</div>}
    </div>
  )
}

export default WorldGenPanelBody
