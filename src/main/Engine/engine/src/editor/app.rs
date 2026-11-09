//! Estado raíz de la aplicación egui.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

use serde_json::{Value, json};
use winit::event_loop::EventLoopProxy;

use crate::editor::assets::AssetLists;
use crate::editor::cli::Cli;
use crate::editor::engine_host::UserEvent;
use crate::editor::project_state::ProjectState;
use crate::editor::save::SaveSuccess;
use crate::editor::scenes::SceneList;
use crate::editor::ui::sidebar_left::SidebarLeftState;
use crate::editor::ui::sidebar_right::SidebarRightState;
use crate::editor::world::WorldSettings;

/// Entidad seleccionada en el viewport 3D (la mantiene el motor).
#[derive(Debug, Clone, PartialEq)]
pub struct SelectedEntity {
    pub id: u64,
    pub name: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub physics_enabled: bool,
    pub physics_type: String,
}

impl SelectedEntity {
    fn from_event(event: &Value) -> Option<Self> {
        Some(Self {
            id: event.get("id")?.as_u64()?,
            name: event
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            position: array3(event.get("position")?)?,
            rotation: array4(event.get("rotation")?)?,
            scale: array3(event.get("scale")?)?,
            physics_enabled: event
                .get("physics_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            physics_type: event
                .get("physics_type")
                .and_then(Value::as_str)
                .unwrap_or("static")
                .to_string(),
        })
    }
}

/// Vista del jugador/cámara 3D confirmada por el motor (`play_character_view_changed`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayView {
    pub player_id: Option<u64>,
    pub eye: [f32; 3],
    pub yaw: f32,
    pub fov_y: f32,
    pub frustum_distance: f32,
}

impl PlayView {
    fn from_event(event: &Value) -> Option<Self> {
        let eye = array3(event.get("camera_eye_position")?)?;
        let player_id = event.get("player_id").and_then(Value::as_u64);
        Some(Self {
            player_id,
            eye,
            yaw: event
                .get("fps_camera_yaw")
                .and_then(Value::as_f64)
                .unwrap_or_default() as f32,
            fov_y: event
                .get("fov_y")
                .and_then(Value::as_f64)
                .unwrap_or_default() as f32,
            frustum_distance: event
                .get("frustum_distance")
                .and_then(Value::as_f64)
                .unwrap_or_default() as f32,
        })
    }
}

fn array3(value: &Value) -> Option<[f32; 3]> {
    let array = value.as_array()?;
    Some([
        array.first()?.as_f64()? as f32,
        array.get(1)?.as_f64()? as f32,
        array.get(2)?.as_f64()? as f32,
    ])
}

fn array4(value: &Value) -> Option<[f32; 4]> {
    let array = value.as_array()?;
    Some([
        array.first()?.as_f64()? as f32,
        array.get(1)?.as_f64()? as f32,
        array.get(2)?.as_f64()? as f32,
        array.get(3)?.as_f64()? as f32,
    ])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorStatus {
    Starting,
    Ready,
    Error,
}

/// Métricas de rendimiento que emite el motor (`debug_metrics`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DebugMetrics {
    pub fps: f32,
    pub frame_time_ms: f32,
    pub draw_calls: u64,
    pub physics_bodies: u64,
    pub cpu_percent: Option<f32>,
    pub gpu_percent: Option<f32>,
}

impl DebugMetrics {
    pub fn from_event(event: &Value) -> Self {
        Self {
            fps: number_field(event, "fps").unwrap_or(0.0) as f32,
            frame_time_ms: number_field(event, "frame_time_ms").unwrap_or(0.0) as f32,
            draw_calls: event.get("draw_calls").and_then(Value::as_u64).unwrap_or(0),
            physics_bodies: event
                .get("physics_bodies")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            cpu_percent: event
                .get("cpu_percent")
                .and_then(Value::as_f64)
                .map(|value| value as f32),
            gpu_percent: event
                .get("gpu_percent")
                .and_then(Value::as_f64)
                .map(|value| value as f32),
        }
    }
}

fn number_field(event: &Value, key: &str) -> Option<f64> {
    event.get(key).and_then(Value::as_f64)
}

