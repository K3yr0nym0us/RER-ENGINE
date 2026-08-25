//! Lógica de botones HUD (alta/baja/lista); render en `button_render.rs`.

use crate::engine::State;
use crate::ipc::{
    AddPlayerUiButtonPayload, EngineEvent, PlayerUiButtonListItem, SavePlayerUiButtonSnapshot,
    send_event,
};

use super::config::{BOX_STACK_GAP, PlayerUiButton, default_button_size_ndc, parse_hex_color_rgba};

impl State {
    pub(crate) fn add_player_ui_button(
        &mut self,
        payload: AddPlayerUiButtonPayload,
    ) -> Result<u32, String> {
        if !self.player_ui_edit_active {
            return Err("modo edición UI inactivo".into());
        }
        let key = self
            .player_ui_screen_key()
            .ok_or_else(|| "contexto de pantalla UI no definido".to_string())?;

        let font_name = if payload.font_name.trim().is_empty() {
            self.font_store
                .get(&payload.font_path)
                .cloned()
                .unwrap_or_default()
        } else {
            payload.font_name
        };

        if !payload.font_path.is_empty() && !self.font_store.contains_key(&payload.font_path) {
            return Err(format!("fuente no registrada: {}", payload.font_path));
        }

        let id = self.player_ui_text_next_id;
        self.player_ui_text_next_id = self.player_ui_text_next_id.saturating_add(1);

        let index = self.player_ui_buttons.get(&key).map_or(0, |v| v.len());
        let (w, h) = default_button_size_ndc(&payload.shape_type);
        let source_aspect = (w / h).max(0.01);
        let center_y = 0.12_f32 - index as f32 * BOX_STACK_GAP;
        let z_index = super::hud_layers::next_z_index_for_screen(
            self.player_ui_text_boxes.get(&key).map(|v| v.as_slice()),
            self.player_ui_buttons.get(&key).map(|v| v.as_slice()),
            self.player_ui_images.get(&key).map(|v| v.as_slice()),
            self.player_ui_objects.get(&key).map(|v| v.as_slice()),
        );

        let bg_alpha = payload.transparency_background;
        let text_alpha = payload.transparency_text;
        self.push_undo_player_ui_hud();

        let entry = PlayerUiButton {
            id,
            shape_type: payload.shape_type,
            round: payload.round,
            background_color: parse_hex_color_rgba(&payload.background_color, bg_alpha),
            texture_path: payload.texture_path.filter(|p| !p.is_empty()),
            transparency_background: (bg_alpha / 100.0).clamp(0.0, 1.0),
            text: payload.text,
            text_color: parse_hex_color_rgba(&payload.text_color, text_alpha),
            transparency_text: (text_alpha / 100.0).clamp(0.0, 1.0),
            font_path: payload.font_path,
            font_name,
            border_color: parse_hex_color_rgba(&payload.border_color, 100.0),
            border_weight: payload.border_weight.max(0.0),
            center_x: 0.0,
            center_y,
            width: w,
            height: h,
            source_aspect,
            z_index,
            locked: false,
        };

        self.player_ui_buttons
            .entry(key)
            .or_default()
            .push(entry.clone());
        self.player_ui_selected_button_id = Some(id);
        self.player_ui_selected_text_id = None;
        self.player_ui_selected_image_id = None;
        self.player_ui_selected_object_id = None;
        self.player_ui_text_editing_id = None;
        self.player_ui_text_drag = None;
        self.rebuild_player_ui_overlay();
        log::info!("[player-ui] botón añadido: id={}", id);
        send_event(&EngineEvent::PlayerUiButtonAdded {
            id: entry.id,
            text: entry.text.clone(),
            font_name: entry.font_name.clone(),
        });
        Ok(id)
    }

    pub(crate) fn remove_player_ui_button(&mut self, id: u32) -> bool {
        let Some(key) = self.player_ui_screen_key() else {
            return false;
        };
        if !self
            .player_ui_buttons
            .get(&key)
            .is_some_and(|list| list.iter().any(|b| b.id == id))
        {
            return false;
        }
        self.push_undo_player_ui_hud();
        let Some(list) = self.player_ui_buttons.get_mut(&key) else {
            return false;
        };
        list.retain(|b| b.id != id);
        if self.player_ui_selected_button_id == Some(id) {
            self.player_ui_selected_button_id = None;
        }
        self.player_ui_text_drag = None;
        self.rebuild_player_ui_overlay();
        log::info!("[player-ui] botón eliminado: id={id}");
        true
    }

    pub(crate) fn import_player_ui_buttons_from_save(
        &mut self,
        buttons: &[SavePlayerUiButtonSnapshot],
    ) {
        self.player_ui_buttons.clear();
        for snap in buttons {
            let key = format!("{}:{}", snap.scope, snap.screen_id);
            self.player_ui_buttons
                .entry(key)
                .or_default()
                .push(PlayerUiButton {
                    id: snap.id,
                    shape_type: snap.shape_type.clone(),
                    round: snap.round,
                    background_color: [
                        snap.background_color[0],
                        snap.background_color[1],
                        snap.background_color[2],
                        snap.background_color[3],
                    ],
                    texture_path: snap.texture_path.clone(),
                    transparency_background: snap.transparency_background,
                    text: snap.text.clone(),
                    text_color: [
                        snap.text_color[0],
                        snap.text_color[1],
                        snap.text_color[2],
                        snap.text_color[3],
                    ],
                    transparency_text: snap.transparency_text,
                    font_path: snap.font_path.clone(),
                    font_name: snap.font_name.clone(),
                    border_color: [
                        snap.border_color[0],
                        snap.border_color[1],
                        snap.border_color[2],
                        snap.border_color[3],
                    ],
                    border_weight: snap.border_weight,
                    center_x: snap.center_x,
                    center_y: snap.center_y,
                    width: snap.width,
                    height: snap.height,
                    source_aspect: snap
                        .source_aspect
                        .filter(|a| *a > 0.0)
                        .unwrap_or_else(|| (snap.width / snap.height.max(0.01)).max(0.01)),
                    z_index: snap.z_index,
                    locked: snap.locked,
                });
            self.player_ui_text_next_id =
                self.player_ui_text_next_id.max(snap.id.saturating_add(1));
        }
        let vw = self.size.width.max(1) as f32;
        let vh = self.size.height.max(1) as f32;
        for list in self.player_ui_buttons.values_mut() {
            for b in list.iter_mut() {
                b.sync_height_for_viewport(vw, vh);
            }
        }
        if self.player_ui_edit_active {
            self.rebuild_player_ui_overlay();
        }
        log::info!(
            "[player-ui] importados {} botones desde .save",
            buttons.len()
        );
    }
}

pub(crate) fn list_buttons_for_event(state: &State, key: &str) -> Vec<PlayerUiButtonListItem> {
    state
        .player_ui_buttons
        .get(key)
        .map(|list| {
            list.iter()
                .map(|b| PlayerUiButtonListItem {
                    id: b.id,
                    text: b.text.clone(),
                    font_name: b.font_name.clone(),
                    z_index: b.z_index,
                    locked: b.locked,
                })
                .collect()
        })
        .unwrap_or_default()
}
