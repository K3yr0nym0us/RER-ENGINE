use super::*;

use rer_engine_core::logging::LogLayer;

/// Entrada de la consola inferior (espejo de `LogEntry` en React).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub text: String,
    pub is_error: bool,
}

/// Errores que deben verse siempre, aunque su capa esté apagada.
fn is_engine_error_event(name: &str) -> bool {
    matches!(
        name,
        "error" | "world_gen_error" | "model_asset_load_failed"
    )
}

/// Capa a la que pertenece un evento del motor (consola inferior).
fn engine_event_layer(name: &str) -> LogLayer {
    // Assets/lists primero: son los eventos más ruidosos y van apagados por defecto.
    if name.ends_with("_loaded")
        || name.ends_with("_list")
        || name.starts_with("model_asset_")
        || name.starts_with("atlas_")
    {
        return LogLayer::Assets;
    }
    match name {
        "ready" | "stopped" | "error" | "pong" => LogLayer::Engine,
        "load_progress" => LogLayer::Loading,
        "world_gen_progress" | "world_gen_chunk_ready" => LogLayer::Runtime,
        "editor_scenes_updated"
        | "editor_scene_created"
        | "editor_scene_switched"
        | "editor_scene_switch_blocked"
        | "entities_merged"
        | "entities_attachments_restored"
        | "collider_created"
        | "execution_area_created" => LogLayer::Editor,
        "trigger_entered" | "trigger_exited" | "physics_changed" => LogLayer::Physics,
        n if n.starts_with("world_gen_") => LogLayer::Worldgen,
        n if n.starts_with("physics_") || n.starts_with("projectile_") => LogLayer::Physics,
        // Ajustes de render y eventos internos: ruido de depuración.
        n if n.ends_with("_changed")
            || n.ends_with("_effective")
            || n.ends_with("_updated")
            || n.starts_with("default_scene_name")
            || n.starts_with("quick_build_")
            || n == "editor_scenes_list"
            || n == "entity_properties_open" =>
        {
            LogLayer::Runtime
        }
        n if n.starts_with("camera_2d")
            || n.starts_with("scenario_")
            || n.starts_with("drawing_")
            || n.starts_with("sprite_")
            || n.starts_with("sound_")
            || n.starts_with("font_")
            || n.starts_with("background_") =>
        {
            LogLayer::World2d
        }
        n if n.starts_with("model_")
            || n.starts_with("mesh_")
            || n.starts_with("light")
            || n.starts_with("sun")
            || n.starts_with("reflection")
            || n.starts_with("ssr_")
            || n.starts_with("shadow_")
            || n.starts_with("msaa_")
            || n.starts_with("ssaa_")
            || n.starts_with("taa_")
            || n.starts_with("graphics_")
            || n.starts_with("texture_")
            || n.starts_with("world_")
            || n.starts_with("sky_") =>
        {
            LogLayer::World3d
        }
        _ => LogLayer::Runtime,
    }
}