fn control_summary(player: &Value) -> Option<ControlSummary> {
    let bindings = player
        .get("controls")
        .or_else(|| player.get("control_bindings"));
    let keys = |key: &str| {
        bindings
            .and_then(|bindings| bindings.get(key))
            .and_then(Value::as_object)
            .map(|map| map.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    Some(ControlSummary {
        player_name: player
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string),
        keyboard: keys("keyboard_mouse"),
        gamepad: keys("gamepad"),
    })
}

/// Guardado en curso: esperando el estado de la UI y/o el snapshot del motor.
pub struct PendingSave {
    pub id: u64,
    pub save_path: PathBuf,
    pub state: Option<ProjectState>,
    pub silent: bool,
    /// El hilo de escritura ya arrancó (el timeout deja de aplicar).
    pub writing: bool,
}

/// Entrada de la consola inferior (espejo de `LogEntry` en React).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub text: String,
    pub is_error: bool,
}

/// Máximo de líneas retenidas (React conserva 200).
pub const MAX_LOG_ENTRIES: usize = 200;

/// Resumen de los bindings de control del jugador (lectura).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ControlSummary {
    pub player_name: Option<String>,
    pub keyboard: Vec<String>,
    pub gamepad: Vec<String>,
}

pub struct EditorApp {
    pub cli: Cli,
    pub status: EditorStatus,
    pub preview_playing: bool,
    pub debug_mode: bool,
    pub saving: bool,
    pub has_saved_once: bool,
    pub auto_save_enabled: bool,
    pub save_path: Option<PathBuf>,
    pub extract_dir: Option<PathBuf>,
    pub save_error: Option<String>,
    pub log: Vec<LogEntry>,
    pub metrics: Option<DebugMetrics>,
    pub world: WorldSettings,
    pub world_sync_pending: bool,
    pub scenes: SceneList,
    pub assets: AssetLists,
    pub player_controls: Option<ControlSummary>,
    pub selected_entity: Option<SelectedEntity>,
    pub play_view: Option<PlayView>,
    /// Id de una entidad cuyo modal de propiedades se abrirá al seleccionarse.
    pub pending_properties: Option<u64>,
    pub request_seq: u64,
    pub pending_save: Option<PendingSave>,
    /// Proxy del event loop para el timeout de guardado.
    pub proxy: Option<EventLoopProxy<UserEvent>>,
    pub left_sidebar: SidebarLeftState,
    pub right_sidebar: SidebarRightState,
    /// Canal de comandos hacia el motor (lo conecta `main` al lanzarlo).
    pub engine_tx: Option<Sender<Value>>,
    /// Rectángulo central (en puntos) donde el motor dibuja la escena.
    pub viewport_rect: Option<egui::Rect>,
}

