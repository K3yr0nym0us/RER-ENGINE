use super::*;

/// Entrada de la consola inferior (espejo de `LogEntry` en React).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub text: String,
    pub is_error: bool,
}

/// Eventos que no merecen una línea de consola (ruido de runtime/carga).
fn is_quiet_engine_event(name: &str) -> bool {
    matches!(
        name,
        "debug_metrics"
            | "autosave_tick"
            | "save_snapshot_ready"
            | "preview_playing_changed"
            | "animation_finished"
            | "animation_logical_resolved"
            | "entity_selected"
            | "entity_deselected"
            | "entity_hovered"
            | "entity_unhovered"
            | "entity_model_replaced"
            | "physics_changed"
            | "quick_build_move"
            | "quick_build_click"
            | "camera_2d_updated"
            | "multi_select_changed"
            | "multi_selection_transformed"
            | "character_loaded"
            | "player_ready"
            | "sprite_loaded"
            | "background_loaded"
            | "sound_loaded"
            | "font_loaded"
            | "hud_image_loaded"
            | "background_asset_loaded"
            | "scenario_loaded"
            | "scene_imported"
            | "model_asset_preload_started"
            | "model_asset_importing"
            | "model_asset_imported"
            | "model_asset_loaded"
            | "model_asset_load_failed"
            | "model_clips_ready"
            | "plane_tool_ready"
            | "tool_cancelled"
            | "atlas_exhausted"
            | "play_character_view_changed"
            | "graphics_texture_tier_changed"
            | "texture_detail_distance_changed"
            | "reflection_tier_changed"
            | "reflection_tier_effective"
            | "reflection_raytracing_changed"
            | "reflection_probes_changed"
            | "reflection_debug_view_changed"
            | "ssr_debug_mode_changed"
            | "shadow_tier_changed"
            | "msaa_tier_changed"
            | "taa_changed"
            | "default_scene_name_ready"
            | "editor_scenes_list"
            | "entity_bones_list"
            | "entity_sockets_list"
            | "entity_sockets_changed"
            | "socket_bone_picked"
            | "entity_socket_attached"
            | "entity_attachment_restored"
            | "entity_properties_open"
    ) || name.ends_with("_list")
        || name.ends_with("_loaded")
        || name.ends_with("_changed")
        || name.starts_with("player_ui_")
        || name.starts_with("project_loaded")
        || name.starts_with("project_load")
        || name.starts_with("quick_build_")
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
        if name.is_empty() || is_quiet_engine_event(name) {
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
