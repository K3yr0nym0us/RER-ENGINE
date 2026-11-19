/**
 * Tipos del SHELL Electron (launcher), NO del editor.
 *
 * El editor real es Rust (`rer_engine_editor` + egui). Los tipos del motor —
 * entidades, escenas, comandos, eventos, HUD, blueprints, cámara, física— viven
 * en Rust (carpetas `ipc/` de `src/main/Engine/engine/src/`).
 *
 * NO agregar ni modificar tipos aquí como consecuencia de un cambio en Rust.
 * Este archivo solo debe crecer si cambia el launcher (selector de proyecto,
 * diálogo de `.save`, idioma) y el dato no puede vivir en Rust.
 * Ver `.cursor/rules/shared-types.mdc`.
 */

export type ProjectType = '2D' | '3D'

/**
 * El launcher solo elige el tipo de proyecto o entrega la ruta del `.save`.
 * Los estilos de cámara (`first-person`, `top-down`, …) los decide Rust:
 * desde el manifest al abrir un save, o con su default por tipo al crear uno.
 */
export interface EngineStartPayload {
  /** `2D`/`3D` para proyecto nuevo; `'save'` para que Rust lea el manifest del `.save`. */
  projectType: ProjectType | 'save'
  /** Ruta absoluta del `.save`; Electron solo la entrega, Rust la abre y la extrae. */
  save_path?:  string | false
}

/** Electron solo entrega la ruta del `.save`; Rust lo abre y decide el motor. */
export interface OpenProjectResult {
  filePath: string
}

declare global {
  interface Window {
    electronAPI: {
      setGameStyle:      (payload: EngineStartPayload) => void
      openProjectDialog: () => Promise<OpenProjectResult | null>
      setLocale:         (locale: 'en' | 'es') => void
    }
  }
}