fn scene_list(scenes: &[Value]) -> String {
    if scenes.is_empty() {
        return "(vacía)".to_string();
    }
    scenes
        .iter()
        .filter_map(|scene| scene.get("name").and_then(Value::as_str))
        .map(|name| format!("«{name}»"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn scene_name(scenes: &[Value], id: u64) -> String {
    scenes
        .iter()
        .find(|scene| scene.get("id").and_then(Value::as_u64) == Some(id))
        .and_then(|scene| scene.get("name").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| format!("#{id}"))
}

impl EditorApp {
    /// Añade una línea a la consola inferior (máx. 200, como React).
    pub fn push_log(&mut self, text: impl Into<String>, is_error: bool) {
        let text = text.into();
        if text.trim().is_empty() {
            return;
        }
        self.log.push(LogEntry { text, is_error });
        if self.log.len() > MAX_LOG_ENTRIES {
            let excess = self.log.len() - MAX_LOG_ENTRIES;
            self.log.drain(0..excess);
        }
    }

    /// Traduce un evento del motor a una línea legible de la consola.
    pub fn log_engine_event(&mut self, event: &Value) {
        let Some(name) = event.get("event").and_then(Value::as_str) else {
            return;
        };
        if name.is_empty() {
            return;
        }
        // Los errores se muestran siempre; el resto depende de su capa.
        if !is_engine_error_event(name) && !self.log_layers.contains(engine_event_layer(name)) {
            return;
        }

        match name {
            "ready" => {
                let gravity = event.get("gravity").and_then(Value::as_f64).unwrap_or(0.0);
                self.push_log(format!("[Motor] listo (gravedad {gravity:.2})"), false);
            }
            "stopped" => self.push_log("[Motor] detenido", false),
            "error" => {
                let message = event
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("error del motor");
                self.push_log(format!("[Error] {message}"), true);
            }
            "load_progress" => {
                let message = event
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if message.is_empty() {
                    return;
                }
                let timing = match (
                    event.get("step_ms").and_then(Value::as_u64),
                    event.get("total_ms").and_then(Value::as_u64),
                ) {
                    (Some(step), Some(total)) => format!(" (+{step} ms, total {total} ms)"),
                    _ => String::new(),
                };
                self.push_log(format!("[Carga] {message}{timing}"), false);
            }
            "world_gen_error" => {
                let message = event
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("error del generador");
                self.push_log(format!("[Worldgen] {message}"), true);
            }
            "world_gen_completed" => {
                let chunks = event.get("chunks").and_then(Value::as_u64).unwrap_or(0);
                self.push_log(
                    format!("[Worldgen] generación completa ({chunks} chunks)"),
                    false,
                );
            }
            "world_gen_status" => {
                let state = event.get("state").and_then(Value::as_str).unwrap_or("");
                if !state.is_empty() {
                    self.push_log(format!("[Worldgen] estado: {state}"), false);
                }
            }
            "world_gen_scatter_done" => {
                let blueprint = event
                    .get("blueprint_id")
                    .and_then(Value::as_str)
                    .unwrap_or("?");
                let count = event.get("count").and_then(Value::as_u64).unwrap_or(0);
                self.push_log(
                    format!("[Worldgen] scatter «{blueprint}»: {count} instancia(s)"),
                    false,
                );
            }
            "entities_merged" => {
                let parent = event.get("parent_id").and_then(Value::as_u64).unwrap_or(0);
                let count = event
                    .get("child_ids")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0);
                self.push_log(
                    format!("[Fusión] {count} entidad(es) vinculadas al padre {parent}"),
                    false,
                );
            }
            "entities_attachments_restored" => {
                let count = event.get("count").and_then(Value::as_u64).unwrap_or(0);
                if count > 0 {
                    self.push_log(
                        format!("[Fusión] {count} vínculo(s) restaurados al cargar escena"),
                        false,
                    );
                }
            }
            "trigger_entered" => {
                let trigger = event.get("trigger_id").and_then(Value::as_u64).unwrap_or(0);
                let actor = event.get("actor_id").and_then(Value::as_u64).unwrap_or(0);
                self.push_log(
                    format!("[Trigger] entidad {actor} entró en {trigger}"),
                    false,
                );
            }
            "collider_created" => self.push_log("[Editor] colisionador creado", false),
            "execution_area_created" => {
                self.push_log("[Editor] área de ejecución creada", false);
            }
            "editor_scenes_updated" => {
                let reason = event
                    .get("update_reason")
                    .and_then(Value::as_str)
                    .unwrap_or("sync");
                let scenes = event
                    .get("scenes")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let active_id = event.get("active_scene_id").and_then(Value::as_u64);
                let active_name = active_id
                    .map(|id| scene_name(scenes, id))
                    .unwrap_or_else(|| "?".to_string());
                let list = scene_list(scenes);
                let count = scenes.len();
                let line = match reason {
                    "boot" => Some(format!(
                        "[Escenas] Registro inicializado — activa «{active_name}» ({count} escena/s: {list})"
                    )),
                    "project_saved" => Some(format!(
                        "[Escenas] Registro alineado tras guardar — activa «{active_name}» ({count} escena/s: {list})"
                    )),
                    "project_loaded" => Some(format!(
                        "[Escenas] Registro cargado — activa «{active_name}» ({count} escena/s: {list})"
                    )),
                    "scene_deleted" => Some(format!(
                        "[Escenas] Escena eliminada — activa «{active_name}» ({count} escena/s: {list})"
                    )),
                    _ => None,
                };
                if let Some(line) = line {
                    self.push_log(line, false);
                }
            }
            "editor_scene_created" => {
                let id = event.get("id").and_then(Value::as_u64).unwrap_or(0);
                let name = event.get("name").and_then(Value::as_str).unwrap_or("");
                let list = scene_list(
                    event
                        .get("scenes")
                        .and_then(Value::as_array)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]),
                );
                self.push_log(
                    format!("[Escenas] Creada «{name}» (id={id}); lista: {list}"),
                    false,
                );
            }
            "editor_scene_switched" => {
                let active_id = event
                    .get("active_scene_id")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let scenes = event
                    .get("scenes")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let active_name = scene_name(scenes, active_id);
                self.push_log(
                    format!("[Escenas] Escena activa {active_name} (id={active_id})"),
                    false,
                );
            }
            "editor_scene_switch_blocked" => {
                let reason = event
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let active = event
                    .get("active_scene_id")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let target = event
                    .get("target_scene_id")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let reason_label = if reason == "unsaved_changes" {
                    "guarda el proyecto antes de cambiar de escena"
                } else {
                    reason
                };
                self.push_log(
                    format!("[Escenas] Cambio bloqueado ({active} → {target}): {reason_label}"),
                    false,
                );
            }
            _ => {
                let json = event.to_string();
                let preview = if json.chars().count() > 240 {
                    format!("{}…", json.chars().take(240).collect::<String>())
                } else {
                    json
                };
                self.push_log(format!("[{name}] {preview}"), false);
            }
        }
    }
}
