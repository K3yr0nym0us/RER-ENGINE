// Tipos compartidos entre main process, preload y renderer.

export type ProjectType = '2D' | '3D' | 'scratch'

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

export interface EngineCommand {
  cmd: 'ping' | 'shutdown' | 'set_clear_color' | 'resize' | 'load_model' | 'set_transform' | 'set_scene' | 'load_scenario' | 'set_scenario_scale' | 'duplicate_scenario' | 'remove_entity' | 'set_world_size' | 'set_grid_visible' | 'set_grid_cell_size' | 'set_ctrl_held'
  [key: string]: unknown
}

export interface EngineEvent {
  event: 'ready' | 'pong' | 'error' | 'model_loaded' | 'stopped' | 'entity_selected' | 'entity_deselected' | 'scenario_loaded'
  [key: string]: unknown
}

export interface ScenarioLoaded {
  event: 'scenario_loaded'
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
      openProjectDialog:       () => Promise<ProjectConfig | null>
      openScenarioDialog:      () => Promise<string | null>
      onRequestViewportBounds: (cb: () => void) => void
    }
  }
}
