// Tipos compartidos entre main process, preload y renderer.

import type { EngineCommandName2D, EngineCommandName3D } from './engineCommands'

export * from './plugins'

export type ProjectType = '2D' | '3D'

export type GameStyle =
  | 'first-person'   // tipo de cámara 3D (vista desde los ojos)
  | 'second-person'
  | 'third-person'
  | 'top-down'       // perspectiva 2D
  | 'side-scroller'  // perspectiva 2D
  | 'isometric'

/** Modo de cámara 3D por defecto al crear un proyecto nuevo. */
export const DEFAULT_3D_CAMERA_MODE: GameStyle = 'first-person'

/** Sincroniza tipo de proyecto, modo de cámara y rutas con Electron antes de arrancar el motor. */
export interface EngineStartPayload {
  projectType: ProjectType
  /** Modo de cámara 3D; `false` en proyectos 2D. */
  mode:        GameStyle | false
  /** Ruta absoluta del `.save` en disco (autosave); `false` si es proyecto nuevo. */
  save_path:   string | false
  /** 2D: carpeta ya extraída con `manifest.json` + assets; el motor lee desde aquí. */
  extract_dir?: string | false
}

/** Metadatos mínimos del manifest al abrir `.save` 2D (sin cargar escena en el renderer). */
export interface OpenProjectManifestMeta {
  type:      ProjectType
  gameStyle: GameStyle
}

/** Rutas simbólicas de entidad (no son archivos en disco). */
export const ENTITY_MARKER_PATHS = [
  '[EditorBox]',
  '[Ground]',
  '[Player]',
  '[EditorCamera]',
  '[Sun]',
  '[Ball]',
  '[Colisionador]',
  '[ExecutionArea]',
] as const

export type EntityMarkerPath = (typeof ENTITY_MARKER_PATHS)[number]

export function entityPathMarker(p: string | null | undefined): EntityMarkerPath | null {
  if (!p) return null
  const marker = p.split(/[/\\]/).pop() ?? p
  return (ENTITY_MARKER_PATHS as readonly string[]).includes(marker)
    ? (marker as EntityMarkerPath)
    : null
}

/** Categoría IPC del motor para colisión/nombrado. */
export type EntityCategory = 'environment' | 'object' | 'character' | 'weapon' | 'projectile'

// ── Modelo 3D (docs/yaml/gameplay/Entities_Model_3D.yaml) ─────────────────────────────────

export type Entity3DCategory =
  | 'environment'
  | 'character'
  | 'player'
  | 'object'
  | 'weapon'
  | 'projectile'
  | 'sun'
  | 'ground'

export type PhysicsType3D = 'dynamic' | 'static' | 'kinematic'

/** Modo de seguimiento del ojo de cámara play character respecto al jugador en editor. */
export type PlayCameraFollowMode = 'follow_character' | 'move_with_character'

/** WASD / gamepad → Rhai (solo `category: player` en instancias). */
export interface SavedControls {
  keyboard_mouse: Record<string, SavedScript>
  gamepad: Record<string, SavedScript>
}

/** @deprecated Use `SavedControls` */
export type SavedControlBindings = SavedControls

/** Entidad 3D en manifest / escena. */
export interface Entity3D {
  id: number
  name: string
  category: Entity3DCategory
  model: string
  /** ID estable del asset importado (`.rerasset` en `resources.models`). */
  model_id?: string
  position: [number, number, number]
  rotation: [number, number, number, number]
  scale: [number, number, number]
  physics_type?: PhysicsType3D
  colision: boolean
  /** Dureza de gameplay (0 = proyectiles la ignoran). */
  hardness?: number
  animations?: SavedAnimation[]
  scripts?: SavedScript[]
  /** Lógica visual compilada (Entity Blueprint). */
  visualGraph?: VisualGraphDocument
  visualScriptRhai?: string
  blueprint_id?: string
  controls?: SavedControls
  /** Fusión en editor: id del padre (solo en entidades hijas). */
  attach_parent_id?: number
  attach_local_position?: [number, number, number]
  attach_local_rotation?: [number, number, number, number]
  /** Escala mundial del hijo al fusionar (no hereda escala del padre). */
  attach_local_scale?: [number, number, number]
  /** Hijo enganchado a socket de otra entidad. */
  attach_socket_host_id?: number
  attach_socket_name?: string
  /** Sockets definidos en esta entidad host. */
  sockets?: EntitySocket3D[]
  /** Física secundaria por hueso (jiggle). */
  bone_physics?: EntityBonePhysics3D[]
  /** Config de disparo (categoría `projectile`). */
  projectile?: ProjectileConfig3D
}

