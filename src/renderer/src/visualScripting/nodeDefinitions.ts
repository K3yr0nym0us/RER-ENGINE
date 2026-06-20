/** Tipos de nodo MVP (catálogo editor). */
export const VISUAL_NODE_TYPES = {
  SCENE_BEGIN: 'event.scene_begin',
  TICK: 'event.tick',
  ENTITY_START: 'event.entity_start',
  ENTITY_UPDATE: 'event.entity_update',
  SEQUENCE: 'flow.sequence',
  IF: 'flow.if',
  FOR_REPEAT: 'flow.for_repeat',
  FOR_EACH_ENTITY: 'flow.for_each_entity',
  LOG: 'action.log',
  PLAY_ANIMATION: 'action.play_animation',
  SET_SCALE: 'action.set_scale',
  MOVE_TO: 'action.move_to',
  TRANSLATE: 'action.translate',
  SET_REFLECTION_TIER: 'action.set_reflection_tier',
} as const

export type VisualNodeType = (typeof VISUAL_NODE_TYPES)[keyof typeof VISUAL_NODE_TYPES]

export const EXEC_OUT = 'exec'
export const EXEC_IN = 'exec_in'
export const THEN_0 = 'then_0'
export const THEN_1 = 'then_1'
export const THEN_TRUE = 'then_true'
export const THEN_FALSE = 'then_false'
export const LOOP_BODY = 'loop_body'

export type VisualGraphContext = 'scene' | 'entity'

export interface NodeDefinition {
  type: VisualNodeType
  label: string
  category: 'event' | 'flow' | 'action'
  /** Contextos donde el nodo está disponible. */
  contexts: VisualGraphContext[]
  defaultData: Record<string, unknown>
}

export const NODE_DEFINITIONS: Record<VisualNodeType, NodeDefinition> = {
  [VISUAL_NODE_TYPES.SCENE_BEGIN]: {
    type: VISUAL_NODE_TYPES.SCENE_BEGIN,
    label: 'Scene start',
    category: 'event',
    contexts: ['scene'],
    defaultData: {},
  },
  [VISUAL_NODE_TYPES.TICK]: {
    type: VISUAL_NODE_TYPES.TICK,
    label: 'Each frame',
    category: 'event',
    contexts: ['scene'],
    defaultData: {},
  },
  [VISUAL_NODE_TYPES.ENTITY_START]: {
    type: VISUAL_NODE_TYPES.ENTITY_START,
    label: 'On start',
    category: 'event',
    contexts: ['entity'],
    defaultData: {},
  },
  [VISUAL_NODE_TYPES.ENTITY_UPDATE]: {
    type: VISUAL_NODE_TYPES.ENTITY_UPDATE,
    label: 'Each frame',
    category: 'event',
    contexts: ['entity'],
    defaultData: {},
  },
  [VISUAL_NODE_TYPES.SEQUENCE]: {
    type: VISUAL_NODE_TYPES.SEQUENCE,
    label: 'Sequence',
    category: 'flow',
    contexts: ['scene', 'entity'],
    defaultData: {},
  },
  [VISUAL_NODE_TYPES.IF]: {
    type: VISUAL_NODE_TYPES.IF,
    label: 'Branch (if)',
    category: 'flow',
    contexts: ['scene', 'entity'],
    defaultData: { expression: 'true' },
  },
  [VISUAL_NODE_TYPES.FOR_REPEAT]: {
    type: VISUAL_NODE_TYPES.FOR_REPEAT,
    label: 'Repeat N times',
    category: 'flow',
    contexts: ['scene', 'entity'],
    defaultData: { count: 3 },
  },
  [VISUAL_NODE_TYPES.FOR_EACH_ENTITY]: {
    type: VISUAL_NODE_TYPES.FOR_EACH_ENTITY,
    label: 'For each entity',
    category: 'flow',
    contexts: ['scene'],
    defaultData: { entityIds: [] as number[] },
  },
  [VISUAL_NODE_TYPES.LOG]: {
    type: VISUAL_NODE_TYPES.LOG,
    label: 'Print',
    category: 'action',
    contexts: ['scene', 'entity'],
    defaultData: { message: 'Hello from logic' },
  },
  [VISUAL_NODE_TYPES.PLAY_ANIMATION]: {
    type: VISUAL_NODE_TYPES.PLAY_ANIMATION,
    label: 'Play animation',
    category: 'action',
    contexts: ['scene', 'entity'],
    defaultData: { entityId: 0, animationName: '' },
  },
  [VISUAL_NODE_TYPES.SET_SCALE]: {
    type: VISUAL_NODE_TYPES.SET_SCALE,
    label: 'Set scale',
    category: 'action',
    contexts: ['entity'],
    defaultData: { scaleX: '1.0', scaleY: '1.0' },
  },
  [VISUAL_NODE_TYPES.MOVE_TO]: {
    type: VISUAL_NODE_TYPES.MOVE_TO,
    label: 'Teleport to',
    category: 'action',
    contexts: ['entity'],
    defaultData: { x: 'entity.x', y: 'entity.y' },
  },
  [VISUAL_NODE_TYPES.TRANSLATE]: {
    type: VISUAL_NODE_TYPES.TRANSLATE,
    label: 'Translate',
    category: 'action',
    contexts: ['entity'],
    defaultData: { dx: '0.0', dy: '0.0' },
  },
  [VISUAL_NODE_TYPES.SET_REFLECTION_TIER]: {
    type: VISUAL_NODE_TYPES.SET_REFLECTION_TIER,
    label: 'Set reflection tier',
    category: 'action',
    contexts: ['scene'],
    defaultData: { tier: 'off' },
  },
}

export function nodesForContext(context: VisualGraphContext): VisualNodeType[] {
  return (Object.values(NODE_DEFINITIONS) as NodeDefinition[])
    .filter((def) => def.contexts.includes(context))
    .map((def) => def.type)
}
