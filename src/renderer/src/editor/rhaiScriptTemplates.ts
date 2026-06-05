import type { Locale } from '../context/LanguageContext'

const ENTITY_SCRIPT_EN = `// Write your Rhai script here
// Available parameters:
//   entity  → entity snapshot { id, x, y, scale_x, scale_y, animations }
//   dt      → seconds since last frame (e.g. 0.016)
//
// ── Movement API ─────────────────────────────────────────────────────────────
//
// engine.move_entity(id, speed, dir_x, dir_y)
//   Physics movement (shape cast + collisions).
//
// engine.translate(id, dx, dy)
//   Direct translation, ignores collisions.
//
// Other: engine.play_animation(id, name)  engine.stop_animation(id)
//         engine.move_to(id, x, y)  engine.log(msg)
// ─────────────────────────────────────────────────────────────────────────────

fn on_start(entity) {
}

fn update(entity, dt) {
}

fn on_stop(entity) {
}
`

const ENTITY_SCRIPT_ES = `// Escribe tu script Rhai aquí
// Parámetros disponibles:
//   entity  → snapshot de la entidad { id, x, y, scale_x, scale_y, animations }
//   dt      → tiempo en segundos desde el último frame (ej: 0.016)
//
// ── Funciones de movimiento disponibles ──────────────────────────────────────
//
// engine.move_entity(id, speed, dir_x, dir_y)
//   Mueve la entidad a través del sistema de físicas (shape cast + colisiones).
//
// engine.translate(id, dx, dy)
//   Traslada la posición directamente, IGNORANDO colisiones.
//
// Otras API: engine.play_animation(id, name)  engine.stop_animation(id)
//            engine.move_to(id, x, y)  engine.log(msg)
// ─────────────────────────────────────────────────────────────────────────────

fn on_start(entity) {
}

fn update(entity, dt) {
}

fn on_stop(entity) {
}
`

const SCENE_SCRIPT_EN = `// Scene Rhai script (Level Blueprint)
// Available callbacks:
//   on_scene_start()  → when play starts in this scene
//   on_scene_tick(dt) → each frame while the scene is in play
//
// API: engine.log(msg)
// ─────────────────────────────────────────────────────────────────────────────

fn on_scene_start() {
    engine.log("Scene started");
}

fn on_scene_tick(dt) {
}
`

const SCENE_SCRIPT_ES = `// Script Rhai de escena (Level Blueprint)
// Callbacks disponibles:
//   on_scene_start()  → al iniciar play en esta escena
//   on_scene_tick(dt) → cada frame mientras la escena está en play
//
// API: engine.log(msg)
// ─────────────────────────────────────────────────────────────────────────────

fn on_scene_start() {
    engine.log("Escena iniciada");
}

fn on_scene_tick(dt) {
}
`

export function getDefaultEntityScript(locale: Locale): string {
  return locale === 'es' ? ENTITY_SCRIPT_ES : ENTITY_SCRIPT_EN
}

export function getDefaultSceneScript(locale: Locale): string {
  return locale === 'es' ? SCENE_SCRIPT_ES : SCENE_SCRIPT_EN
}
