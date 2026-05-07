// Tipos compartidos entre main process, preload y renderer.

export type ProjectType = '2D' | '3D'

export type GameStyle =
  | 'first-person'
  | 'second-person'
  | 'third-person'
  | 'top-down'
  | 'side-scroller'
  | 'isometric'

export interface ProjectConfig {
  type:      ProjectType
  gameStyle: GameStyle
}

// ── Estado completo guardado en disco ───────────────────────────────────────

export interface SavedEntity {
  id:               number
  /** Nombre visible de la entidad en el editor. */
  name?:            string
  path:             string
  kind:             'model' | 'scenario' | 'character' | 'collider' | 'execution_area'
  position:         [number, number, number]
  rotation:         [number, number, number, number]
  scale:            [number, number, number]
  physics_enabled?: boolean
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

export interface SavedWorldConfig {
  worldWidth:   number
  worldHeight:  number
  gridVisible:  boolean
  gridCellSize: number
  gravity?:     number
}

export interface SavedScene {
  id:             number
  name:           string
  world:          SavedWorldConfig
  backgroundPath: string | null
  entities:       SavedEntity[]
  playerTransform:{ position: [number, number, number]; scale: [number, number, number] } | null
  camera2d:       { x: number; y: number; halfH: number } | null
  sprites?:       Array<{ name: string; path: string }>
}

export interface ProjectSaveData {
  version:         number
  type:            ProjectType
  gameStyle:       GameStyle
  /** Escenas del proyecto. Si no existe, se asume formato legacy de escena única. */
  scenes?:         SavedScene[]
  activeSceneId?:  number
  world:           SavedWorldConfig
  backgroundPath:  string | null
  entities:        SavedEntity[]
  playerTransform: { position: [number, number, number]; scale: [number, number, number] } | null
  camera2d:        { x: number; y: number; halfH: number } | null
  savedAt:         string   // ISO timestamp
  /** Sprites precargados en el proyecto (nombre -> ruta relativa). */
  sprites?:        Array<{ name: string; path: string }>
  /** Blueprints creados en el proyecto. */
  blueprints?:     BluePrintEntry[]
}

export interface OpenProjectResult {
  project:  ProjectSaveData
  filePath: string
}

export interface EngineCommand {
  cmd: 'ping' | 'shutdown' | 'set_clear_color' | 'resize' | 'set_bounds' | 'load_model' | 'set_transform' | 'set_entity_name' | 'set_scene' | 'load_scenario' | 'set_scenario_scale' | 'duplicate_scenario' | 'load_character' | 'set_character_scale' | 'duplicate_character' | 'remove_entity' | 'set_world_size' | 'set_grid_visible' | 'set_grid_cell_size' | 'set_ctrl_held' | 'set_physics' | 'set_active_tool' | 'create_collider_from_points' | 'create_execution_area_from_points' | 'play_animation_frame' | 'restore_animation_frame' | 'set_pivot_edit_mode' | 'cancel_pivot_edit_mode' | 'set_logical_area_mode' | 'cancel_logical_area_mode' | 'play_audio' | 'stop_audio' | 'set_animation' | 'remove_animation' | 'set_default_animation' | 'play_animation' | 'stop_animation' | 'load_script' | 'unload_script' | 'load_sprite' | 'remove_sprite' | 'get_sprites_list' | 'set_preview_playing' | 'run_control_script' | 'undo' | 'clear_background' | 'reload_asset' | 'set_locale'
  [key: string]: unknown
}

export interface EngineEvent {
  event: 'ready' | 'pong' | 'error' | 'model_loaded' | 'stopped' | 'entity_selected' | 'entity_deselected' | 'entity_hovered' | 'entity_unhovered' | 'scenario_loaded' | 'character_loaded' | 'player_ready' | 'camera_2d_updated' | 'background_loaded' | 'drawing_progress' | 'collider_created' | 'execution_area_created' | 'tool_cancelled' | 'pivot_selected' | 'physics_changed' | 'sprite_loaded' | 'sprite_removed' | 'sprites_list' | 'control_input_detected' | 'debug_metrics' | 'trigger_entered' | 'trigger_exited' | 'entity_removed' | 'quick_build_move' | 'quick_build_click'
  [key: string]: unknown
}

export interface DebugMetrics {
  fps:            number
  frame_time_ms:  number
  draw_calls:     number
  physics_bodies: number
}

export interface PlayerReady {
  event:    'player_ready'
  id:       number
  position: [number, number, number]
  scale:    [number, number, number]
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
}

export interface CharacterLoaded {
  event: 'character_loaded'
  id:    number
  path:  string
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

export interface ControlInputDetected {
  event: 'control_input_detected'
  device: 'keyboard_mouse' | 'gamepad'
  control_key: string
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

export interface SpriteInfo {
  path:   string
  name:   string
  width:  number
  height: number
}

export type BluePrintCategory = 'personaje' | 'entorno' | 'objetos'

export interface BluePrintEntry {
  /** Identificador único generado al crear el blueprint. */
  id:               string
  name:             string
  category:         BluePrintCategory
  kind:             'scenario' | 'character' | 'model' | 'collider' | 'execution_area'
  path:             string
  scale:            [number, number, number]
  physics_enabled?: boolean
  physics_type?:    string
  animations?:      SavedAnimation[]
  scripts?:         SavedScript[]
  control_bindings?: SavedControlBindings
}

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
      off:  () => void
    }
    electronAPI: {
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
      onRequestViewportBounds: (cb: () => void) => void
    }
  }
}
