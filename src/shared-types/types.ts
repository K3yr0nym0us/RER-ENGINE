// Tipos compartidos entre main process, preload y renderer.

export type ProjectType = '2D' | '3D'

export type GameStyle =
  | 'first-person'
  | 'second-person'
  | 'third-person'
  | 'top-down'
  | 'side-scroller'
  | 'isometric'

/** Sincroniza tipo de proyecto, modo y rutas con Electron antes de arrancar el motor. */
export interface EngineStartPayload {
  projectType: ProjectType
  /** Estilo 3D (`first-person`, etc.); `false` en proyectos 2D. */
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

/** Categoría IPC del motor para colisión/nombrado (`environment` | `object` | `character`). */
export type EntityCategory = 'environment' | 'object' | 'character'

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

/** Escala del mesh placeholder del jugador FP (debe coincidir con `engine_3d`). */
export const FIRST_PERSON_PLAYER_BODY_SCALE: [number, number, number] = [0.8, 1.7, 0.8]

// ── Modelo 3D (docs/Entities_Model_3D.yaml) ─────────────────────────────────

export type Entity3DCategory =
  | 'environment'
  | 'character'
  | 'player'
  | 'object'
  | 'sun'
  | 'ground'

export type PhysicsType3D = 'dynamic' | 'static' | 'kinematic'

/** Modo de seguimiento del ojo FPS respecto al jugador en editor. */
export type PlayCameraFollowMode = 'follow_character' | 'move_with_character'

/** WASD / gamepad → Lua (solo `category: player` en instancias). */
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
  position: [number, number, number]
  rotation: [number, number, number, number]
  scale: [number, number, number]
  physics_type?: PhysicsType3D
  colision: boolean
  animations?: SavedAnimation[]
  scripts?: SavedScript[]
  blueprint_id?: string
  controls?: SavedControls
}

/** Plantilla (`project.blueprints[]` / manifest). */
export interface Blueprint3D {
  id: string
  name: string
  category: Entity3DCategory
  model: string
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
  /** Scripts Lua adjuntos a esta entidad. */
  scripts?:         SavedScript[]
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
  /** Scripts Lua asociados a esta animación. */
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
  /** Código fuente Lua completo. */
  source: string
}

export interface SavedControlBindings {
  keyboard_mouse: Record<string, SavedScript>
  gamepad: Record<string, SavedScript>
}

/** Valores por defecto de luz direccional 3D (alineados con el motor). */
export const DEFAULT_LIGHT_AMBIENT = 0.06
export const DEFAULT_LIGHT_INTENSITY = 1.0
export const DEFAULT_SHADOW_DARKNESS = 0.22

export interface SavedWorldConfig {
  worldWidth:   number
  worldHeight:  number
  worldDepth?:  number
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
}

/** @deprecated 3D: `player` + `config_camera`. Solo runtime 2D / migración UI. */
export interface SavedPlayerTransform {
  /** Pies del Player en el mundo. */
  position: [number, number, number]
  /** Posición absoluta del ojo de la cámara FPS, independiente del Player (3D FP). */
  camera_eye_position?: [number, number, number]
  /** Yaw del cono FPS en editor (rad); distinto del viewport orbital. */
  fps_camera_yaw?: number
  /** Pitch del cono FPS en editor (rad). */
  fps_camera_pitch?: number
  scale:    [number, number, number]
  /** 3D: yaw de cámara en radianes. */
  yaw?:     number
  /** 3D: pitch de cámara en radianes. */
  pitch?:   number
  /** Modelo visual (.glb/.fbx) del jugador si se reemplazó el placeholder. */
  visual_model_path?: string
  /** FOV vertical de la cámara en radianes (3D FP). */
  fov_y?: number
  /** Alcance del gizmo de frustum en el editor (metros). */
  frustum_distance?: number
  /** Seguimiento del ojo FPS respecto al jugador en editor. */
  camera_follow_mode?: PlayCameraFollowMode
  /** Bindings de control Lua del jugador principal. */
  control_bindings?: SavedControlBindings
  /** Scripts Lua adjuntos al jugador (no confundir con scripts embebidos en bindings). */
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
}

export interface ProjectSaveData {
  version:         number
  type:            ProjectType
  gameStyle:       GameStyle
  /** Escenas del proyecto (multi-escena). */
  scenes?:         SavedScene[]
  activeSceneId?:  number
  world:           SavedWorldConfig
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
  /** Modelos 3D precargados en el proyecto. */
  models?:         Array<{ name: string; path: string; category?: ModelCategory }>
  /** Sonidos precargados en el proyecto. */
  sounds?:         Array<{ name: string; path: string }>
  /** Fuentes precargadas en el proyecto. */
  fonts?:          Array<{ name: string; path: string }>
  /** Fondos precargados en el proyecto. */
  backgrounds?:    Array<{ name: string; path: string }>
  /** Blueprints creados en el proyecto. */
  blueprints?:     Blueprint3D[]
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
  models:        Array<{ name: string; path: string; category?: ModelCategory }>
  sounds:        Array<{ name: string; path: string }>
  fonts:         Array<{ name: string; path: string }>
  backgrounds:   Array<{ name: string; path: string }>
  blueprints:    Blueprint3D[]
  world:         SavedWorldConfig
  player?: Entity3D | null
  config_camera?: ConfigCamera | null
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
}

