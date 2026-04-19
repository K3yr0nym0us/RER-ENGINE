// Tipos compartidos entre main process, preload y renderer.

export interface EngineCommand {
  cmd: 'ping' | 'shutdown' | 'set_clear_color' | 'resize' | 'load_model' | 'set_transform'
  [key: string]: unknown
}

export interface EngineEvent {
  event: 'ready' | 'pong' | 'error' | 'model_loaded' | 'stopped' | 'entity_selected' | 'entity_deselected'
  [key: string]: unknown
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
    }
    electronAPI: {
      sendViewportBounds: (bounds: ViewportBounds) => void
      openModelDialog:    () => Promise<string | null>
      onRequestViewportBounds: (cb: () => void) => void
    }
  }
}
