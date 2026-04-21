import { useReducer, useEffect, useRef } from 'react'
import type { EngineEvent, EntitySelected, ScenarioLoaded, CharacterLoaded } from '../../../shared-types/types'

export interface Entity {
  id: number
}

export interface SelectedEntity {
  id:       number
  name:     string
  position: [number, number, number]
  rotation: [number, number, number, number]
  scale:    [number, number, number]
}

export interface LogEntry {
  id:      number
  text:    string
  isError: boolean
}

export interface ScenarioEntry {
  id:   number
  path: string
}

export type CharacterEntry = ScenarioEntry

export interface WorldConfig {
  worldWidth:   number
  worldHeight:  number
  gridVisible:  boolean
  gridCellSize: number
}

const DEFAULT_WORLD_CONFIG: WorldConfig = {
  worldWidth:   100,
  worldHeight:  50,
  gridVisible:  true,
  gridCellSize: 1,
}

interface EngineState {
  engineReady:        boolean
  engineError:        string | null
  log:                LogEntry[]
  entities:           Entity[]
  selectedEntity:     SelectedEntity | null
  scenarioEntities:   ScenarioEntry[]
  characterEntities:  CharacterEntry[]
  worldConfig:        WorldConfig
}

type EngineAction =
  | { type: 'SET_READY' }
  | { type: 'SET_ERROR'; payload: string }
  | { type: 'ADD_LOG'; payload: LogEntry }
  | { type: 'ADD_ENTITY'; payload: number }
  | { type: 'SELECT_ENTITY'; payload: SelectedEntity }
  | { type: 'DESELECT_ENTITY' }
  | { type: 'ENGINE_STOPPED'; payload: number | undefined }
  | { type: 'CLEAR_ENTITIES' }
  | { type: 'RESET_ENGINE' }
  | { type: 'ADD_SCENARIO'; payload: ScenarioEntry }
  | { type: 'REMOVE_SCENARIO'; payload: number }
  | { type: 'ADD_CHARACTER'; payload: CharacterEntry }
  | { type: 'REMOVE_CHARACTER'; payload: number }
  | { type: 'SET_WORLD_CONFIG'; payload: Partial<WorldConfig> }

const initialState: EngineState = {
  engineReady:       false,
  engineError:       null,
  log:               [],
  entities:          [],
  selectedEntity:    null,
  scenarioEntities:  [],
  characterEntities: [],
  worldConfig:       DEFAULT_WORLD_CONFIG,
}

function engineReducer(state: EngineState, action: EngineAction): EngineState {
  switch (action.type) {
    case 'SET_READY':
      return { ...state, engineReady: true, engineError: null }
    case 'SET_ERROR':
      return { ...state, engineError: action.payload }
    case 'ADD_LOG':
      return { ...state, log: [...state.log.slice(-199), action.payload] }
    case 'ADD_ENTITY':
      if (state.entities.some((e) => e.id === action.payload)) return state
      return { ...state, entities: [...state.entities, { id: action.payload }] }
    case 'SELECT_ENTITY':
      return { ...state, selectedEntity: action.payload }
    case 'DESELECT_ENTITY':
      return { ...state, selectedEntity: null }
    case 'ENGINE_STOPPED': {
      const code = action.payload
      const error = (code !== 0 && code != null)
        ? `El motor terminó inesperadamente (código ${code}).`
        : null
      return { ...state, engineReady: false, ...(error ? { engineError: error } : {}) }
    }
    case 'CLEAR_ENTITIES':
      return { ...state, entities: [] }
    case 'RESET_ENGINE':
      return { ...state, engineReady: false, engineError: null, entities: [] }
    case 'ADD_SCENARIO':
      return { ...state, scenarioEntities: [...state.scenarioEntities, action.payload] }
    case 'REMOVE_SCENARIO':
      return { ...state, scenarioEntities: state.scenarioEntities.filter((s) => s.id !== action.payload) }
    case 'ADD_CHARACTER':
      return { ...state, characterEntities: [...state.characterEntities, action.payload] }
    case 'REMOVE_CHARACTER':
      return { ...state, characterEntities: state.characterEntities.filter((c) => c.id !== action.payload) }
    case 'SET_WORLD_CONFIG':
      return { ...state, worldConfig: { ...state.worldConfig, ...action.payload } }
    default:
      return state
  }
}

