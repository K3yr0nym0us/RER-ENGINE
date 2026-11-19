import type {
  ENGINE_COMMANDS_2D_ONLY,
  ENGINE_COMMANDS_3D_ONLY,
  ENGINE_COMMANDS_SHARED,
} from './engineCommandCatalog'

export type EngineCommandNameShared = (typeof ENGINE_COMMANDS_SHARED)[number]
export type EngineCommandName2DOnly = (typeof ENGINE_COMMANDS_2D_ONLY)[number]
export type EngineCommandName3DOnly = (typeof ENGINE_COMMANDS_3D_ONLY)[number]

/** Comandos válidos con motor 2D activo (compartidos + Only2d). */
export type EngineCommandName2D = EngineCommandNameShared | EngineCommandName2DOnly

/** Comandos válidos con motor 3D activo (compartidos + Only3d). */
export type EngineCommandName3D = EngineCommandNameShared | EngineCommandName3DOnly

/** Payload IPC hacia el motor 2D. */
export type EngineCommand2D = {
  cmd: EngineCommandName2D
} & Record<string, unknown>

/** Payload IPC hacia el motor 3D. */
export type EngineCommand3D = {
  cmd: EngineCommandName3D
} & Record<string, unknown>

export interface EngineEventListener {
  (event: import('./types').EngineEvent): void
}

export interface EngineApiBase {
  on: (cb: EngineEventListener) => void
  off: (cb?: EngineEventListener) => void
}

export interface Engine2dApi extends EngineApiBase {
  send: (cmd: EngineCommand2D) => void
}

export interface Engine3dApi extends EngineApiBase {
  send: (cmd: EngineCommand3D) => void
}

