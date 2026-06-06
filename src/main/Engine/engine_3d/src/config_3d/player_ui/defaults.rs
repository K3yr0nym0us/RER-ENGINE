//! HUD Player UI por defecto en plantillas first-person.

use crate::engine::State;
use crate::ipc::{SavePlayerUiObjectSnapshot, SaveUiScreenSnapshot};

use super::config::PlayerUiObject;

impl State {
    /// Pantalla HUD + crosshair si el proyecto FP aún no tiene Player UI (idempotente).
    pub(crate) fn ensure_default_fp_player_ui(&mut self) {
        use rer_engine_shared::editor_defaults::player_ui::{
            default_crosshair_horizontal_vertices, default_crosshair_vertical_vertices,
            DEFAULT_CROSSHAIR_FILL, DEFAULT_CROSSHAIR_H_OBJECT_ID, DEFAULT_CROSSHAIR_V_OBJECT_ID,
            DEFAULT_FP_SCREEN_ID, DEFAULT_FP_SCREEN_NAME,
        };

        if !self.player_ui_player_screen_names.is_empty()
            || self
                .player_ui_objects
                .values()
                .any(|objects| !objects.is_empty())
        {
            return;
        }

        self.player_ui_player_screen_names.insert(
            DEFAULT_FP_SCREEN_ID.to_string(),
            DEFAULT_FP_SCREEN_NAME.to_string(),
        );
        self.player_ui_active_player_screen_id = Some(DEFAULT_FP_SCREEN_ID.to_string());

        let key = format!("player:{DEFAULT_FP_SCREEN_ID}");
        let objects = self.player_ui_objects.entry(key).or_default();
        objects.push(PlayerUiObject {
            id: DEFAULT_CROSSHAIR_H_OBJECT_ID,
            vertices: default_crosshair_horizontal_vertices(),
            fill_color: DEFAULT_CROSSHAIR_FILL,
            texture_path: None,
            z_index: 1000,
            locked: false,
        });
        objects.push(PlayerUiObject {
            id: DEFAULT_CROSSHAIR_V_OBJECT_ID,
            vertices: default_crosshair_vertical_vertices(),
            fill_color: DEFAULT_CROSSHAIR_FILL,
            texture_path: None,
            z_index: 1000,
            locked: false,
        });
        self.player_ui_text_next_id = self
            .player_ui_text_next_id
            .max(DEFAULT_CROSSHAIR_V_OBJECT_ID.saturating_add(1));

        log::info!("[player-ui] HUD FP por defecto: pantalla + crosshair");
    }
}

pub(crate) fn default_fp_project_ui_screens() -> Vec<SaveUiScreenSnapshot> {
    use rer_engine_shared::editor_defaults::player_ui::{
        DEFAULT_FP_SCREEN_ID, DEFAULT_FP_SCREEN_NAME,
    };
    vec![SaveUiScreenSnapshot {
        id: DEFAULT_FP_SCREEN_ID.to_string(),
        name: DEFAULT_FP_SCREEN_NAME.to_string(),
        active: true,
    }]
}

pub(crate) fn default_fp_project_ui_objects() -> Vec<SavePlayerUiObjectSnapshot> {
    use rer_engine_shared::editor_defaults::player_ui::{
        default_crosshair_horizontal_vertices, default_crosshair_vertical_vertices,
        DEFAULT_CROSSHAIR_FILL, DEFAULT_CROSSHAIR_H_OBJECT_ID, DEFAULT_CROSSHAIR_V_OBJECT_ID,
        DEFAULT_FP_SCREEN_ID,
    };
    vec![
        SavePlayerUiObjectSnapshot {
            scope: "player".to_string(),
            screen_id: DEFAULT_FP_SCREEN_ID.to_string(),
            id: DEFAULT_CROSSHAIR_H_OBJECT_ID,
            vertices: default_crosshair_horizontal_vertices(),
            fill_color: DEFAULT_CROSSHAIR_FILL,
            texture_path: None,
            z_index: 1000,
            locked: false,
        },
        SavePlayerUiObjectSnapshot {
            scope: "player".to_string(),
            screen_id: DEFAULT_FP_SCREEN_ID.to_string(),
            id: DEFAULT_CROSSHAIR_V_OBJECT_ID,
            vertices: default_crosshair_vertical_vertices(),
            fill_color: DEFAULT_CROSSHAIR_FILL,
            texture_path: None,
            z_index: 1000,
            locked: false,
        },
    ]
}
