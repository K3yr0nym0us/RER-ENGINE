import type {
  BluePrintEntry,
  Blueprint3D,
  BlueprintTabCategory,
  Entity3DCategory,
  EntityCategory,
} from '@shared-types'
import type { PendingRestore } from '../context/useContextEngine/types'
import { isEditorBoxPath, isGroundPath, isPlayerPath, isSunPath } from '@shared-types'

const MODEL_3D_EXT = /\.(glb|gltf|fbx)$/i

const PLACEHOLDER_PATHS = new Set([
  '[EditorBox]',
  '[Ground]',
  '[Sun]',
  '[Player]',
  '[Colisionador]',
  '[ExecutionArea]',
])

/** Solo migración de saves/UI antiguos con tabs en español. */
const LEGACY_TAB_CATEGORY: Record<string, BlueprintTabCategory> = {
  personaje: 'character',
  entorno: 'environment',
  objetos: 'object',
}

export function isModel3DPath(path: string | undefined): boolean {
  if (!path) return false
  return MODEL_3D_EXT.test(path)
}

/** Ruta del archivo 3D a cargar para ghost/spawn (visual reemplazado o path principal). */
export function resolveBlueprintModelPath(
  bp: Pick<BluePrintEntry, 'model' | 'path' | 'visualModelPath'>,
): string {
  if (bp.visualModelPath && isModel3DPath(bp.visualModelPath)) {
    return bp.visualModelPath
  }
  const primary = bp.model ?? bp.path ?? ''
  if (primary && isModel3DPath(primary) && !PLACEHOLDER_PATHS.has(primary)) {
    return primary
  }
  return bp.visualModelPath ?? primary
}

export function blueprintUsesModel3D(
  bp: Pick<BluePrintEntry, 'kind' | 'model' | 'path' | 'visualModelPath'>,
): boolean {
  return bp.kind === 'model' || isModel3DPath(resolveBlueprintModelPath(bp))
}

/** Blueprint del editor → manifest `.save` (`Blueprint3D`). */
export function blueprintToSave(bp: BluePrintEntry): Blueprint3D {
  const model = resolveBlueprintModelPath(bp)
  const category = blueprintPlacementCategory(bp)
  const colision = bp.colision ?? bp.physics_enabled ?? category === 'environment'
  return {
    id: bp.id,
    name: bp.name,
    category,
    model,
    colision,
    ...(bp.physics_type ? { physics_type: bp.physics_type } : {}),
    ...(bp.animations?.length ? { animations: bp.animations } : {}),
    ...(bp.scripts?.length ? { scripts: bp.scripts } : {}),
  }
}

/** Manifest → blueprint en memoria del editor (path/kind para UI y quick-build). */
export function blueprintFromSave(bp: Blueprint3D): BluePrintEntry {
  const rawCategory = normalizeBlueprintCategory(bp.category) ?? 'object'
  const category = reconcileCategoryWithName(rawCategory, bp.name)
  const model = bp.model?.trim() ? bp.model : ''
  const kind: BluePrintEntry['kind'] =
    category === 'character' || category === 'player'
      ? 'character'
      : category === 'environment'
        ? 'scenario'
        : 'model'
  const entity_category = blueprintEntityCategoryForEngine(category)
  return {
    ...bp,
    category,
    model,
    path: model,
    kind,
    colision: bp.colision,
    physics_enabled: bp.colision,
    physics_type: bp.physics_type ?? 'static',
    scale: [1, 1, 1],
    rotation: [0, 0, 0, 1],
    ...(entity_category ? { entity_category } : {}),
  }
}

/** Entidad 3D instanciada desde archivo .glb/.gltf/.fbx (no marcadores del editor). */
export function is3dModelFileEntity(
  projectType: string | undefined,
  entity: { path: string },
): boolean {
  if (projectType !== '3D') return false
  if (!isModel3DPath(entity.path)) return false
  return !isPlayerPath(entity.path)
    && !isSunPath(entity.path)
    && !isGroundPath(entity.path)
    && !isEditorBoxPath(entity.path)
}