export interface ProjectileConfig3D {
  speed: number
  lifetime_s: number
  /** Si false, trayectoria recta (ignora gravity_scale). */
  affected_by_gravity?: boolean
  /** Multiplicador de gravedad del mundo (solo si affected_by_gravity). */
  gravity_scale?: number
  /** Orientar mesh al vector de vuelo. */
  align_to_velocity?: boolean
  /** Socket de origen (default `muzzle`) al disparar desde from_id. */
  muzzle_socket?: string
  /** Legacy (ignored for surface decision); rebote lo decide hardness. */
  bounceable?: boolean
  /** Rebotes máxim al disparar. */
  max_bounces?: number
  /** Fracción de velocidad perdida por rebote o pierce (0..1). */
  bounce_speed_loss?: number
  /** Tier de dureza del proyectil (0 = atraviesa todo sin pérdida). */
  hardness?: number
}

export interface EntitySocket3D {
  name: string
  bone_name: string
  local_position: [number, number, number]
  local_rotation: [number, number, number, number]
}

export interface EntityBonePhysics3D {
  bone_name: string
  mode: 'none' | 'inherit' | 'static' | 'dynamic' | 'kinematic'
}

/** Plantilla (`project.blueprints[]` / manifest). */
export interface Blueprint3D {
  id: string
  name: string
  category: Entity3DCategory
  model: string
  /** Asset importado (`model_*`); clave canónica GPU / `.rerasset`. */
  model_id?: string
  physics_type?: PhysicsType3D
  colision: boolean
  animations?: SavedAnimation[]
  scripts?: SavedScript[]
  /** Solo editor en memoria; no se serializa en manifest. */
  kind?: SavedEntity['kind']
  path?: string
  scale?: [number, number, number]
  rotation?: [number, number, number, number]
  physics_enabled?: boolean
  entity_category?: EntityCategory
  visualModelPath?: string
  control_bindings?: SavedControlBindings
}

/** Cámara FPS (no va en `player`). */
export interface ConfigCamera {
  camera_eye_position?: [number, number, number]
  fps_camera_yaw?: number
  fps_camera_pitch?: number
  yaw?: number
  pitch?: number
  fov_y?: number
  frustum_distance?: number
  camera_follow_mode?: PlayCameraFollowMode
}

/** Viewport orbital del editor. */
export interface ConfigEditorCamera {
  position: [number, number, number]
  rotation?: [number, number, number, number]
}

export type ModelCategory = Entity3DCategory

// ── 2D legacy (engine2d) ───────────────────────────────────────────────────

export interface SavedEntity {
  id:               number
  /** Nombre visible de la entidad en el editor. */
  name?:            string
  path:             string
  kind:             'model' | 'scenario' | 'character' | 'projectile' | 'collider' | 'execution_area' | 'directional_light'
  position:         [number, number, number]
  rotation:         [number, number, number, number]
  scale:            [number, number, number]
  physics_enabled?: boolean
  /**
   * Tipo de físicas del cuerpo.
   * Valores válidos: "dynamic" (afectado por fuerzas y gravedad),
   * "static" (no se mueve), "kinematic" (movido solo por código, sin fuerzas).
   */
  physics_type?:    string
/** Puntos en espacio de mundo para entidades de tipo 'collider' y 'execution_area'. */
  points?:          [[number,number],[number,number],[number,number],[number,number]]
  /** Animaciones asociadas a esta entidad. */
  animations?:      SavedAnimation[]
  /** Scripts Rhai adjuntos a esta entidad. */
  scripts?:         SavedScript[]
  /** Lógica visual compilada (Entity Blueprint). */
  visualGraph?:     VisualGraphDocument
  visualScriptRhai?: string
  /** Mapeo de controles por entidad (personajes). */
  control_bindings?: SavedControlBindings
  /** Nombre del sprite precargado si esta entidad lo usa. */
  spriteName?:      string
  /** ID de la blueprint desde la que fue instanciada esta entidad. */
  blueprint_id?:    string
  /** Ruta del modelo visual (.glb/.fbx) si difiere de `path` (p. ej. jugador con `[Player]`). */
  visual_model_path?: string
  /** Config de disparo (kind `projectile`). */
  projectile?: ProjectileConfig3D
  /** Dureza de gameplay (0 = proyectiles la ignoran). */
  hardness?: number
  /** Categoría de entorno para UI de colisión en 3D. */
  entity_category?: EntityCategory
}

