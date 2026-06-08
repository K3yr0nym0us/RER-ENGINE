//! Catálogo de pantallas Player UI y pantalla activa en play.

use crate::engine::State;
use crate::ipc::{send_event, EngineEvent, PlayerUiScreenInfo};

impl State {
    fn player_ui_screens_catalog_matches(&self, screens: &[PlayerUiScreenInfo]) -> bool {
        if screens.len() != self.player_ui_player_screen_names.len() {
            return false;
        }
        screens.iter().all(|s| {
            self.player_ui_player_screen_names
                .get(&s.id)
                .is_some_and(|name| name == &s.name)
        })
    }

    fn player_ui_active_screen_from_list(screens: &[PlayerUiScreenInfo]) -> Option<String> {
        screens
            .iter()
            .find(|s| s.active)
            .map(|s| s.id.clone())
    }

    fn player_ui_active_screen_matches(&self, screens: &[PlayerUiScreenInfo]) -> bool {
        Self::player_ui_active_screen_from_list(screens) == self.player_ui_active_player_screen_id
    }

    pub(crate) fn sync_player_ui_screens(&mut self, screens: &[PlayerUiScreenInfo]) {
        let catalog_same = self.player_ui_screens_catalog_matches(screens);
        let active_same = self.player_ui_active_screen_matches(screens);
        if catalog_same && active_same {
            return;
        }

        let prev_len = self.player_ui_player_screen_names.len();
        let prev_active = self.player_ui_active_player_screen_id.clone();
        let prev_ids: std::collections::HashSet<String> =
            self.player_ui_player_screen_names.keys().cloned().collect();

        self.player_ui_player_screen_names.clear();
        for s in screens {
            self.player_ui_player_screen_names
                .insert(s.id.clone(), s.name.clone());
        }

        self.player_ui_active_player_screen_id = Self::player_ui_active_screen_from_list(screens);

        if self.preview_playing {
            let structure_changed = screens.len() != prev_len
                || screens.iter().any(|s| !prev_ids.contains(&s.id));
            let active_changed = self.player_ui_active_player_screen_id != prev_active;
            if active_changed || structure_changed {
                self.rebuild_player_ui_overlay();
            }
        }
    }

    pub(crate) fn clear_active_player_ui_screen(&mut self) {
        if self.player_ui_active_player_screen_id.is_none() {
            return;
        }
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
        if self.player_ui_active_player_screen_id.as_deref() == Some(screen_id) {
            return Ok(());
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
            || self
                .player_ui_objects
                .get(key)
                .is_some_and(|v| !v.is_empty())
    }
}
