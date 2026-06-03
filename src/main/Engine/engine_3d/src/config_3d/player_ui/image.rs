//! Imágenes HUD del jugador (alta/baja/lista/persistencia).

use crate::engine::State;
use crate::ipc::{
    send_event, EngineEvent, PlayerUiImageListItem, SavePlayerUiImageSnapshot,
};

use super::config::{
    default_image_width_ndc, ndc_height_for_width, PlayerUiImage, BOX_STACK_GAP,
};

impl State {
    pub(crate) fn add_player_ui_image(&mut self, image_path: &str) -> Result<u32, String> {
        if !self.player_ui_edit_active {
            return Err("modo edición UI inactivo".into());
        }
        let key = self
            .player_ui_screen_key()
            .ok_or_else(|| "contexto de pantalla UI no definido".to_string())?;

        let entry_meta = self
            .hud_image_store
            .get(image_path)
            .ok_or_else(|| format!("imagen no registrada: {image_path}"))?;

        let id = self.player_ui_text_next_id;
        self.player_ui_text_next_id = self.player_ui_text_next_id.saturating_add(1);

        let index = self.player_ui_images.get(&key).map_or(0, |v| v.len());
        let center_y = 0.12_f32 - index as f32 * BOX_STACK_GAP;
        let z_index = super::hud_layers::next_z_index_for_screen(
            self.player_ui_text_boxes.get(&key).map(|v| v.as_slice()),
            self.player_ui_buttons.get(&key).map(|v| v.as_slice()),
            self.player_ui_images.get(&key).map(|v| v.as_slice()),
            self.player_ui_objects.get(&key).map(|v| v.as_slice()),
        );

        let source_aspect = if entry_meta.height_px > 0 {
            entry_meta.width_px as f32 / entry_meta.height_px as f32
        } else {
            1.0
        };
        let width = default_image_width_ndc(source_aspect);
        let vw = self.size.width.max(1) as f32;
        let vh = self.size.height.max(1) as f32;
        let height = ndc_height_for_width(width, source_aspect, vw, vh);

        let image_name = entry_meta.name.clone();
        self.push_undo_player_ui_hud();

        let entry = PlayerUiImage {
            id,
            image_path: image_path.to_string(),
            image_name: image_name.clone(),
            center_x: 0.0,
            center_y,
            width,
            height,
            source_aspect,
            z_index,
            locked: false,
        };

        self.player_ui_images.entry(key).or_default().push(entry);
        self.player_ui_selected_image_id = Some(id);
        self.player_ui_selected_text_id = None;
        self.player_ui_selected_button_id = None;
        self.player_ui_selected_object_id = None;
        self.player_ui_text_editing_id = None;
        self.player_ui_text_drag = None;
        self.rebuild_player_ui_overlay();
        log::info!("[player-ui] imagen HUD añadida: id={} {}", id, image_name);
        send_event(&EngineEvent::PlayerUiImageAdded {
            id,
            image_name,
        });
        Ok(id)
    }

    pub(crate) fn remove_player_ui_image(&mut self, id: u32) -> bool {
        let Some(key) = self.player_ui_screen_key() else {
            return false;
        };
        if !self
            .player_ui_images
            .get(&key)
            .is_some_and(|list| list.iter().any(|img| img.id == id))
        {
            return false;
        }
        self.push_undo_player_ui_hud();
        let Some(list) = self.player_ui_images.get_mut(&key) else {
            return false;
        };
        list.retain(|img| img.id != id);
        if self.player_ui_selected_image_id == Some(id) {
            self.player_ui_selected_image_id = None;
        }
        self.player_ui_text_drag = None;
        self.rebuild_player_ui_overlay();
        log::info!("[player-ui] imagen HUD eliminada: id={id}");
        send_event(&EngineEvent::PlayerUiImageRemoved { id });
        true
    }

    pub(crate) fn remove_selected_player_ui_image(&mut self) -> Option<u32> {
        let id = self.player_ui_selected_image_id?;
        self.remove_player_ui_image(id).then_some(id)
    }

    pub(crate) fn import_player_ui_images_from_save(
        &mut self,
        images: &[SavePlayerUiImageSnapshot],
    ) {
        self.player_ui_images.clear();
        for snap in images {
            let key = format!("{}:{}", snap.scope, snap.screen_id);
            self.player_ui_images.entry(key).or_default().push(PlayerUiImage {
                id: snap.id,
                image_path: snap.image_path.clone(),
                image_name: snap.image_name.clone(),
                center_x: snap.center_x,
                center_y: snap.center_y,
                width: snap.width,
                height: snap.height,
                source_aspect: snap.source_aspect.max(0.01),
                z_index: snap.z_index,
                locked: snap.locked,
            });
            self.player_ui_text_next_id = self
                .player_ui_text_next_id
                .max(snap.id.saturating_add(1));
        }
        if self.player_ui_edit_active {
            self.rebuild_player_ui_overlay();
        }
        log::info!(
            "[player-ui] importadas {} imágenes HUD desde .save",
            images.len()
        );
    }
}

pub(crate) fn list_images_for_event(state: &State, key: &str) -> Vec<PlayerUiImageListItem> {
    state
        .player_ui_images
        .get(key)
        .map(|list| {
            list.iter()
                .map(|img| PlayerUiImageListItem {
                    id: img.id,
                    image_name: img.image_name.clone(),
                    z_index: img.z_index,
                    locked: img.locked,
                })
                .collect()
        })
        .unwrap_or_default()
}
