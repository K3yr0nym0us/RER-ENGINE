import type { Blueprint3D, Entity3D, Entity3DCategory, VisualGraphContext } from '@shared-types'

export interface ContextVariable {
  id: string
  /** Texto visible si no hay `labelKey`. */
  label: string
  /** Clave i18n para nombre legible en el panel. */
  labelKey?: string
  rhaiSnippet: string
  /** Clave i18n (inglés) para el tooltip. */
  description?: string
  /** Detalle extra sin traducir (p. ej. nombre de entidad). */
  detail?: string
  /** Solo referencias a entidades de escena. */
  entityCategory?: Entity3DCategory
  kind: 'global' | 'entity' | 'animation'
  /** Agrupación en el panel «Entity variables». */
  entityGroup?: EntityVariableGroup
}

export type EntityVariableGroup = 'transform' | 'animations' | 'other'

export const ENTITY_VARIABLE_GROUP_ORDER: EntityVariableGroup[] = [
  'transform',
  'animations',
  'other',
]

export const ENTITY_VARIABLE_GROUP_LABEL_KEYS: Record<EntityVariableGroup, string> = {
  transform: 'Transform',
  animations: 'Animations',
  other: 'Others',
}

/** Mismo orden que en el acordeón Entidades / manifest. */
export const ENTITY_3D_CATEGORY_ORDER: Entity3DCategory[] = [
  'environment',
  'character',
  'player',
  'object',
  'sun',
  'ground',
]

export const ENTITY_CATEGORY_LABEL_KEYS: Record<Entity3DCategory, string> = {
  environment: 'Environment',
  character: 'Characters',
  player: 'Player',
  object: 'Objects',
  sun: 'Sun',
  ground: 'Ground',
}

/** Solo lectura — las acciones `engine.*` van como nodos en el canvas. */
const ENTITY_READ_VARIABLES: ContextVariable[] = [
  { id: 'entity.id', label: 'entity.id', rhaiSnippet: 'entity.id', kind: 'global', description: 'Var entity id', entityGroup: 'other' },
  { id: 'entity.x', label: 'entity.x', labelKey: 'Position X', rhaiSnippet: 'entity.x', kind: 'global', description: 'Var entity x', entityGroup: 'transform' },
  { id: 'entity.y', label: 'entity.y', labelKey: 'Position Y', rhaiSnippet: 'entity.y', kind: 'global', description: 'Var entity y', entityGroup: 'transform' },
  { id: 'entity.scale_x', label: 'entity.scale_x', labelKey: 'Scale X', rhaiSnippet: 'entity.scale_x', kind: 'global', description: 'Var entity scale x', entityGroup: 'transform' },
  { id: 'entity.scale_y', label: 'entity.scale_y', labelKey: 'Scale Y', rhaiSnippet: 'entity.scale_y', kind: 'global', description: 'Var entity scale y', entityGroup: 'transform' },
]

const SCENE_GLOBAL_VARIABLES: ContextVariable[] = [
  {
    id: 'dt',
    label: 'dt',
    labelKey: 'Delta time (seconds)',
    rhaiSnippet: 'dt',
    kind: 'global',
    description: 'Var scene dt',
    detail: 'dt',
  },
]

export function getContextVariables(
  context: VisualGraphContext,
  options?: {
    sceneEntities?: Entity3D[]
    entityId?: number
    entityName?: string
  },
): ContextVariable[] {
  if (context === 'entity') {
    const base = [...ENTITY_READ_VARIABLES]
    if (options?.entityId != null) {
      base.unshift({
        id: 'this_entity',
        label: `This entity (id ${options.entityId})`,
        labelKey: 'This entity',
        rhaiSnippet: `entity.id == ${options.entityId}`,
        kind: 'global',
        description: 'Var this entity',
        detail: options.entityName,
        entityGroup: 'other',
      })
    }
    return base
  }

  const globals = [...SCENE_GLOBAL_VARIABLES]
  const entities = options?.sceneEntities ?? []

  if (entities.length > 0) {
    globals.unshift({
      id: 'scene_entity_ids',
      label: 'Scene entity ids',
      labelKey: 'All entity ids (list)',
      rhaiSnippet: `[${entities.map((e) => e.id).join(', ')}]`,
      kind: 'global',
      description: 'Var scene entity ids',
    })
  }

  const entityVars: ContextVariable[] = entities.map((ent) => ({
    id: `scene_entity_${ent.id}`,
    label: ent.name || `Entity ${ent.id}`,
    rhaiSnippet: String(ent.id),
    kind: 'entity',
    entityCategory: ent.category,
    description: 'Var scene entity ref',
    detail: `id ${ent.id}`,
  }))

  return [...globals, ...entityVars]
}

