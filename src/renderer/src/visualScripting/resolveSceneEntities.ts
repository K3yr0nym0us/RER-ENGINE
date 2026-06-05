import type { Entity3D, Entity3DCategory } from '@shared-types'

import type { EntityMeta, Transform } from '../context/useContextEngine/types'
import {
  isEditorOnlySceneEntity,
  resolveEntity3dCategoryForScene,
} from '../utils/entity3dEditorSync'

export interface ResolveSceneEntitiesInput {
  /** Entidades del manifest / snapshot guardado. */
  savedEntities?: Entity3D[]
  /** Jugador FP (en SavedScene va aparte de `entities`). */
  savedPlayer?: Entity3D | null
  /** Entidades vivas en el motor (fuente principal). */
  entityMeta: Record<number, EntityMeta>
  entityTransforms?: Record<number, Transform>
}

function entityFromMeta(id: number, meta: EntityMeta, transform?: Transform): Entity3D {
  return {
    id,
    name: meta.name ?? `Entity ${id}`,
    category: meta.entity3dCategory ?? 'object',
    model: meta.path,
    position: transform?.position ?? [0, 0, 0],
    rotation: transform?.rotation ?? [0, 0, 0, 1],
    scale: transform?.scale ?? [1, 1, 1],
    colision: meta.physicsEnabled,
    animations: meta.animations,
  }
}

function mergeEntity(base: Entity3D | undefined, patch: Entity3D): Entity3D {
  if (!base) return patch
  const animations = patch.animations?.length ? patch.animations : base.animations
  return {
    ...base,
    ...patch,
    ...(animations?.length ? { animations } : {}),
  }
}

/** Entidades de escena enriquecidas para el editor de nodos (categoría + animaciones). */
export function resolveSceneEntitiesForVisualScript({
  savedEntities = [],
  savedPlayer = null,
  entityMeta,
  entityTransforms = {},
}: ResolveSceneEntitiesInput): Entity3D[] {
  const byId = new Map<number, Entity3D>()

  for (const [idStr, meta] of Object.entries(entityMeta)) {
    const id = Number(idStr)
    if (!Number.isFinite(id)) continue
    byId.set(id, entityFromMeta(id, meta, entityTransforms[id]))
  }

  for (const entity of savedEntities) {
    byId.set(entity.id, mergeEntity(byId.get(entity.id), entity))
  }

  if (savedPlayer) {
    byId.set(savedPlayer.id, mergeEntity(byId.get(savedPlayer.id), savedPlayer))
  }

  return [...byId.values()]
    .filter((entity) => !isEditorOnlySceneEntity(entity, entityMeta[entity.id]))
    .map((entity) => {
      const meta = entityMeta[entity.id]
      const animations = entity.animations?.length
        ? entity.animations
        : meta?.animations
      return {
        ...entity,
        name: entity.name || meta?.name || `Entity ${entity.id}`,
        category: resolveEntity3dCategoryForScene(entity, meta),
        ...(animations?.length ? { animations } : {}),
      }
    })
}

export function animationNamesForEntity(
  entity?: Pick<Entity3D, 'animations'>,
): string[] {
  return (entity?.animations ?? [])
    .map((anim) => anim.name?.trim())
    .filter((name): name is string => Boolean(name))
}

/** Subconjunto serializable para IPC modal Electron (sin campos `undefined`). */
export interface VisualScriptSceneEntity {
  id: number
  name: string
  category: Entity3DCategory
  model: string
  colision: boolean
  animations?: Array<{ name: string }>
}

export function sanitizeSceneEntitiesForModal(entities: Entity3D[]): VisualScriptSceneEntity[] {
  return entities.map((entity) => {
    const names = animationNamesForEntity(entity)
    const row: VisualScriptSceneEntity = {
      id: entity.id,
      name: entity.name || `Entity ${entity.id}`,
      category: entity.category ?? 'object',
      model: entity.model ?? '',
      colision: Boolean(entity.colision),
    }
    if (names.length > 0) {
      row.animations = names.map((name) => ({ name }))
    }
    return row
  })
}