/** Normaliza categoría manifest/UI (minúsculas, PascalCase, tabs legacy). */
export function normalizeBlueprintCategory(
  raw: string | undefined,
): Entity3DCategory | undefined {
  if (!raw?.trim()) return undefined
  const trimmed = raw.trim()
  if (trimmed in LEGACY_TAB_CATEGORY) {
    const tab = LEGACY_TAB_CATEGORY[trimmed]
    return tab === 'character' ? 'character' : tab === 'environment' ? 'environment' : 'object'
  }
  const lower = trimmed.toLowerCase()
  if ((ENTITY_3D_CATEGORY_VALUES as string[]).includes(lower)) {
    return lower as Entity3DCategory
  }
  return inferEntity3dCategoryFromName(trimmed)
}

/** Categoría del modelo en biblioteca Resources (`models_list`). */
export function categoryFromModelLibrary(
  modelPath: string | undefined,
  models: Array<{ path: string; category?: string }> | undefined,
): Entity3DCategory | undefined {
  if (!modelPath?.trim() || !models?.length) return undefined
  const norm = (p: string) => p.replace(/\\/g, '/').toLowerCase()
  const target = norm(modelPath)
  const leaf = target.split('/').pop() ?? target
  const hit = models.find((m) => {
    const mp = norm(m.path)
    return mp === target || mp.endsWith(`/${leaf}`) || mp === leaf
  })
  return normalizeBlueprintCategory(hit?.category)
}

/** Nombre de plantilla `Environment_01` según categoría (YAML Blueprints). */
export function nextBlueprintTemplateName(
  category: Entity3DCategory,
  existingNames: string[],
): string {
  const prefix =
    category === 'environment'
      ? 'Environment'
      : category === 'character' || category === 'player'
        ? 'Character'
        : category === 'sun'
          ? 'Sun'
          : category === 'ground'
            ? 'Ground'
            : 'Object'
  let max = 0
  const re = new RegExp(`^${prefix}_(\\d+)$`, 'i')
  for (const raw of existingNames) {
    const m = raw.trim().match(re)
    if (m) max = Math.max(max, Number.parseInt(m[1], 10))
  }
  return `${prefix}_${String(max + 1).padStart(2, '0')}`
}

/** Categoría de blueprint al convertir entidad (manifest / `Entity3DCategory`). */
export function blueprintCategoryFromEntity(
  isEnvironment: boolean,
  kind: string | undefined,
  entityCategory?: EntityCategory | string,
  entity3dCategory?: Entity3DCategory | string,
  entityName?: string,
  models?: Array<{ path: string; category?: string }>,
  modelPath?: string,
): Entity3DCategory {
  const fromName = inferEntity3dCategoryFromName(entityName)
  if (fromName === 'environment' || fromName === 'character') {
    return fromName
  }
  const fromManifest = normalizeBlueprintCategory(entity3dCategory)
  if (fromManifest) {
    return reconcileCategoryWithName(fromManifest, entityName)
  }
  const fromEntityCat = normalizeBlueprintCategory(entityCategory)
  if (fromEntityCat) {
    return reconcileCategoryWithName(fromEntityCat, entityName)
  }
  if (kind === 'character') return 'character'
  if (isEnvironment) return 'environment'
  if (kind === 'scenario') return 'environment'
  const fromLibrary = categoryFromModelLibrary(modelPath, models)
  if (fromLibrary) return reconcileCategoryWithName(fromLibrary, entityName)
  if (fromName) return fromName
  return 'object'
}

/** Pestaña de construcción rápida para una categoría 3D. */
export function blueprintTabCategory(
  category: Entity3DCategory | EntityCategory | string | undefined,
): BlueprintTabCategory {
  const norm = normalizeBlueprintCategory(category)
  if (norm === 'character' || norm === 'player') return 'character'
  if (norm === 'environment') return 'environment'
  return 'object'
}

/** Pestaña de construcción rápida para una blueprint (solo inglés en lógica). */
export function resolveBlueprintCategory(
  bp: Pick<BluePrintEntry, 'kind' | 'category' | 'entity_category' | 'name'>,
): BlueprintTabCategory {
  const norm = blueprintPlacementCategory(bp)
  return blueprintTabCategory(norm)
}

