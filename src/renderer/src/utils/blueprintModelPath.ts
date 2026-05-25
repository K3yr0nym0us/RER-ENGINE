import type { BluePrintEntry, BluePrintCategory, EntityCategory } from '@shared-types'
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

export function isModel3DPath(path: string | undefined): boolean {
  if (!path) return false
  return MODEL_3D_EXT.test(path)
}

/** Ruta del archivo 3D a cargar para ghost/spawn (visual reemplazado o path principal). */
export function resolveBlueprintModelPath(bp: Pick<BluePrintEntry, 'path' | 'visualModelPath'>): string {
  if (bp.visualModelPath && isModel3DPath(bp.visualModelPath)) {
    return bp.visualModelPath
  }
  if (bp.path && isModel3DPath(bp.path) && !PLACEHOLDER_PATHS.has(bp.path)) {
    return bp.path
  }
  return bp.visualModelPath ?? bp.path
}

export function blueprintUsesModel3D(bp: Pick<BluePrintEntry, 'kind' | 'path' | 'visualModelPath'>): boolean {
  return bp.kind === 'model' || isModel3DPath(resolveBlueprintModelPath(bp))
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

export function resolveBlueprintCategory(
  bp: Pick<BluePrintEntry, 'kind' | 'category' | 'entity_category'>,
  entityCategory?: EntityCategory,
): BluePrintCategory {
  if (bp.category) return bp.category
  const cat = bp.entity_category ?? entityCategory
  if (bp.kind === 'character') return 'personaje'
  if (bp.kind === 'scenario') return 'entorno'
  if (cat === 'environment') return 'entorno'
  return 'objetos'
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