export interface SavedAnimation {
  name:       string
  fps:        number
  loop:       boolean
  /** Marca esta animación como predeterminada para volver al finalizar otras. */
  is_default?: boolean
  /** Indica la orientación por defecto de la animación (true = mira a la derecha). */
  facing_right?: boolean
  /** Bounding box lógico fijo (en píxeles) que define el tamaño referencia de la entidad. */
  logical_w:  number
  logical_h:  number
  /** Ruta del archivo de audio asociado a la animación (wav/ogg/mp3). */
  audio_path?: string
  frames: {
    path:    string
    /** Punto ancla en píxeles dentro del frame (esquina superior-izq = 0,0). */
    pivot_x?: number
    pivot_y?: number
    /** Coordenadas opcionales dentro del sprite sheet (si el frame viene de un atlas). */
    src_x?:  number
    src_y?:  number
    src_w?:  number
    src_h?:  number
  }[]
  /** Scripts Rhai asociados a esta animación. */
  scripts?: SavedScript[]
  /**
   * Si true, una animación siguiente puede interrumpir/cancelar esta antes de que termine.
   * Por defecto false: ninguna animación puede cancelarla.
   */
  is_cancelable?: boolean
  /** Modo de selección usado en la modal de sprites (cell/grid o box libre). */
  selection_mode?: 'cell' | 'box'
  /** Tamaño de celda usado cuando selection_mode = 'cell'. */
  grid_size?: number
  /** Offset horizontal de la grilla en la modal de sprites. */
  cell_offset_x?: number
  /** Offset vertical de la grilla en la modal de sprites. */
  cell_offset_y?: number
  /** Clip embebido en modelo 3D (sin frames PNG en el front). */
  embedded_in_model?: boolean
}

export interface SavedScript {
  /** Nombre identificador del script (elegido por el usuario). */
  name:   string
  /** Código fuente Rhai completo. */
  source: string
}

export interface SavedWorldConfig {
  worldWidth:   number
  worldHeight:  number
  worldDepth?:  number
  /** 3D: radio equivalente (mayor semieje) del mundo rectangular. */
  worldRadius?: number
  /** Centro XZ de la huella; permite crecer una cara sin recentrar. */
  worldCenterX?: number
  worldCenterZ?: number
  gridVisible:  boolean
  gridCellSize: number
  gravity?:     number
  targetFps:    number
  /** 3D: ambiente 0–1 (caras no iluminadas). */
  lightAmbient?:     number
  /** 3D: intensidad de la luz direccional. */
  lightIntensity?:   number
  /** 3D: oscuridad de sombras proyectadas (0.9–1, mayor = más oscuro). */
  shadowDarkness?:   number
  /** 3D: nivel global de texturas GLB embebidas. */
  graphicsTextureTier?: 'low' | 'medium' | 'high' | 'ultra'
  /** 3D: distancia (m) con textura al tope del tier activo. */
  textureDetailDistance?: number
  /** 3D: nivel global de reflejos (SSR / temporal / RT). */
  reflectionTier?: 'off' | 'low' | 'medium' | 'high' | 'ultra'
  /** 3D: ray tracing HW (High/Ultra + GPU compatible). */
  reflectionRaytracing?: boolean
  /** 3D: reflection probes (cubemap IBL + captura). */
  reflectionProbes?: boolean
  /** 3D: nivel de calidad de sombras (resolución del shadow map por tier). */
  shadowTier?: 'off' | 'low' | 'medium' | 'high' | 'ultra'
  /** 3D: MSAA del pase forward (Low=Off, Medium 2x, High 4x, Ultra 8x). */
  msaaTier?: 'low' | 'medium' | 'high' | 'ultra'
  /** 3D: TAA de escena. */
  taaEnabled?: boolean
  taaBlend?: number
  taaJitterScale?: number
  /** 3D: LOD de mallas GLB por distancia (3 niveles, vertex clustering en carga). */
  lodEnabled?:   boolean
  /** 3D: distancia (m) desde la que se usa LOD1. */
  lod1Distance?: number
  /** 3D: distancia (m) desde la que se usa LOD2. */
  lod2Distance?: number
}

/** Contexto del grafo: escena (Level Blueprint) o entidad (Entity Blueprint). */
export type VisualGraphContext = 'scene' | 'entity'

