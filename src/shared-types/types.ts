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
  id:       number
  path:     string
  kind:     'model' | 'scenario' | 'character'
  position: [number, number, number]
  rotation: [number, number, number, number]
  scale:    [number, number, number]
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
  cmd: 'ping' | 'shutdown' | 'set_clear_color' | 'resize' | 'set_bounds' | 'load_model' | 'set_transform' | 'set_scene' | 'load_scenario' | 'set_scenario_scale' | 'duplicate_scenario' | 'load_character' | 'set_character_scale' | 'duplicate_character' | 'remove_entity' | 'set_world_size' | 'set_grid_visible' | 'set_grid_cell_size' | 'set_ctrl_held'
  [key: string]: unknown
}

export interface EngineEvent {
  event: 'ready' | 'pong' | 'error' | 'model_loaded' | 'stopped' | 'entity_selected' | 'entity_deselected' | 'scenario_loaded' | 'character_loaded' | 'player_ready' | 'camera_2d_updated'
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

export interface EntitySelected {
  event:    'entity_selected'
  id:       number
  name:     string
  position: [number, number, number]
  rotation: [number, number, number, number]  // quaternion xyzw
  scale:    [number, number, number]
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
      onRequestViewportBounds: (cb: () => void) => void
    }
  }
}
