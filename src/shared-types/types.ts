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
  path:             string
  kind:             'model' | 'scenario' | 'character' | 'collider'
  position:         [number, number, number]
  rotation:         [number, number, number, number]
  scale:            [number, number, number]
  physics_enabled?: boolean
  physics_type?:    string
/** Puntos en espacio de mundo para entidades de tipo 'collider'. */
  points?:          [[number,number],[number,number],[number,number],[number,number]]
  /** Animaciones asociadas a esta entidad. */
  animations?:      SavedAnimation[]
  /** Scripts Lua adjuntos a esta entidad. */
  scripts?:         SavedScript[]
}

export interface SavedAnimation {
  name:       string
  fps:        number
  loop:       boolean
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
  }[]
  /** Scripts Lua asociados a esta animación. */
  scripts?: SavedScript[]
}

export interface SavedScript {
  /** Nombre identificador del script (elegido por el usuario). */
  name:   string
  /** Código fuente Lua completo. */
  source: string
}

export interface SavedWorldConfig {
  worldWidth:   number
  worldHeight:  number
  gridVisible:  boolean
  gridCellSize: number
}

export interface ProjectSaveData {
  version:         number
  type:            ProjectType
  gameStyle:       GameStyle
  world:           SavedWorldConfig
  backgroundPath:  string | null
  entities:        SavedEntity[]
  playerTransform: { position: [number, number, number]; scale: [number, number, number] } | null
  camera2d:        { x: number; y: number; halfH: number } | null
  savedAt:         string   // ISO timestamp
}

export interface EngineCommand {
  cmd: 'ping' | 'shutdown' | 'set_clear_color' | 'resize' | 'set_bounds' | 'load_model' | 'set_transform' | 'set_scene' | 'load_scenario' | 'set_scenario_scale' | 'duplicate_scenario' | 'load_character' | 'set_character_scale' | 'duplicate_character' | 'remove_entity' | 'set_world_size' | 'set_grid_visible' | 'set_grid_cell_size' | 'set_ctrl_held' | 'set_physics' | 'set_active_tool' | 'create_collider_from_points' | 'play_animation_frame' | 'restore_animation_frame' | 'set_pivot_edit_mode' | 'cancel_pivot_edit_mode' | 'set_logical_area_mode' | 'cancel_logical_area_mode' | 'play_audio' | 'stop_audio' | 'set_animation' | 'play_animation' | 'stop_animation' | 'load_script' | 'unload_script'
  [key: string]: unknown
}

export interface EngineEvent {
  event: 'ready' | 'pong' | 'error' | 'model_loaded' | 'stopped' | 'entity_selected' | 'entity_deselected' | 'entity_hovered' | 'entity_unhovered' | 'scenario_loaded' | 'character_loaded' | 'player_ready' | 'camera_2d_updated' | 'background_loaded' | 'drawing_progress' | 'collider_created' | 'tool_cancelled' | 'pivot_selected'
  [key: string]: unknown
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
      openProjectDialog:       () => Promise<ProjectSaveData | null>
      saveProject:             (data: ProjectSaveData) => Promise<boolean>
      saveProjectSilent:       (filePath: string, data: ProjectSaveData) => Promise<boolean>
      openScenarioDialog:      () => Promise<string | null>
      openCharacterDialog:     () => Promise<string | null>
      openBackgroundDialog:    () => Promise<string | null>
      openAudioDialog:         () => Promise<string | null>
      onRequestViewportBounds: (cb: () => void) => void
    }
  }
}