const NUMBERED_NAME_CATEGORY: Record<string, Entity3DCategory> = {
  Environment: 'environment',
  Scenario: 'environment',
  Object: 'object',
  Character: 'character',
  Player: 'player',
  Sun: 'sun',
  Ground: 'ground',
}

const ENTITY_3D_CATEGORY_VALUES: Entity3DCategory[] = [
  'environment',
  'character',
  'player',
  'object',
  'sun',
  'ground',
]

/** Prefijo de nombre numerado (`Environment_04`) → categoría manifest. */
export function inferEntity3dCategoryFromName(
  name: string | undefined,
): Entity3DCategory | undefined {
  if (!name?.trim()) return undefined
  const base = name.trim().split('_')[0]
  return NUMBERED_NAME_CATEGORY[base]
}

/**
 * Corrige `object` genérico del motor cuando el nombre ya indica entorno/personaje.
 * (p. ej. `Environment_04` guardado con category `object`).
 */
export function reconcileCategoryWithName(
  category: Entity3DCategory,
  name?: string,
): Entity3DCategory {
  const fromName = inferEntity3dCategoryFromName(name)
  if (!fromName || fromName === 'object') return category
  if (category === 'object') return fromName
  return category
}

/** Categoría 3D al instanciar (category, entity_category, kind, nombre o biblioteca). */
export function blueprintPlacementCategory(
  bp: Pick<BluePrintEntry, 'kind' | 'category' | 'entity_category' | 'name' | 'model' | 'path' | 'visualModelPath'>,
  models?: Array<{ path: string; category?: string }>,
): Entity3DCategory {
  const fromName = inferEntity3dCategoryFromName(bp.name)
  const fromCat = normalizeBlueprintCategory(bp.category)
  const fromEntityCat = normalizeBlueprintCategory(bp.entity_category)

  if (fromName === 'environment' || fromName === 'character') {
    if (!fromCat || fromCat === 'object') return fromName
  }
  if (fromCat) return reconcileCategoryWithName(fromCat, bp.name)
  if (fromEntityCat) return reconcileCategoryWithName(fromEntityCat, bp.name)
  if (bp.kind === 'character') return 'character'
  if (bp.kind === 'scenario') return 'environment'
  const fromLibrary = categoryFromModelLibrary(resolveBlueprintModelPath(bp), models)
  if (fromLibrary && fromLibrary !== 'object') {
    return reconcileCategoryWithName(fromLibrary, bp.name)
  }
  if (fromName) return fromName
  return 'object'
}

/** Colisión/física al instanciar una blueprint (alineado al panel «Con colisión» / physics). */
export function blueprintPlacementPhysics(
  bp: Pick<BluePrintEntry, 'kind' | 'category' | 'entity_category' | 'name' | 'physics_enabled' | 'physics_type' | 'model' | 'path' | 'visualModelPath' | 'colision'>,
  models?: Array<{ path: string; category?: string }>,
): {
  placementCategory: Entity3DCategory
  entityCategory: EntityCategory | undefined
  physicsEnabled: boolean
  physicsType: string
} {
  const placementCategory = blueprintPlacementCategory(bp, models)
  const entityCategory = blueprintEntityCategoryForEngine(placementCategory)
  const isEnvironment = placementCategory === 'environment'
  const colision = bp.colision ?? bp.physics_enabled
  return {
    placementCategory,
    entityCategory,
    physicsEnabled: isEnvironment ? true : (colision ?? false),
    physicsType: isEnvironment ? 'static' : (bp.physics_type ?? 'static'),
  }
}

