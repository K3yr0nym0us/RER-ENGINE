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

/** Escala del mesh placeholder del jugador 3D (debe coincidir con `engine_3d`). */
export const PLAY_CHARACTER_BODY_SCALE: [number, number, number] = [0.8, 1.7, 0.8]

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

export function isMinimalOpenProject(
  project: ProjectSaveData | OpenProjectManifestMeta,
): project is OpenProjectManifestMeta {
  return !('version' in project)
}

export interface ProjectConfig {
  type:      ProjectType
  /** Modo de cámara 3D (manifest); en 2D define perspectiva del juego (top-down, etc.). */
  gameStyle: GameStyle
}

/** Debe coincidir con `DEFAULT_GRAVITY_MAGNITUDE` en `engine_shared` (Rust). */
export const DEFAULT_GRAVITY_MAGNITUDE = 15

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

export function isEditorBoxPath(p: string | null | undefined): boolean {
  return entityPathMarker(p) === '[EditorBox]'
}

export function isPlayerPath(p: string | null | undefined): boolean {
  return entityPathMarker(p) === '[Player]'
}

export function isEditorCameraPath(p: string | null | undefined): boolean {
  return entityPathMarker(p) === '[EditorCamera]'
}

export function isEditorCameraEntity(
  id: number,
  meta: { path?: string } | undefined,
  editorCameraEntityId: number | null,
): boolean {
  return isEditorCameraPath(meta?.path) || editorCameraEntityId === id
}

export function isSunPath(p: string | null | undefined): boolean {
  return entityPathMarker(p) === '[Sun]'
}

export function isGroundPath(p: string | null | undefined): boolean {
  return entityPathMarker(p) === '[Ground]'
}

/** Categoría IPC del motor para colisión/nombrado. */
export type EntityCategory = 'environment' | 'object' | 'character' | 'weapon' | 'projectile'

export function isPlayerEntity(
  id: number,
  meta: { path?: string } | undefined,
  playerEntityId: number | null,
): boolean {
  return isPlayerPath(meta?.path) || playerEntityId === id
}

export function isEnvironmentEntity(
  isScenario: boolean,
  meta: { path?: string; entityCategory?: EntityCategory } | undefined,
): boolean {
  return isScenario || isEditorBoxPath(meta?.path) || meta?.entityCategory === 'environment'
}

// ── Modelo 3D (docs/Entities_Model_3D.yaml) ─────────────────────────────────

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

// ── 2D legacy (engine_2d) ───────────────────────────────────────────────────

