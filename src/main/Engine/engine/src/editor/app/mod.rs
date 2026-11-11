//! Estado raíz de la aplicación egui.

pub use self::events::{ControlSummary, DebugMetrics, PlayView, SelectedEntity};
pub use self::logs::LogEntry;
pub use self::save::PendingSave;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use winit::event_loop::EventLoopProxy;

use crate::editor::assets::AssetLists;
use crate::editor::cli::Cli;
use crate::editor::project_state::ProjectState;
use crate::editor::save::SaveSuccess;
use crate::editor::scenes::SceneList;
use crate::editor::ui::sidebar_left::SidebarLeftState;
use crate::editor::ui::sidebar_right::SidebarRightState;
use crate::editor::ui_screens::UiScreenLists;
use crate::editor::user_event::UserEvent;
use crate::editor::world::WorldSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorStatus {
    Starting,
    Ready,
    Error,
}

/// Máximo de líneas retenidas (React conserva 200).
pub const MAX_LOG_ENTRIES: usize = 200;

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
    pub ui_screens: UiScreenLists,
    /// Blueprint activo de construcción rápida (lo gestiona React).
    pub quick_build_active: Option<String>,
    pub player_controls: Option<ControlSummary>,
    /// Fondo 2D activo (ruta del asset), sincronizado con el motor.
    pub background_path: Option<String>,
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
    /// Rectángulo central (en puntos) donde el motor dibuja la escena.
    pub viewport_rect: Option<egui::Rect>,
    /// Canal de comandos hacia el motor in-process (se drena en cada frame).
    pub pending_engine: std::cell::RefCell<Vec<Value>>,
    /// El motor del proyecto se está construyendo; el viewport muestra un placeholder.
    pub engine_loading: bool,
    /// Error de arranque del motor, mostrado en el viewport.
    pub engine_error: Option<String>,
}

mod events;
mod logs;
mod save;

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
            ui_screens: UiScreenLists::default(),
            quick_build_active: None,
            player_controls: None,
            background_path: None,
            selected_entity: None,
            play_view: None,
            pending_properties: None,
            request_seq: 0,
            pending_save: None,
            proxy: None,
            left_sidebar: SidebarLeftState::default(),
            right_sidebar: SidebarRightState::default(),
            viewport_rect: None,
            pending_engine: std::cell::RefCell::new(Vec::new()),
            engine_loading: false,
            engine_error: None,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        crate::editor::ui::show(ui, self);
    }

    pub fn text(&self) -> crate::editor::i18n::Text {
        crate::editor::i18n::text(self.cli.locale)
    }

    pub fn send_engine(&self, command: Value) {
        // El host drena la cola hacia el motor in-process en el siguiente frame.
        self.pending_engine.borrow_mut().push(command);
    }
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