export interface SceneVariableAccordionGroup {
  eventKey: string
  labelKey: string
  items: ContextVariable[]
}

/** Agrupa referencias de escena: globals + entidades por `Entity3DCategory`. */
export function groupSceneVariablesForAccordion(
  variables: ContextVariable[],
): SceneVariableAccordionGroup[] {
  const groups: SceneVariableAccordionGroup[] = []

  const globals = variables.filter((v) => v.kind === 'global')
  if (globals.length > 0) {
    groups.push({
      eventKey: 'scene-globals',
      labelKey: 'Scene logic',
      items: globals,
    })
  }

  const byCategory = new Map<Entity3DCategory, ContextVariable[]>()
  for (const variable of variables) {
    if (variable.kind !== 'entity') continue
    const category = variable.entityCategory ?? 'object'
    const list = byCategory.get(category) ?? []
    list.push(variable)
    byCategory.set(category, list)
  }

  for (const category of ENTITY_3D_CATEGORY_ORDER) {
    const items = byCategory.get(category)
    if (!items?.length) continue
    groups.push({
      eventKey: `entity-${category}`,
      labelKey: ENTITY_CATEGORY_LABEL_KEYS[category],
      items,
    })
  }

  return groups
}

/** Agrupa variables de entidad: Transform, Movement, Animations, Others. */
export function groupEntityVariablesForAccordion(
  variables: ContextVariable[],
  animationItems: ContextVariable[] = [],
): SceneVariableAccordionGroup[] {
  const groups: SceneVariableAccordionGroup[] = []

  for (const group of ENTITY_VARIABLE_GROUP_ORDER) {
    const items = group === 'animations'
      ? animationItems
      : variables.filter((v) => v.entityGroup === group)

    if (items.length === 0) continue
    groups.push({
      eventKey: `entity-var-${group}`,
      labelKey: ENTITY_VARIABLE_GROUP_LABEL_KEYS[group],
      items,
    })
  }

  return groups
}

export interface AnimationAccordionGroup {
  entityId: number
  entityName: string
  entityCategory?: Entity3DCategory
  items: ContextVariable[]
}

/** Animaciones agrupadas por entidad para el panel lateral del editor de nodos. */
export function buildAnimationAccordionGroups(
  sceneEntities?: Entity3D[],
  options?: { entityId?: number },
): AnimationAccordionGroup[] {
  const entities = options?.entityId != null
    ? (sceneEntities ?? []).filter((entity) => entity.id === options.entityId)
    : (sceneEntities ?? [])

  const groupsById = new Map<number, AnimationAccordionGroup>()

  for (const entity of entities) {
    const animationNames = (entity.animations ?? [])
      .map((anim) => anim.name?.trim())
      .filter((name): name is string => Boolean(name))
    if (animationNames.length === 0) continue

    const existing = groupsById.get(entity.id)
    const mergedNames = existing
      ? [...new Set([...existing.items.map((item) => item.label), ...animationNames])]
      : animationNames

    groupsById.set(entity.id, {
      entityId: entity.id,
      entityName: entity.name || `Entity ${entity.id}`,
      entityCategory: entity.category,
      items: mergedNames.map((name) => ({
        id: `anim_${entity.id}_${name}`,
        label: name,
        rhaiSnippet: name,
        kind: 'animation' as const,
        entityCategory: entity.category,
        description: 'Var animation ref',
        detail: `id ${entity.id}`,
      })),
    })
  }

  return [...groupsById.values()]
}