export interface SavedEntity {
  id:               number
  /** Nombre visible de la entidad en el editor. */
  name?:            string
  path:             string
  kind:             'model' | 'scenario' | 'character' | 'collider' | 'execution_area' | 'directional_light'
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
    pivot_x: number
    pivot_y: number
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

/** Valores por defecto de luz direccional 3D (alineados con el motor). */
export const DEFAULT_LIGHT_AMBIENT = 0.06
export const DEFAULT_LIGHT_INTENSITY = 1.0
export const DEFAULT_SHADOW_DARKNESS = 0.22

export interface SavedWorldConfig {
  worldWidth:   number
  worldHeight:  number
  worldDepth?:  number
  /** 3D: radio de la esfera de límites. */
  worldRadius?: number
  gridVisible:  boolean
  gridCellSize: number
  gravity?:     number
  targetFps:    number
  /** 3D: ambiente 0–1 (caras no iluminadas). */
  lightAmbient?:     number
  /** 3D: intensidad de la luz direccional. */
  lightIntensity?:   number
  /** 3D: oscuridad de sombras proyectadas (0.02–1, menor = más oscuro). */
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
  shadowTier?: 'low' | 'medium' | 'high' | 'ultra'
}

/** @deprecated 3D: `player` + `config_camera`. Solo runtime 2D / migración UI. */
export interface SavedPlayerTransform {
  /** Pies del Player en el mundo. */
  position: [number, number, number]
  /** Posición absoluta del ojo de la cámara play character, independiente del Player (3D). */
  camera_eye_position?: [number, number, number]
  /** Yaw del cono de cámara play character en editor (rad); distinto del viewport orbital. */
  fps_camera_yaw?: number
  /** Pitch del cono de cámara play character en editor (rad). */
  fps_camera_pitch?: number
  scale:    [number, number, number]
  /** 3D: yaw de cámara en radianes. */
  yaw?:     number
  /** 3D: pitch de cámara en radianes. */
  pitch?:   number
  /** Modelo visual (.glb/.fbx) del jugador si se reemplazó el placeholder. */
  visual_model_path?: string
  /** FOV vertical de la cámara en radianes (3D, cámara play character). */
  fov_y?: number
  /** Alcance del gizmo de frustum en el editor (metros). */
  frustum_distance?: number
  /** Seguimiento del ojo de cámara play character respecto al jugador en editor. */
  camera_follow_mode?: PlayCameraFollowMode
  /** Bindings de control Rhai del jugador principal. */
  control_bindings?: SavedControlBindings
  /** Scripts Rhai adjuntos al jugador (no confundir con scripts embebidos en bindings). */
  scripts?: SavedScript[]
  /** Rotación del mesh del jugador (quaternion xyzw) en editor. */
  body_rotation?: [number, number, number, number]
  /** Escala del transform del jugador en editor. */
  body_scale?: [number, number, number]
  /** Cápsula de colisión del jugador tras reemplazar el mesh (motor). */
  mesh_collision_extents?: {
    local_min_y: number
    local_max_y: number
    radius_xz: number
  }
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

/** Metadatos de editor enviados por el motor 2D tras cargar desde `extract_dir`. */
/** Metadatos de editor enviados por el motor 3D tras cargar desde `extract_dir`. */
export interface ProjectLoaded3dPayload {
  activeSceneId: number
  sceneName:     string
  entityCount:   number
  scenes?:       Array<{ id: number; name: string }>
  language?:     string
  models:        Array<{ name: string; path: string; category?: ModelCategory; model_id?: string; asset?: string }>
  sounds:        Array<{ name: string; path: string }>
  fonts:         Array<{ name: string; path: string }>
  backgrounds:   Array<{ name: string; path: string }>
  hudImages?:    Array<{ name: string; path: string }>
  blueprints:    Blueprint3D[]
  world:         SavedWorldConfig
  player?: Entity3D | null
  config_camera?: ConfigCamera | null
  config_editor_camera?: ConfigEditorCamera | null
  playerUiScreens?: Array<{ id: string; name: string; active?: boolean }>
  menuUiScreens?:  Array<{ id: string; name: string }>
}

export interface ProjectLoaded2dPayload {
  activeSceneId: number
  sceneName:     string
  entityCount:   number
  scenes?:       Array<{ id: number; name: string }>
  language?:     string
  sprites:       Array<{ name: string; path: string }>
  sounds:        Array<{ name: string; path: string }>
  fonts:         Array<{ name: string; path: string }>
  backgrounds:   Array<{ name: string; path: string }>
  blueprints:    BluePrintEntry[]
  world:         SavedWorldConfig
  backgroundPath: string | null
  camera2d:      { x: number; y: number; halfH: number } | null
  hudImages?:    Array<{ name: string; path: string }>
  playerUiScreens?: Array<{ id: string; name: string; active?: boolean }>
  menuUiScreens?: Array<{ id: string; name: string }>
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
  EngineApi,
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

export interface EditorSceneListItem {
  id: number
  name: string
  dirty?: boolean
}

export interface EngineEvent {
  event: 'ready' | 'pong' | 'error' | 'model_loaded' | 'entity_model_replaced' | 'model_clips_ready' | 'stopped' | 'entity_selected' | 'entity_deselected' | 'entity_hovered' | 'entity_unhovered' | 'scenario_loaded' | 'character_loaded' | 'scene_imported' | 'project_loaded_2d' | 'project_loaded_3d' | 'project_load_2d_complete' | 'project_load_3d_complete' | 'load_progress' | 'camera_2d_updated' | 'background_loaded' | 'drawing_progress' | 'collider_created' | 'execution_area_created' | 'tool_cancelled' | 'pivot_selected' | 'physics_changed' | 'sprite_loaded' | 'sprite_removed' | 'sprites_list' | 'model_asset_loaded' | 'model_asset_removed' | 'models_list' | 'sound_loaded' | 'sound_removed' | 'sounds_list' | 'font_loaded' | 'font_removed' | 'fonts_list' | 'hud_image_loaded' | 'hud_image_removed' | 'hud_images_list' | 'background_asset_loaded' | 'background_asset_removed' | 'backgrounds_list' | 'play_character_view_changed' | 'save_snapshot_ready' | 'default_scene_name_ready' | 'editor_scene_created' | 'editor_scene_switched' | 'editor_scene_switch_blocked' | 'editor_scenes_updated' | 'debug_metrics' | 'preview_playing_changed' | 'trigger_entered' | 'trigger_exited' | 'entity_removed' | 'quick_build_move' | 'quick_build_click' | 'animation_logical_resolved' | 'animation_finished' | 'autosave_tick' | 'atlas_exhausted' | 'player_ui_image_added' | 'player_ui_image_removed' | 'player_ui_object_added' | 'player_ui_object_removed' | 'player_ui_object_draw_ended'
  [key: string]: unknown
}

/** Escena activa exportada por el motor 3D (`export_save_snapshot`). */
export interface EngineSaveSceneSnapshot {
  world: {
    world_radius: number
    world_width: number
    world_height: number
    world_depth: number
    grid_visible: boolean
    grid_cell_size: number
    gravity: number
    target_fps: number
    light_ambient?: number | null
    light_intensity?: number | null
    shadow_darkness?: number | null
  }
  background_path?: string | null
  entities: Entity3D[]
  player?: Entity3D | null
  config_camera?: ConfigCamera | null
  config_editor_camera?: ConfigEditorCamera | null
  camera2d?: { x: number; y: number; half_h: number } | null
  sprites: Array<{ name: string; path: string }>
  models?: Array<{ name: string; path: string; category?: ModelCategory }>
  sounds: Array<{ name: string; path: string }>
  backgrounds: Array<{ name: string; path: string }>
  fonts?: Array<{ name: string; path: string }>
  hud_images?: Array<{ name: string; path: string }>
  player_ui_text_boxes?: Array<{
    scope: string
    screen_id: string
    id: number
    font_path: string
    font_name: string
    text: string
    center_x: number
    center_y: number
    width: number
    height: number
    z_index?: number
    locked?: boolean
  }>
  player_ui_buttons?: Array<{
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
  }>
  player_ui_images?: Array<{
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
  }>
  player_ui_objects?: Array<{
    scope: string
    screen_id: string
    id: number
    vertices: [number, number][]
    fill_color: [number, number, number, number]
    texture_path?: string | null
    z_index?: number
    locked?: boolean
  }>
}

export interface SaveSnapshotReady {
  event: 'save_snapshot_ready'
  scene: EngineSaveSceneSnapshot
}

export interface DefaultSceneNameReady {
  event: 'default_scene_name_ready'
  id: number
  name: string
}

export interface PlayCharacterViewChanged {
  event:                 'play_character_view_changed'
  player_id:             number | null
  position:              [number, number, number]
  camera_eye_position?:  [number, number, number]
  fps_camera_yaw?:       number
  fps_camera_pitch?:     number
  yaw:                   number
  pitch:                 number
  fov_y:                 number
  frustum_distance:      number
  camera_follow_mode?:   PlayCameraFollowMode
  body_center:           [number, number, number]
  body_rotation:         [number, number, number, number]
  body_scale:            [number, number, number]
  sync_editor_viewport?: boolean
  editor_camera_id?: number | null
  editor_orbit_target?: [number, number, number]
}

/** Plataforma para métricas GPU del shell Electron (no del motor Rust). */
export type GpuMetricsPlatform = 'windows' | 'linux' | 'darwin' | 'other'

export interface AppResourceUsage {
  /** Suma de procesos Chromium/Electron de esta app (no del sistema). */
  electronCpuPercent: number
  /** % GPU de procesos Electron por PID (solo implementado en Windows). */
  electronGpuPercent: number | null
  gpuMetricsPlatform: GpuMetricsPlatform
  /** Si esta plataforma puede leer % GPU de procesos Electron (hoy solo Windows). */
  electronGpuMetricsSupported: boolean
}

export interface DebugMetrics {
  fps:            number
  frame_time_ms:  number
  draw_calls:     number
  physics_bodies: number
  /** % CPU del proceso del motor (Rust/wgpu). */
  cpu_percent?:   number
  /** % GPU del proceso del motor (contadores por PID; no uso global del SO). */
  gpu_percent?:   number
  /** 3D: posición de pies del personaje jugable. */
  play_character_position?: [number, number, number]
  play_character_yaw?:     number
  play_character_pitch?:   number
}

export interface Camera2dUpdated {
  event:  'camera_2d_updated'
  x:      number
  y:      number
  half_h: number
}

export interface ScenarioLoaded {
  event: 'scenario_loaded'
  id:    number
  path:  string
  name?: string
  img_width: number
  img_height: number
  default_pivot_x: number
  default_pivot_y: number
}

export interface CharacterLoaded {
  event: 'character_loaded'
  id:    number
  path:  string
  img_width: number
  img_height: number
  default_pivot_x: number
  default_pivot_y: number
}

export interface EntityHovered {
  event: 'entity_hovered'
  id:    number
}

export interface EntityUnhovered {
  event: 'entity_unhovered'
}

export interface BackgroundLoaded {
  event: 'background_loaded'
  path:  string
}

export interface AnimationFinished {
  event:            'animation_finished'
  entity_id:        number
}

export interface PhysicsChanged {
  event:       'physics_changed'
  entity_id:   number
  enabled:     boolean
  body_type:   string
}

export interface EntitySelected {
  event:           'entity_selected'
  id:              number
  name:            string
  position:        [number, number, number]
  rotation:        [number, number, number, number]  // quaternion xyzw
  scale:           [number, number, number]
  physics_enabled: boolean
  physics_type:    string
  blueprint_id?:   string
}

export interface PivotSelected {
  event:      'pivot_selected'
  frame_path: string
  pivot_x:    number
  pivot_y:    number
}

export interface SpriteLoaded {
  event:  'sprite_loaded'
  path:   string
  width:  number
  height: number
}

export interface SpriteRemoved {
  event:  'sprite_removed'
  path:  string
}

export interface SpritesList {
  event:  'sprites_list'
  sprites: SpriteInfo[]
}

export interface TriggerEntered {
  event:      'trigger_entered'
  trigger_id: number
  actor_id:   number
  /** Scripts Rhai adjuntos en runtime (motor). */
  has_attached_script?: boolean
}

export interface TriggerExited {
  event:      'trigger_exited'
  trigger_id: number
  actor_id:   number
}

export interface EntityRemoved {
  event: 'entity_removed'
  id: number
  kind: 'scenario' | 'character' | 'model' | 'collider' | 'execution_area' | 'directional_light'
  /** Vértices del quad (solo colisionadores / áreas de ejecución 2D). */
  points?: [[number, number], [number, number], [number, number], [number, number]]
}

export interface AnimationLogicalResolved {
  event: 'animation_logical_resolved'
  id: number
  name: string
  logical_w: number
  logical_h: number
}

export interface SpriteInfo {
  path:   string
  name:   string
  width:  number
  height: number
}

export interface ModelInfo {
  path: string
  name: string
  /** true mientras el motor precarga en segundo plano */
  loading?: boolean
  /** Categoría de uso en modales de entidades 3D. */
  category?: ModelCategory
  /** ID estable del asset importado (.rerasset). */
  model_id?: string
  /** Ruta relativa al `.rerasset` en el `.save`. */
  asset?: string
  /** importing | ready | failed */
  state?: string
}

export interface ResourceModelEntry {
  id: string
  name: string
  type: ModelCategory
  asset: string
  importer_version: number
}

export interface SoundInfo {
  path: string
  name: string
}

export interface FontInfo {
  path: string
  name: string
}

export interface HudImageInfo {
  path: string
  name: string
}

export interface BackgroundInfo {
  path: string
  name: string
}

/** Pestañas de construcción rápida (inglés, alineado a `Entity3DCategory`). */
export type BlueprintTabCategory = 'character' | 'environment' | 'object'

/** @deprecated Usar `BlueprintTabCategory` */
export type BluePrintCategory = BlueprintTabCategory

/** @deprecated 3D usa `Blueprint3D` */
export type BluePrintEntry = Blueprint3D

export interface ViewportBounds {
  x:      number
  y:      number
  width:  number
  height: number
}

/** Tamaños de ventana modal Electron (aprox. Bootstrap 5). */
export type ModalElectronSize = 'sm' | 'md' | 'lg' | 'xl' | 'xxl'

export interface ModalElectronOpenRequest {
  size?: ModalElectronSize
  title: string
  handlerId: string
  componentKey: string
  /** Idioma de la ventana principal (`en` | `es`). */
  locale?: string
  /** Permite redimensionar la ventana modal (p. ej. editor de nodos). */
  resizable?: boolean
  /** Cubre la ventana principal y no se cierra con la X hasta `closeModalElectron`. */
  blockingOverlay?: boolean
  props: Record<string, unknown>
  /** Props función registrados en el renderer principal (no serializables). */
  callbackKeys?: string[]
  fonts?: FontInfo[]
  hudImages?: HudImageInfo[]
  sprites?: Array<{ path: string; name: string; width: number; height: number }>
  models?: ModelInfo[]
  blueprints?: Blueprint3D[]
  /** blueprintId → número de entidades vinculadas (para diálogo de borrado). */
  linkedEntityCounts?: Record<string, number>
  /** Estado inicial del editor Player UI (modal Electron). */
  playerUiEditorState?: Record<string, unknown>
  /** Estado del panel de propiedades de entidad (modal Electron). */
  entityPropertiesState?: Record<string, unknown>
  /** Estado de configuración de sockets (modal Electron). */
  socketConfigModalState?: Record<string, unknown>
  /** Entidades de escena para el editor de nodos (snapshot IPC, sin `undefined`). */
  sceneEntities?: Array<{
    id: number
    name: string
    category: Entity3DCategory
    model: string
    colision: boolean
    blueprint_id?: string
    animations?: Array<{ name: string }>
  }>
}

export interface ModalElectronDelegateRequest {
  handlerId: string
  action: 'deleteWithEntities' | 'deleteKeepEntities'
  blueprint: Blueprint3D
}

export interface ModalElectronResultPayload {
  handlerId: string
  result: unknown
  callbackKey?: string
}

// Extiende la interfaz global Window para el renderer
declare global {
  interface Window {
    engine: import('./engineCommands').EngineApi
    engine2d: import('./engineCommands').Engine2dApi
    engine3d: import('./engineCommands').Engine3dApi
    electronAPI: {
      setGameStyle:            (payload: EngineStartPayload) => void
      sendViewportBounds:      (bounds: ViewportBounds) => void
      openModelDialog:         () => Promise<string | null>
      openProjectDialog:       () => Promise<OpenProjectResult | null>
      saveProject:             (data: ProjectSaveData) => Promise<string | null>
      /** Diálogo «Guardar como»; devuelve la ruta elegida sin empaquetar. */
      pickProjectSavePath:     () => Promise<string | null>
      saveProjectSilent:       (filePath: string, data: ProjectSaveData) => Promise<boolean>
      getProjectExtractDir:    () => Promise<string | null>
      /** Manifest completo del .save abierto (escenas inactivas, visualGraph, etc.). */
      readProjectManifest:     () => Promise<ProjectSaveData | null>
      openSpriteDialog:        () => Promise<string | null>
      openHudImageDialog:      () => Promise<string | null>
      openScenarioDialog:      () => Promise<string | null>
      openCharacterDialog:     () => Promise<string | null>
      getImageDataUrl:         (filePath: string) => Promise<string | null>
      openBackgroundDialog:    () => Promise<string | null>
      openAudioDialog:         () => Promise<string | null>
      openFontDialog:          () => Promise<string | null>
      onRequestViewportBounds: (cb: () => void) => void
      onAutoSaveRequest:       (cb: (filePath: string) => void) => void
      getAppResourceUsage:     () => Promise<AppResourceUsage>
      hideEngineViewport:      () => void
      restoreEngineViewport:   (bounds?: ViewportBounds) => void
      openModalElectron:       (request: ModalElectronOpenRequest) => Promise<void>
      closeModalElectron:      () => Promise<void>
      completeModalElectron:   (handlerId: string, result: unknown, callbackKey?: string) => void
      notifyModalElectronReady: () => void
      resizeModalElectron:     (contentHeight: number) => void
      delegateModalElectron:   (request: ModalElectronDelegateRequest) => Promise<{ blueprints?: Blueprint3D[] } | null>
      onModalElectronDelegateRequest: (
        cb: (request: ModalElectronDelegateRequest) => Promise<{ blueprints?: Blueprint3D[] } | null>,
      ) => () => void
      onModalElectronRender:   (cb: (payload: ModalElectronOpenRequest | null) => void) => () => void
      onModalElectronResult:   (cb: (handlerId: string, result: unknown, callbackKey?: string) => void) => () => void
      onModalElectronParentOpenRequest: (
        cb: (req: { parentHandlerId: string; action: string; payload?: Record<string, unknown> }) => void,
      ) => () => void
      requestParentModalOpen: (req: {
        parentHandlerId: string
        action: string
        payload?: Record<string, unknown>
      }) => void
      patchModalElectron: (data: {
        handlerId: string
        playerUiEditorState?: unknown
        entityPropertiesState?: unknown
        socketConfigModalState?: unknown
        models?: ModelInfo[]
      }) => void
      entityPropertiesAction: (handlerId: string, action: unknown) => Promise<void>
      socketConfigModalAction: (handlerId: string, action: unknown) => Promise<void>
      onModalElectronEntityPropertiesActionRequest: (
        cb: (req: { handlerId: string; action: unknown; requestId: string }) => void | Promise<void>,
      ) => () => void
      onModalElectronSocketConfigModalActionRequest: (
        cb: (req: { handlerId: string; action: unknown; requestId: string }) => void | Promise<void>,
      ) => () => void
      onModalElectronClosed: (
        cb: (data: { componentKey?: string }) => void,
      ) => () => void
      playerUiEditorAction: (handlerId: string, action: unknown) => Promise<void>
      fetchPlayerUiEditorState: (handlerId: string) => Promise<unknown>
      onModalElectronPatch: (
        cb: (data: {
          handlerId: string
          playerUiEditorState?: unknown
          entityPropertiesState?: unknown
          socketConfigModalState?: unknown
          models?: ModelInfo[]
        }) => void,
      ) => () => void
      onModalElectronPlayerUiActionRequest: (
        cb: (req: { handlerId: string; action: unknown; requestId: string }) => void | Promise<void>,
      ) => () => void
      onModalElectronPlayerUiStateRequest: (
        cb: (req: { handlerId: string; requestId: string }) => unknown,
      ) => () => void
      pluginsGetCatalog: () => Promise<import('./plugins').PluginCatalogEntry[]>
      pluginsGetState: () => Promise<import('./plugins').PluginsState>
      pluginsSetEnabled: (
        pluginId: import('./plugins').PluginId,
        enabled: boolean,
      ) => Promise<import('./plugins').PluginsState>
      pluginsInstall: (
        pluginId: import('./plugins').PluginId,
      ) => Promise<import('./plugins').PluginInstallResult>
      pluginsCancelInstall: () => Promise<{ ok: boolean }>
      pluginsUninstall: (
        pluginId: import('./plugins').PluginId,
      ) => Promise<import('./plugins').PluginInstallResult>
      pluginsGetLlmStatus: () => Promise<{
        status: string
        error: string | null
        enabled: boolean
        installed: boolean
      }>
      pluginsChat: (
        request: import('./plugins').AssistantChatRequest,
      ) => Promise<import('./plugins').AssistantChatResponse>
      pluginsStartLlm: () => Promise<{ ok: boolean; error?: string }>
      pluginsStopLlm: () => Promise<{ ok: boolean }>
      onPluginsDownloadProgress: (
        cb: (progress: import('./plugins').PluginDownloadProgress) => void,
      ) => () => void
      onPluginsUiAction: (
        cb: (action: import('./plugins').PluginUiAction) => void,
      ) => () => void
      onPluginsStateChanged: (cb: () => void) => () => void
      aiAssistantShow: (config: { locale?: 'en' | 'es' }) => Promise<void>
      aiAssistantHide: () => Promise<void>
      aiAssistantSetLayout: (layout: 'intro' | 'thinking' | 'input' | 'answer') => void
      aiAssistantFabDragStart: () => void
      aiAssistantFabDragEnd: () => void
      notifyAiAssistantReady: () => void
      onAiAssistantConfig: (
        cb: (config: { locale?: 'en' | 'es' } | null) => void,
      ) => () => void
    }
  }
}
