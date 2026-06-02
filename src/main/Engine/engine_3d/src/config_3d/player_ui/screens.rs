//! Catálogo de pantallas Player UI y pantalla activa en play.

use crate::engine::State;
use crate::ipc::{send_event, EngineEvent, PlayerUiScreenInfo};

impl State {
    pub(crate) fn sync_player_ui_screens(&mut self, screens: &[PlayerUiScreenInfo]) {
        self.player_ui_player_screen_names.clear();
        for s in screens {
            self.player_ui_player_screen_names
                .insert(s.id.clone(), s.name.clone());
        }

        self.player_ui_active_player_screen_id = screens
            .iter()
            .find(|s| s.active)
            .map(|s| s.id.clone());

        if self.preview_playing {
            self.rebuild_player_ui_overlay();
        }
    }

    pub(crate) fn clear_active_player_ui_screen(&mut self) {
        self.player_ui_active_player_screen_id = None;
        if self.preview_playing {
            self.rebuild_player_ui_overlay();
        }
        send_event(&EngineEvent::PlayerUiActiveScreenChanged { screen_id: None });
        log::info!("[player-ui] ninguna pantalla activa para play");
    }

    pub(crate) fn set_active_player_ui_screen(
        &mut self,
        screen_id: &str,
    ) -> Result<(), String> {
        if !self.player_ui_player_screen_names.contains_key(screen_id) {
            return Err(format!("pantalla Player UI desconocida: {screen_id}"));
        }
        self.player_ui_active_player_screen_id = Some(screen_id.to_string());
        if self.preview_playing {
            self.rebuild_player_ui_overlay();
        }
        send_event(&EngineEvent::PlayerUiActiveScreenChanged {
            screen_id: Some(screen_id.to_string()),
        });
        log::info!("[player-ui] pantalla activa: {screen_id}");
        Ok(())
    }

    pub(crate) fn set_active_player_ui_screen_by_name(
        &mut self,
        name: &str,
    ) -> Result<(), String> {
        let id = self
            .player_ui_player_screen_names
            .iter()
            .find(|(_, n)| n.as_str() == name)
            .map(|(id, _)| id.clone())
            .ok_or_else(|| format!("pantalla Player UI con nombre '{name}' no encontrada"))?;
        self.set_active_player_ui_screen(&id)
    }

    pub(crate) fn player_ui_play_screen_id(&self) -> Option<String> {
        let id = self.player_ui_active_player_screen_id.as_ref()?;
        let key = format!("player:{id}");
        if self.player_ui_screen_has_hud_content(&key) {
            Some(id.clone())
        } else {
            None
        }
    }

    fn player_ui_screen_has_hud_content(&self, key: &str) -> bool {
        self.player_ui_text_boxes
            .get(key)
            .is_some_and(|v| !v.is_empty())
            || self
                .player_ui_buttons
                .get(key)
                .is_some_and(|v| !v.is_empty())
            || self
                .player_ui_images
                .get(key)
                .is_some_and(|v| !v.is_empty())
    }
}