/** Grafo canónico de programación visual (fuente de verdad; el motor no lee React Flow). */
export interface VisualGraphDocument {
  version: 1
  /** Por defecto `scene` si falta (compatibilidad con saves antiguos). */
  context?: VisualGraphContext
  sceneId?: number
  entityId?: number
  nodes: VisualGraphNode[]
  edges: VisualGraphEdge[]
}

export interface VisualGraphNode {
  id: string
  type: string
  position: { x: number; y: number }
  data: Record<string, unknown>
}

export interface VisualGraphEdge {
  id: string
  source: string
  sourceHandle: string
  target: string
  targetHandle: string
}

export interface SavedScene {
  id:             number
  name:           string
  world:          SavedWorldConfig
  backgroundPath: string | null
  entities:       Entity3D[]
  player:         Entity3D | null
  config_camera:  ConfigCamera | null
  config_editor_camera: ConfigEditorCamera | null
  blueprints?:    Blueprint3D[]
  camera2d:       { x: number; y: number; halfH: number } | null
  sprites:        Array<{ name: string; path: string }>
  models?:        Array<{ name: string; path: string; category?: ModelCategory }>
  /** Lógica de escena (Level Blueprint) — layout de nodos. */
  visualGraph?:     VisualGraphDocument
  /** Caché Rhai compilada en el editor; opcional si falta `visualGraph`. */
  visualScriptRhai?: string
  /** Script Rhai manual de escena (Level Blueprint). */
  sceneScriptRhai?: string
}

export interface ProjectSaveData {
  version:         number
  type:            ProjectType
  gameStyle:       GameStyle
  /** Escenas del proyecto (multi-escena). */
  scenes?:         SavedScene[]
  activeSceneId?:  number
  /** Omitido cuando `scenes[]` define el mundo por escena. */
  world?:          SavedWorldConfig
  backgroundPath:  string | null
  entities:        Entity3D[]
  player:          Entity3D | null
  config_camera:   ConfigCamera | null
  config_editor_camera: ConfigEditorCamera | null
  blueprints?:     Blueprint3D[]
  camera2d:        { x: number; y: number; halfH: number } | null
  savedAt:         string   // ISO timestamp
  /** Sprites precargados en el proyecto (nombre -> ruta relativa). */
  sprites?:        Array<{ name: string; path: string }>
  /** Biblioteca de modelos 3D importados (única fuente en proyectos 3D). */
  resources?: {
    models?: ResourceModelEntry[]
  }
  /** Sonidos precargados en el proyecto. */
  sounds?:         Array<{ name: string; path: string }>
  /** Fuentes precargadas en el proyecto. */
  fonts?:          Array<{ name: string; path: string }>
  /** Fondos precargados en el proyecto. */
  backgrounds?:    Array<{ name: string; path: string }>
  /** Idioma/locale del proyecto (en | es). */
  language?:       string
  /** Pantallas UI del jugador (editor). */
  playerUiScreens?: Array<{ id: string; name: string; active?: boolean }>
  /** Pantallas UI de menú (editor). */
  menuUiScreens?:  Array<{ id: string; name: string }>
  /** Cuadros de texto HUD por pantalla (motor). */
  playerUiTextBoxes?: SavedPlayerUiTextBox[]
  /** Botones HUD por pantalla (motor). */
  playerUiButtons?: SavedPlayerUiButton[]
  /** Imágenes HUD por pantalla (motor). */
  playerUiImages?: SavedPlayerUiImage[]
  /** Objetos HUD poligonales por pantalla (motor). */
  playerUiObjects?: SavedPlayerUiObject[]
  /** Biblioteca de imágenes HUD (Resources). */
  hudImages?: Array<{ name: string; path: string }>
}

export interface SavedPlayerUiTextBox {
  scope:       string
  screen_id:   string
  id:          number
  font_path:   string
  font_name:   string
  text:        string
  center_x:    number
  center_y:    number
  width:       number
  height:      number
  z_index?:    number
  locked?:     boolean
}

export interface SavedPlayerUiButton {
  scope: string
  screen_id: string
  id: number
  type: string
  round: number
  background_color: [number, number, number, number]
  texture_path?: string | null
  transparency_background: number
  text: string
  text_color: [number, number, number, number]
  transparency_text: number
  font_path: string
  font_name: string
  border_color: [number, number, number, number]
  border_weight: number
  center_x: number
  center_y: number
  width: number
  height: number
  source_aspect?: number
  z_index?: number
  locked?: boolean
}