export function useEngine(
  viewportRef: React.RefObject<HTMLDivElement | null>,
  projectType?: string,
) {
  const [state, dispatch] = useReducer(engineReducer, initialState)
  const readyTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const logIdRef   = useRef(0)

  const addLog = (text: string, isError = false) => {
    logIdRef.current += 1
    dispatch({ type: 'ADD_LOG', payload: { id: logIdRef.current, text, isError } })
  }

  const reportBounds = () => {
    if (!viewportRef.current) return
    const rect = viewportRef.current.getBoundingClientRect()
    const dpr  = window.devicePixelRatio ?? 1
    window.electronAPI.sendViewportBounds({
      x:      rect.x      * dpr,
      y:      rect.y      * dpr,
      width:  rect.width  * dpr,
      height: rect.height * dpr,
    })
  }

  const send = (cmd: object) => window.engine.send(cmd as never)

  const loadModel = (path: string) => {
    dispatch({ type: 'CLEAR_ENTITIES' })
    send({ cmd: 'load_model', path })
  }

  const retryEngine = () => {
    dispatch({ type: 'RESET_ENGINE' })
    addLog('[retry] Reiniciando motor…')
    reportBounds()
  }

  // Reportar bounds del viewport al proceso principal
  useEffect(() => {
    reportBounds()
    const observer = new ResizeObserver(reportBounds)
    if (viewportRef.current) observer.observe(viewportRef.current)
    window.electronAPI.onRequestViewportBounds(reportBounds)
    return () => observer.disconnect()
  }, [])

  // Enviar estado de Ctrl al motor (la ventana embebida no recibe teclado directo)
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Control') window.engine.send({ cmd: 'set_ctrl_held', held: true } as never)
    }
    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key === 'Control') window.engine.send({ cmd: 'set_ctrl_held', held: false } as never)
    }
    window.addEventListener('keydown', onKeyDown)
    window.addEventListener('keyup',   onKeyUp)
    return () => {
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('keyup',   onKeyUp)
    }
  }, [])

  // Timeout de 5 s esperando el evento "ready"
  useEffect(() => {
    readyTimer.current = setTimeout(() => {
      dispatch({ type: 'SET_ERROR', payload: 'El motor no respondió en 5 segundos. Puede que el binario no exista o haya fallado al iniciar.' })
      addLog('[timeout] Motor no respondió en 5s', true)
    }, 5000)
    return () => {
      if (readyTimer.current) clearTimeout(readyTimer.current)
    }
  }, [])

  // Suscribirse a eventos del motor
  useEffect(() => {
    window.engine.on((event: EngineEvent) => {
      addLog(JSON.stringify(event), event.event === 'error')

      if (event.event === 'ready') {
        dispatch({ type: 'SET_READY' })
        if (readyTimer.current) clearTimeout(readyTimer.current)
        if (projectType) {
          window.engine.send({ cmd: 'set_scene', scene: projectType } as never)
        }
      }
      if (event.event === 'model_loaded') {
        const id = (event as { id?: number }).id ?? -1
        dispatch({ type: 'ADD_ENTITY', payload: id })
      }
      if (event.event === 'entity_selected') {
        const e = event as unknown as EntitySelected
        dispatch({ type: 'SELECT_ENTITY', payload: { id: e.id, name: e.name, position: e.position, rotation: e.rotation, scale: e.scale } })
      }
      if (event.event === 'entity_deselected') {
        dispatch({ type: 'DESELECT_ENTITY' })
      }
      if (event.event === 'scenario_loaded') {
        const e = event as unknown as ScenarioLoaded
        dispatch({ type: 'ADD_SCENARIO', payload: { id: e.id, path: e.path } })
      }
      if (event.event === 'character_loaded') {
        const e = event as unknown as CharacterLoaded
        dispatch({ type: 'ADD_CHARACTER', payload: { id: e.id, path: e.path } })
      }
      if (event.event === 'stopped') {
        dispatch({ type: 'ENGINE_STOPPED', payload: (event as { code?: number }).code })
      }
      if (event.event === 'error') {
        dispatch({ type: 'SET_ERROR', payload: (event as { message?: string }).message ?? 'Error desconocido' })
      }
    })
    return () => { window.engine.off() }
  }, [])

  const removeScenario = (id: number) => {
    send({ cmd: 'remove_entity', id })
    dispatch({ type: 'REMOVE_SCENARIO', payload: id })
  }

  const duplicateScenario = (id: number) => {
    send({ cmd: 'duplicate_scenario', id })
  }

  const removeCharacter = (id: number) => {
    send({ cmd: 'remove_entity', id })
    dispatch({ type: 'REMOVE_CHARACTER', payload: id })
  }

  const duplicateCharacter = (id: number) => {
    send({ cmd: 'duplicate_character', id })
  }

  const setWorldSize = (width: number, height: number) => {
    dispatch({ type: 'SET_WORLD_CONFIG', payload: { worldWidth: width, worldHeight: height } })
    send({ cmd: 'set_world_size', width, height })
  }

  const setGridVisible = (visible: boolean) => {
    dispatch({ type: 'SET_WORLD_CONFIG', payload: { gridVisible: visible } })
    send({ cmd: 'set_grid_visible', visible })
  }

  const setGridCellSize = (size: number) => {
    dispatch({ type: 'SET_WORLD_CONFIG', payload: { gridCellSize: size } })
    send({ cmd: 'set_grid_cell_size', size })
  }

  return {
    engineReady:        state.engineReady,
    engineError:        state.engineError,
    log:                state.log,
    entities:           state.entities,
    selectedEntity:     state.selectedEntity,
    scenarioEntities:   state.scenarioEntities,
    characterEntities:  state.characterEntities,
    worldConfig:        state.worldConfig,
    send,
    loadModel,
    reportBounds,
    retryEngine,
    removeScenario,
    duplicateScenario,
    removeCharacter,
    duplicateCharacter,
    setWorldSize,
    setGridVisible,
    setGridCellSize,
  }
}
