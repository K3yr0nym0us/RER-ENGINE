//! HUD Player UI por defecto en proyectos 2D.

use crate::engine::State;
use crate::ipc::PlayerUiScreenInfo;

pub(crate) const DEFAULT_2D_SCREEN_ID: &str = "hud-01";
pub(crate) const DEFAULT_2D_SCREEN_NAME: &str = "Player UI 01";

impl State {
    /// Pantalla HUD vacía si el proyecto 2D aún no tiene Player UI (idempotente).
    pub(crate) fn ensure_default_player_ui(&mut self) {
        if !self.player_ui_player_screen_names.is_empty() {
            return;
        }

        self.player_ui_player_screen_names.insert(
            DEFAULT_2D_SCREEN_ID.to_string(),
            DEFAULT_2D_SCREEN_NAME.to_string(),
        );
        self.player_ui_active_player_screen_id = Some(DEFAULT_2D_SCREEN_ID.to_string());

        log::info!("[player-ui] HUD 2D por defecto: pantalla «{DEFAULT_2D_SCREEN_NAME}»");
    }
}

pub(crate) fn default_2d_project_ui_screens_info() -> Vec<PlayerUiScreenInfo> {
    vec![PlayerUiScreenInfo {
        id: DEFAULT_2D_SCREEN_ID.to_string(),
        name: DEFAULT_2D_SCREEN_NAME.to_string(),
        active: true,
    }]
}