export interface SceneEntityRow {
  id: number
  name: string
}

export interface SceneBlueprintGroup {
  blueprintId: string
  baseEntityId: number
  baseName: string
  instances: SceneEntityRow[]
}

export interface ScenePanelStructure {
  globals: ContextVariable[]
  player: {
    entityId: number
    entityName: string
    animations: ContextVariable[]
  } | null
  characters: ContextVariable[]
  objects: ContextVariable[]
  environment: {
    baseEnvironment: SceneEntityRow[]
    standalone: SceneEntityRow[]
    blueprintGroups: SceneBlueprintGroup[]
  }
}

function entityToContextVariable(ent: Entity3D): ContextVariable {
  return {
    id: `scene_entity_${ent.id}`,
    label: ent.name || `Entity ${ent.id}`,
    rhaiSnippet: String(ent.id),
    kind: 'entity',
    entityCategory: ent.category,
    description: 'Var scene entity ref',
    detail: `id ${ent.id}`,
  }
}

function resolveBlueprintBase(
  blueprintId: string,
  instances: Entity3D[],
  blueprints?: Blueprint3D[],
): { baseEntityId: number; baseName: string } {
  const blueprint = blueprints?.find((bp) => bp.id === blueprintId)
  const byName = blueprint
    ? instances.find((entity) => entity.name === blueprint.name)
    : undefined
  const base = byName ?? instances.reduce((prev, next) => (prev.id <= next.id ? prev : next))
  return {
    baseEntityId: base.id,
    baseName: blueprint?.name ?? base.name,
  }
}

/** Estructura del panel lateral en contexto escena (Player, Environment agrupado, etc.). */
export function buildScenePanelStructure(
  variables: ContextVariable[],
  sceneEntities: Entity3D[] = [],
  blueprints?: Blueprint3D[],
): ScenePanelStructure {
  const globals = variables.filter((variable) => variable.kind === 'global')

  const playerEntity = sceneEntities.find((entity) => entity.category === 'player') ?? null
  const playerAnimations = playerEntity
    ? buildAnimationAccordionGroups([playerEntity]).flatMap((group) => group.items)
    : []

  const characters = sceneEntities
    .filter((entity) => entity.category === 'character')
    .map(entityToContextVariable)

  const objects = sceneEntities
    .filter((entity) => entity.category === 'object')
    .map(entityToContextVariable)

  const baseEnvironment: SceneEntityRow[] = []
  const standaloneEnvironment: SceneEntityRow[] = []
  const blueprintInstances = new Map<string, Entity3D[]>()

  for (const entity of sceneEntities) {
    if (entity.category === 'sun' || entity.category === 'ground') {
      baseEnvironment.push({ id: entity.id, name: entity.name || `Entity ${entity.id}` })
      continue
    }
    if (entity.category !== 'environment') continue

    const blueprintId = entity.blueprint_id?.trim()
    if (blueprintId) {
      const list = blueprintInstances.get(blueprintId) ?? []
      list.push(entity)
      blueprintInstances.set(blueprintId, list)
      continue
    }

    standaloneEnvironment.push({ id: entity.id, name: entity.name || `Entity ${entity.id}` })
  }

  const blueprintGroups: SceneBlueprintGroup[] = [...blueprintInstances.entries()]
    .map(([blueprintId, instances]) => {
      const { baseEntityId, baseName } = resolveBlueprintBase(blueprintId, instances, blueprints)
      return {
        blueprintId,
        baseEntityId,
        baseName,
        instances: instances
          .map((entity) => ({ id: entity.id, name: entity.name || `Entity ${entity.id}` }))
          .sort((a, b) => a.name.localeCompare(b.name) || a.id - b.id),
      }
    })
    .sort((a, b) => a.baseName.localeCompare(b.baseName))

  return {
    globals,
    player: playerEntity
      ? {
          entityId: playerEntity.id,
          entityName: playerEntity.name || `Entity ${playerEntity.id}`,
          animations: playerAnimations,
        }
      : null,
    characters,
    objects,
    environment: {
      baseEnvironment,
      standalone: standaloneEnvironment,
      blueprintGroups,
    },
  }
}
