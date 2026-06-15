import type { EngineCommand, ProjectType } from '../../shared-types/types'
import { isCommandAllowedForMotor } from './engineCommandCatalog'

export type EngineCmdSend = (cmd: EngineCommand) => void

export interface EngineIpcRegistryDeps {
  getProjectType: () => ProjectType | null
  sendToEngine: EngineCmdSend
  setLocale: (locale: 'en' | 'es') => void
  /** Registro de file watcher para PNG 2D (escenario / personaje / fondo). */
  watchPngAsset: (path: string) => void
}

/** Side-effects y validación de `engine:cmd` según el motor activo. */
export function handleEngineCommand(cmd: EngineCommand, deps: EngineIpcRegistryDeps): void {
  const cmdName = typeof cmd.cmd === 'string' ? cmd.cmd : ''
  if (!cmdName) {
    console.warn('[ipc] engine:cmd sin campo cmd')
    return
  }

  if (cmdName === 'set_locale') {
    const next = String((cmd as Record<string, unknown>).locale ?? 'en').toLowerCase() === 'es' ? 'es' : 'en'
    deps.setLocale(next)
    console.log(`[i18n] IPC renderer -> main set_locale: ${next}`)
  }

  const projectType = deps.getProjectType()
  if (!isCommandAllowedForMotor(cmdName, projectType)) {
    const motor = projectType ?? '(sin motor)'
    console.warn(`[ipc] comando "${cmdName}" rechazado para motor ${motor}`)
    return
  }

  if (projectType === '2D') {
    applyEngine2dMainSideEffects(cmd, deps)
  }

  deps.sendToEngine(cmd)
}

function applyEngine2dMainSideEffects(cmd: EngineCommand, deps: EngineIpcRegistryDeps): void {
  const c = cmd as Record<string, unknown>
  if (
    typeof c.path === 'string'
    && (c.cmd === 'load_character' || c.cmd === 'load_scenario' || c.cmd === 'load_background')
  ) {
    deps.watchPngAsset(c.path)
  }
}