impl EditorApp {
    pub fn new(cli: Cli) -> Self {
        Self {
            save_path: cli.save_path.as_deref().map(PathBuf::from),
            extract_dir: cli.extract_dir.as_deref().map(PathBuf::from),
            has_saved_once: cli.save_path.is_some(),
            cli,
            status: EditorStatus::Starting,
            preview_playing: false,
            debug_mode: false,
            saving: false,
            auto_save_enabled: false,
            save_error: None,
            log: Vec::new(),
            metrics: None,
            world: WorldSettings::default(),
            world_sync_pending: false,
            scenes: SceneList::default(),
            assets: AssetLists::default(),
            player_controls: None,
            selected_entity: None,
            play_view: None,
            pending_properties: None,
            request_seq: 0,
            pending_save: None,
            proxy: None,
            left_sidebar: SidebarLeftState::default(),
            right_sidebar: SidebarRightState::default(),
            engine_tx: None,
            viewport_rect: None,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        crate::editor::ui::show(ui, self);
    }

    pub fn text(&self) -> crate::editor::i18n::Text {
        crate::editor::i18n::text(self.cli.locale)
    }

    pub fn send_engine(&self, command: Value) {
        if let Some(tx) = &self.engine_tx {
            let _ = tx.send(command);
        }
    }

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

    /// Actualiza el estado de la barra superior a partir de un evento del motor.
    pub fn handle_engine_event(&mut self, event: &Value) {
        let Some(name) = event.get("event").and_then(Value::as_str) else {
            return;
        };

        self.world.apply_event(event);
        self.scenes.apply_event(event);
        self.assets.apply_event(event);

        match name {
            "ready" => {
                self.status = EditorStatus::Ready;
                self.preview_playing = false;
            }
            "error" => {
                self.status = EditorStatus::Error;
            }
            "stopped" => {
                self.status = EditorStatus::Starting;
                self.preview_playing = false;
                self.left_sidebar.active_tool = None;
                self.selected_entity = None;
                self.play_view = None;
                self.pending_properties = None;
            }
            "entity_selected" => {
                self.selected_entity = SelectedEntity::from_event(event);
                if let Some(entity) = self.selected_entity.as_ref()
                    && self.pending_properties == Some(entity.id)
                {
                    self.pending_properties = None;
                    crate::editor::bridge::request_legacy_window("entity_properties", json!({}));
                }
            }
            "entity_properties_open" => {
                let id = event.get("id").and_then(Value::as_u64);
                match (id, self.selected_entity.as_ref()) {
                    (Some(id), Some(entity)) if entity.id == id => {
                        self.pending_properties = None;
                        crate::editor::bridge::request_legacy_window(
                            "entity_properties",
                            json!({}),
                        );
                    }
                    (Some(id), _) => self.pending_properties = Some(id),
                    _ => {}
                }
            }
            "entity_deselected" => {
                self.selected_entity = None;
                self.pending_properties = None;
            }
            "play_character_view_changed" => {
                self.play_view = PlayView::from_event(event);
            }
            "editor_scene_switched"
            | "project_loaded_3d"
            | "project_loaded_2d"
            | "project_load_3d_complete" => {
                self.selected_entity = None;
                self.play_view = None;
                self.pending_properties = None;
            }
            "preview_playing_changed" => {
                self.preview_playing = event
                    .get("playing")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            }
            "debug_metrics" => {
                self.metrics = Some(DebugMetrics::from_event(event));
            }
            "tool_cancelled" | "collider_created" | "execution_area_created" => {
                self.left_sidebar.active_tool = None;
            }
            "player_ui_active_screen_changed" => {
                self.left_sidebar.active_ui_screen = event
                    .get("screen_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            _ => {}
        }
    }

    /// Pide al motor un snapshot para sincronizar los ajustes de mundo.
    pub fn request_world_sync(&mut self) {
        if self.status != EditorStatus::Ready || self.world_sync_pending || self.saving {
            return;
        }
        self.world_sync_pending = true;
        self.send_engine(serde_json::json!({ "cmd": "export_save_snapshot" }));
    }

    /// Pide al motor las listas de recursos del proyecto.
    pub fn request_asset_sync(&mut self) {
        if self.status != EditorStatus::Ready {
            return;
        }
        for command in AssetLists::request_commands(self.cli.context.is_3d()) {
            self.send_engine(command);
        }
    }

    /// Aplica `scene.world` de un `save_snapshot_ready`.
    pub fn apply_world_snapshot(&mut self, scene: &Value) {
        self.world.apply_snapshot(scene);
        self.player_controls = scene
            .get("player")
            .filter(|player| player.is_object())
            .and_then(control_summary);
        self.world_sync_pending = false;
        log::info!(
            "[editor] mundo sincronizado (gravedad {:.2}, radio {:.1}, rejilla {}, tier sombras {:?})",
            self.world.gravity,
            self.world.world_radius,
            self.world.grid_visible,
            self.world.shadow_tier
        );
    }

    /// Aplica un comando de React para no desincronizar la barra derecha.
    pub fn apply_engine_command(&mut self, command: &Value) {
        self.world.apply_command(command);
    }

    pub fn can_save(&self) -> bool {
        self.status == EditorStatus::Ready && !self.saving
    }

    /// Inicia un guardado (con diálogo o silencioso para autosave).
    /// Devuelve el id de petición para poder armar el timeout.
    pub fn begin_save(&mut self, silent: bool) -> Option<u64> {
        if !self.can_save() {
            return None;
        }

        let save_path = match self.save_path.clone() {
            Some(path) => path,
            None if silent => return None,
            None => pick_save_path()?,
        };

        self.request_seq = self.request_seq.wrapping_add(1);
        let id = self.request_seq;
        self.saving = true;
        self.save_error = None;
        self.push_log(
            if silent {
                "[Autoguardado] guardando…".to_string()
            } else {
                format!("[Guardar] guardando en {}", save_path.display())
            },
            false,
        );
        self.pending_save = Some(PendingSave {
            id,
            save_path,
            state: None,
            silent,
            writing: false,
        });
        crate::editor::bridge::request_project_state(id);
        self.spawn_save_timeout(id);
        Some(id)
    }

    /// Recibe el estado de proyecto que envió la UI React.
    pub fn receive_project_state(&mut self, id: u64, state: &Value) -> bool {
        let Some(pending) = self.pending_save.as_mut() else {
            return false;
        };
        if pending.id != id || pending.state.is_some() {
            return false;
        }
        let parsed = ProjectState::from_value(state);
        pending.state = Some(parsed);
        self.send_engine(serde_json::json!({
            "cmd": "export_save_snapshot"
        }));
        true
    }

    /// Extrae el estado pendiente cuando llega el snapshot del motor.
    pub fn take_pending_save(&mut self) -> Option<(PathBuf, ProjectState)> {
        let pending = self.pending_save.as_mut()?;
        let state = pending.state.clone()?;
        pending.writing = true;
        let save_path = pending.save_path.clone();
        Some((save_path, state))
    }

    /// Finaliza el guardado en curso con el resultado del hilo de escritura.
    pub fn complete_save(&mut self, result: Result<SaveSuccess, String>) {
        let Some(pending) = self.pending_save.take() else {
            return;
        };
        self.saving = false;

        match result {
            Ok(success) => {
                self.save_path = Some(success.save_path.clone());
                self.has_saved_once = true;
                self.save_error = None;
                let prefix = if pending.silent {
                    "[Autoguardado]"
                } else {
                    "[Guardar]"
                };
                self.push_log(
                    format!(
                        "{prefix} {} ({} KB)",
                        success.file_name(),
                        success.bytes / 1024
                    ),
                    false,
                );
                self.notify_engine_project_saved();
                crate::editor::bridge::notify_project_saved(&success.save_path);
            }
            Err(message) => {
                self.save_error = Some(message.clone());
                self.push_log(format!("[Guardar] error: {message}"), true);
                crate::editor::bridge::notify_save_failed(&message);
            }
        }
    }

    /// Falla un guardado que excedió el tiempo de espera.
    pub fn timeout_save(&mut self, id: u64) {
        let Some(pending) = self.pending_save.as_ref() else {
            return;
        };
        if pending.id != id {
            return;
        }
        if pending.writing {
            return;
        }
        self.pending_save = None;
        self.saving = false;
        let message = self.text().save_timeout.to_string();
        self.save_error = Some(message.clone());
        self.push_log(message.clone(), true);
        crate::editor::bridge::notify_save_failed(&message);
    }

    /// Lanza un hilo que dispara el timeout si el guardado no arranca.
    fn spawn_save_timeout(&self, id: u64) {
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        std::thread::Builder::new()
            .name("save-timeout".into())
            .spawn(move || {
                std::thread::sleep(Duration::from_secs(20));
                let _ = proxy.send_event(UserEvent::SaveTimeout(id));
            })
            .ok();
    }

    /// Autosave disparado por el motor (`autosave_tick`).
    pub fn on_autosave_tick(&mut self) -> Option<u64> {
        if !self.auto_save_enabled || !self.has_saved_once || self.saving {
            return None;
        }
        self.begin_save(true)
    }

    pub fn toggle_auto_save(&mut self) {
        if !self.has_saved_once {
            return;
        }
        self.auto_save_enabled = !self.auto_save_enabled;
        self.send_engine(serde_json::json!({
            "cmd": "set_autosave",
            "enabled": self.auto_save_enabled
        }));
    }

    pub fn project_type_string(&self, state: &ProjectState) -> String {
        state
            .project_type
            .clone()
            .unwrap_or_else(|| self.cli.context.project_type.as_str().to_string())
    }

    pub fn game_style_string(&self, state: &ProjectState) -> String {
        state
            .game_style
            .clone()
            .unwrap_or_else(|| self.cli.context.game_style.as_str().to_string())
    }

    fn notify_engine_project_saved(&self) {
        if !self.cli.context.is_3d() {
            return;
        }
        let Some(extract_dir) = self.extract_dir.as_ref() else {
            return;
        };
        // Solo si el motor ya está montando un proyecto extraído con manifiesto.
        if !extract_dir.join("manifest.json").is_file() {
            return;
        }
        self.send_engine(serde_json::json!({
            "cmd": "notify_project_saved",
            "extract_dir": extract_dir.to_string_lossy(),
        }));
    }
}

fn pick_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("RER Save", &["save"])
        .set_file_name("RER-Project.save")
        .save_file()
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_app() -> EditorApp {
        EditorApp::new(Cli::default())
    }

    #[test]
    fn log_keeps_only_the_last_entries() {
        let mut app = test_app();
        for index in 0..(MAX_LOG_ENTRIES + 20) {
            app.push_log(format!("line {index}"), false);
        }
        assert_eq!(app.log.len(), MAX_LOG_ENTRIES);
        let expected = format!("line {}", MAX_LOG_ENTRIES + 19);
        assert_eq!(
            app.log.last().map(|entry| entry.text.as_str()),
            Some(expected.as_str())
        );
    }

    #[test]
    fn noisy_events_are_ignored() {
        let mut app = test_app();
        app.log_engine_event(&json!({ "event": "debug_metrics", "fps": 60 }));
        app.log_engine_event(&json!({ "event": "autosave_tick" }));
        app.log_engine_event(&json!({ "event": "player_ui_text_boxes_list" }));
        assert!(app.log.is_empty());
    }

    #[test]
    fn errors_and_loading_are_formatted() {
        let mut app = test_app();
        app.log_engine_event(&json!({ "event": "error", "message": "boom" }));
        assert!(app.log[0].is_error);
        assert!(app.log[0].text.contains("boom"));

        app.log_engine_event(&json!({
            "event": "load_progress",
            "message": "cargando",
            "step_ms": 10,
            "total_ms": 50
        }));
        assert_eq!(app.log[1].text, "[Carga] cargando (+10 ms, total 50 ms)");
        assert!(!app.log[1].is_error);
    }

    #[test]
    fn ready_reports_gravity() {
        let mut app = test_app();
        app.log_engine_event(&json!({ "event": "ready", "gravity": 9.81 }));
        assert_eq!(app.log[0].text, "[Motor] listo (gravedad 9.81)");
    }

    #[test]
    fn scene_boot_is_formatted_instead_of_dumped_as_json() {
        let mut app = test_app();
        app.log_engine_event(&json!({
            "event": "editor_scenes_updated",
            "active_scene_id": 1,
            "scenes": [{ "id": 1, "name": "Scene-01" }],
            "update_reason": "boot"
        }));
        assert_eq!(
            app.log[0].text,
            "[Escenas] Registro inicializado — activa «Scene-01» (1 escena/s: «Scene-01»)"
        );
    }

    #[test]
    fn undo_scene_sync_stays_silent() {
        let mut app = test_app();
        app.log_engine_event(&json!({
            "event": "editor_scenes_updated",
            "active_scene_id": 1,
            "scenes": [{ "id": 1, "name": "Scene-01" }],
            "update_reason": "undo_state"
        }));
        assert!(app.log.is_empty());
    }

    #[test]
    fn metrics_event_updates_panel_state() {
        let mut app = test_app();
        app.handle_engine_event(&json!({
            "event": "debug_metrics",
            "fps": 59.94,
            "frame_time_ms": 16.7,
            "draw_calls": 12,
            "physics_bodies": 3,
            "cpu_percent": 12.5,
            "gpu_percent": 3.2
        }));
        let metrics = app.metrics.expect("metrics actualizadas");
        assert!((metrics.fps - 59.94).abs() < 1e-3);
        assert_eq!(metrics.draw_calls, 12);
        assert_eq!(metrics.physics_bodies, 3);
        assert_eq!(metrics.cpu_percent, Some(12.5));
        assert_eq!(metrics.gpu_percent, Some(3.2));
    }

    #[test]
    fn metrics_without_gpu_stay_pending() {
        let metrics = DebugMetrics::from_event(&json!({
            "event": "debug_metrics",
            "fps": 30.0,
            "frame_time_ms": 33.3,
            "draw_calls": 1,
            "physics_bodies": 0,
            "cpu_percent": 0.0
        }));
        assert_eq!(metrics.gpu_percent, None);
        assert_eq!(metrics.cpu_percent, Some(0.0));
    }

    #[test]
    fn play_view_event_updates_camera_state() {
        let mut app = test_app();
        app.handle_engine_event(&json!({
            "event": "play_character_view_changed",
            "player_id": 7,
            "camera_eye_position": [1.5, 2.0, -3.0],
            "fps_camera_yaw": 0.5,
            "fov_y": 1.2,
            "frustum_distance": 12.0
        }));
        let view = app.play_view.expect("vista del jugador");
        assert_eq!(view.player_id, Some(7));
        assert_eq!(view.eye, [1.5, 2.0, -3.0]);
        assert!((view.yaw - 0.5).abs() < 1e-6);
        assert!((view.fov_y - 1.2).abs() < 1e-6);
        assert!((view.frustum_distance - 12.0).abs() < 1e-6);
    }

    #[test]
    fn entity_selection_tracks_engine_events() {
        let mut app = test_app();
        app.handle_engine_event(&json!({
            "event": "entity_selected",
            "id": 12,
            "name": "Player",
            "position": [1.0, 2.0, 3.0],
            "rotation": [0.0, 0.0, 0.0, 1.0],
            "scale": [1.0, 1.0, 1.0],
            "physics_enabled": true,
            "physics_type": "dynamic"
        }));
        let entity = app.selected_entity.as_ref().expect("entidad seleccionada");
        assert_eq!(entity.id, 12);
        assert_eq!(entity.name, "Player");
        assert_eq!(entity.position, [1.0, 2.0, 3.0]);
        assert!(entity.physics_enabled);

        app.handle_engine_event(&json!({ "event": "entity_deselected" }));
        assert_eq!(app.selected_entity, None);
    }
}