export interface EngineCommand {
  cmd:
    | 'ping'
    | 'shutdown'
    | 'set_clear_color'
    | 'resize'
    | 'set_bounds'
    | 'load_model'
    | 'spawn_cached_model'
    | 'replace_entity_model'
    | 'set_transform'
    | 'set_entity_name'
    | 'set_scene'
    | 'load_scenario'
    | 'set_scenario_scale'
    | 'duplicate_scenario'
    | 'load_character'
    | 'set_character_scale'
    | 'duplicate_character'
    | 'remove_entity'
    | 'set_world_size'
    | 'set_grid_visible'
    | 'set_grid_cell_size'
    | 'set_target_fps'
    | 'set_gravity'
    | 'set_directional_light'
    | 'set_ctrl_held'
    | 'set_physics'
    | 'set_active_tool'
    | 'create_collider_from_points'
    | 'create_execution_area_from_points'
    | 'play_animation_frame'
    | 'restore_animation_frame'
    | 'set_pivot_edit_mode'
    | 'cancel_pivot_edit_mode'
    | 'set_logical_area_mode'
    | 'cancel_logical_area_mode'
    | 'play_audio'
    | 'stop_audio'
    | 'set_animation'
    | 'remove_animation'
    | 'set_default_animation'
    | 'play_animation'
    | 'stop_animation'
    | 'load_script'
    | 'set_control_bindings'
    | 'unload_script'
    | 'load_sprite'
    | 'remove_sprite'
    | 'get_sprites_list'
    | 'load_model_asset'
    | 'remove_model_asset'
    | 'get_models_list'
    | 'load_sound'
    | 'remove_sound'
    | 'get_sounds_list'
    | 'load_font'
    | 'remove_font'
    | 'get_fonts_list'
    | 'load_hud_image'
    | 'remove_hud_image'
    | 'get_hud_images_list'
    | 'add_player_ui_image'
    | 'remove_player_ui_image'
    | 'load_background_asset'
    | 'remove_background_asset'
    | 'get_backgrounds_list'
    | 'set_preview_playing'
    | 'set_player_ui_edit_mode'
    | 'add_player_ui_text_box'
    | 'remove_player_ui_text_box'
    | 'add_player_ui_button'
    | 'remove_player_ui_button'
    | 'set_play_character_view'
    | 'set_play_character_spawn'
    | 'set_first_person_view'
    | 'set_first_person_spawn'
    | 'run_control_script'
    | 'undo'
    | 'clear_background'
    | 'reload_asset'
    | 'set_locale'
    | 'set_autosave'
    | 'set_debug_mode'
    | 'export_save_snapshot'
    | 'get_default_scene_name'
  [key: string]: unknown
}

export interface EngineEvent {
  event: 'ready' | 'pong' | 'error' | 'model_loaded' | 'entity_model_replaced' | 'model_clips_ready' | 'stopped' | 'entity_selected' | 'entity_deselected' | 'entity_hovered' | 'entity_unhovered' | 'scenario_loaded' | 'character_loaded' | 'scene_imported' | 'project_loaded_2d' | 'project_loaded_3d' | 'project_load_3d_complete' | 'load_progress' | 'camera_2d_updated' | 'background_loaded' | 'drawing_progress' | 'collider_created' | 'execution_area_created' | 'tool_cancelled' | 'pivot_selected' | 'physics_changed' | 'sprite_loaded' | 'sprite_removed' | 'sprites_list' | 'model_asset_loaded' | 'model_asset_removed' | 'models_list' | 'sound_loaded' | 'sound_removed' | 'sounds_list' | 'font_loaded' | 'font_removed' | 'fonts_list' | 'hud_image_loaded' | 'hud_image_removed' | 'hud_images_list' | 'background_asset_loaded' | 'background_asset_removed' | 'backgrounds_list' | 'play_character_view_changed' | 'first_person_view_changed' | 'save_snapshot_ready' | 'default_scene_name_ready' | 'debug_metrics' | 'preview_playing_changed' | 'trigger_entered' | 'trigger_exited' | 'entity_removed' | 'quick_build_move' | 'quick_build_click' | 'animation_logical_resolved' | 'animation_finished' | 'autosave_tick' | 'atlas_exhausted' | 'player_ui_image_added' | 'player_ui_image_removed'
  [key: string]: unknown
}

/** Escena activa exportada por el motor 3D (`export_save_snapshot`). */
export interface EngineSaveSceneSnapshot {
  world: {
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
  event:                 'play_character_view_changed' | 'first_person_view_changed'
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
  /** @deprecated Alias de `play_character_position` */
  first_person_position?: [number, number, number]
  /** @deprecated Alias de `play_character_yaw` */
  first_person_yaw?:     number
  /** @deprecated Alias de `play_character_pitch` */
  first_person_pitch?:   number
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

// Extiende la interfaz global Window para el renderer
declare global {
  interface Window {
    engine: {
      send: (cmd: EngineCommand) => void
      on:   (cb: (event: EngineEvent) => void) => void
      /** Quita un listener concreto; sin argumento borra todos. */
      off:  (cb?: (event: EngineEvent) => void) => void
    }
    electronAPI: {
      setGameStyle:            (payload: EngineStartPayload) => void
      sendViewportBounds:      (bounds: ViewportBounds) => void
      openModelDialog:         () => Promise<string | null>
      openProjectDialog:       () => Promise<OpenProjectResult | null>
      saveProject:             (data: ProjectSaveData) => Promise<string | null>
      saveProjectSilent:       (filePath: string, data: ProjectSaveData) => Promise<boolean>
      openSpriteDialog:        () => Promise<string | null>
      openScenarioDialog:      () => Promise<string | null>
      openCharacterDialog:     () => Promise<string | null>
      getImageDataUrl:         (filePath: string) => Promise<string | null>
      openBackgroundDialog:    () => Promise<string | null>
      openAudioDialog:         () => Promise<string | null>
      openFontDialog:          () => Promise<string | null>
      onRequestViewportBounds: (cb: () => void) => void
      onAutoSaveRequest:       (cb: (filePath: string) => void) => void
      getAppResourceUsage:     () => Promise<AppResourceUsage>
    }
  }
}