export interface SavedPlayerUiImage {
  scope: string
  screen_id: string
  id: number
  image_path: string
  image_name: string
  center_x: number
  center_y: number
  width: number
  height: number
  source_aspect: number
  z_index?: number
  locked?: boolean
}

export interface SavedPlayerUiObject {
  scope: string
  screen_id: string
  id: number
  vertices: [number, number][]
  fill_color: [number, number, number, number]
  texture_path?: string | null
  z_index?: number
  locked?: boolean
}

export interface OpenProjectResult {
  /** Ruta del archivo `.save` (persistencia / autosave). */
  filePath:   string
  /** Directorio temporal con el contenido extraído del zip. */
  extractDir: string
  /** Solo `type`/`gameStyle`; escena la carga el motor desde `extractDir`. */
  project:    ProjectSaveData | OpenProjectManifestMeta
}

export interface EngineCommand {
  cmd: EngineCommandName2D | EngineCommandName3D
  [key: string]: unknown
}

export type {
  EngineCommand2D,
  EngineCommand3D,
  EngineCommandName2D,
  EngineCommandName3D,
  EngineCommandNameShared,
  Engine2dApi,
  Engine3dApi,
} from './engineCommands'

export {
  ENGINE_COMMANDS_SHARED,
  ENGINE_COMMANDS_2D_ONLY,
  ENGINE_COMMANDS_3D_ONLY,
  ENGINE_COMMAND_SET_2D,
  ENGINE_COMMAND_SET_3D,
  engineCommandSetFor,
  isCommandAllowedForMotor,
} from './engineCommandCatalog'

export interface EngineEvent {
  event: 'ready' | 'pong' | 'error' | 'model_loaded' | 'entity_model_replaced' | 'model_clips_ready' | 'stopped' | 'entity_selected' | 'entity_deselected' | 'entity_properties_open' | 'entity_hovered' | 'entity_unhovered' | 'scenario_loaded' | 'character_loaded' | 'scene_imported' | 'project_loaded_2d' | 'project_loaded_3d' | 'project_load_2d_complete' | 'project_load_3d_complete' | 'load_progress' | 'camera_2d_updated' | 'background_loaded' | 'drawing_progress' | 'collider_created' | 'execution_area_created' | 'tool_cancelled' | 'pivot_selected' | 'physics_changed' | 'sprite_loaded' | 'sprite_removed' | 'sprites_list' | 'model_asset_loaded' | 'model_asset_removed' | 'models_list' | 'world_gen_status' | 'world_gen_progress' | 'world_gen_chunk_ready' | 'world_gen_completed' | 'world_gen_error' | 'world_gen_scatter_done' | 'sound_loaded' | 'sound_removed' | 'sounds_list' | 'font_loaded' | 'font_removed' | 'fonts_list' | 'hud_image_loaded' | 'hud_image_removed' | 'hud_images_list' | 'background_asset_loaded' | 'background_asset_removed' | 'backgrounds_list' | 'play_character_view_changed' | 'save_snapshot_ready' | 'default_scene_name_ready' | 'editor_scene_created' | 'editor_scene_switched' | 'editor_scene_switch_blocked' | 'editor_scenes_updated' | 'debug_metrics' | 'preview_playing_changed' | 'trigger_entered' | 'trigger_exited' | 'entity_removed' | 'quick_build_move' | 'quick_build_click' | 'animation_logical_resolved' | 'animation_finished' | 'autosave_tick' | 'atlas_exhausted' | 'player_ui_image_added' | 'player_ui_image_removed' | 'player_ui_object_added' | 'player_ui_object_removed' | 'player_ui_object_draw_ended'
  [key: string]: unknown
}

export interface ResourceModelEntry {
  id: string
  name: string
  type: ModelCategory
  asset: string
  importer_version: number
  /** Escena propietaria (recurso por escena). `undefined` = biblioteca global. */
  scene_id?: number
  /** `true` si el usuario globalizó el modelo (override del enrutado por escena). */
  global?: boolean
}

// Extiende la interfaz global Window para el renderer
declare global {
  interface Window {
    engine2d: import('./engineCommands').Engine2dApi
    engine3d: import('./engineCommands').Engine3dApi
    electronAPI: {
      setGameStyle:            (payload: EngineStartPayload) => void
      openProjectDialog:       () => Promise<OpenProjectResult | null>
    }
  }
}