/** Metadatos de restore al colocar una blueprint (3D quick-build / fallback 2D). */
export function buildQuickBuildPendingRestore(
  bp: BluePrintEntry,
  models?: Array<{ path: string; category?: string }>,
): PendingRestore {
  const { placementCategory, entityCategory, physicsEnabled, physicsType } =
    blueprintPlacementPhysics(bp, models)
  return {
    blueprintId: bp.id,
    entityCategory: entityCategory ?? blueprintEntityCategoryForEngine(placementCategory),
    physicsEnabled,
    physicsType,
    animations: bp.animations as PendingRestore['animations'],
    scripts: bp.scripts,
    controlBindings: bp.control_bindings,
    transform: {
      position: [0, 0, 0],
      rotation: bp.rotation ?? [0, 0, 0, 1],
      scale: bp.scale ?? [1, 1, 1],
    },
  }
}

export function queueQuickBuildPendingRestore(
  restores: Map<string, PendingRestore[]>,
  enginePath: string,
  pending: PendingRestore,
): void {
  const queue = restores.get(enginePath) ?? []
  queue.push(pending)
  restores.set(enginePath, queue)
}

/** `entity_category` IPC (environment | object | character) desde categoría de blueprint. */
export function blueprintEntityCategoryForEngine(
  category: Entity3DCategory | string | undefined,
): EntityCategory | undefined {
  const norm = normalizeBlueprintCategory(category)
  if (norm === 'environment') return 'environment'
  if (norm === 'object') return 'object'
  if (norm === 'character' || norm === 'player') return 'character'
  return undefined
}

/** Categoría enviada al motor para nombrado incremental (`Environment_05`, `Object_03`, …). */
export function placementCategoryForEngine(category: Entity3DCategory): string {
  const norm = normalizeBlueprintCategory(category) ?? 'object'
  if (norm === 'player') return 'character'
  return norm
}

/** Manifest de blueprint para construcción rápida (`docs/Entities_Model_3D.yaml` → Blueprints). */
export function buildBlueprintPlacementMeta(
  bp: BluePrintEntry,
  models?: Array<{ path: string; category?: string }>,
) {
  const model = resolveBlueprintModelPath(bp)
  const category = blueprintPlacementCategory(bp, models)
  const { physicsEnabled, physicsType } = blueprintPlacementPhysics(bp, models)
  const colision = bp.colision ?? bp.physics_enabled ?? category === 'environment'

  const scripts = bp.scripts?.length
    ? bp.scripts.map((s) => ({ name: s.name, source: s.source }))
    : undefined

  const animations = bp.animations?.length
    ? bp.animations.map((anim) => ({
        name: anim.name,
        fps: anim.fps,
        loop_: anim.loop ?? false,
        is_default: anim.is_default,
        facing_right: anim.facing_right,
        logical_w: anim.logical_w,
        logical_h: anim.logical_h,
        audio_path: anim.audio_path,
        frames: anim.frames.map((f) => ({
          path: f.path,
          pivot_x: f.pivot_x,
          pivot_y: f.pivot_y,
          src_x: f.src_x,
          src_y: f.src_y,
          src_w: f.src_w,
          src_h: f.src_h,
        })),
        scripts: (anim.scripts ?? []).map((s) => ({ name: s.name, source: s.source })),
        is_cancelable: anim.is_cancelable,
        embedded_in_model: anim.embedded_in_model,
      }))
    : undefined

  return {
    category: placementCategoryForEngine(category),
    model,
    colision,
    physics_type: physicsType,
    physics_enabled: physicsEnabled,
    rotation: bp.rotation ?? [0, 0, 0, 1],
    scale: bp.scale ?? [1, 1, 1],
    blueprint_id: bp.id,
    template_name: bp.name,
    scripts,
    animations,
  }
}

/** Alinea una ruta de blueprint con la ruta canónica precargada en el motor. */
export function resolveEngineModelPath(
  path: string,
  models: ReadonlyArray<{ path: string; loading?: boolean }>,
): string {
  if (!path) return path
  const exact = models.find((m) => m.path === path && m.loading !== true)
  if (exact) return exact.path
  const base = path.split(/[/\\]/).pop()?.toLowerCase()
  if (!base) return path
  const byName = models.find(
    (m) => m.loading !== true && m.path.split(/[/\\]/).pop()?.toLowerCase() === base,
  )
  return byName?.path ?? path
}
