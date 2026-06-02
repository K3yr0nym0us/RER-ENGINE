//! Bloqueo y `z_index` de elementos HUD (editor).

use crate::engine::State;

impl State {
    pub(crate) fn set_player_ui_hud_element_props(
        &mut self,
        element_kind: &str,
        id: u32,
        locked: Option<bool>,
        z_index: Option<i32>,
    ) -> Result<(), String> {
        if !self.player_ui_edit_active {
            return Err("modo edición UI inactivo".into());
        }
        if locked.is_none() && z_index.is_none() {
            return Err("sin cambios".into());
        }
        let key = self
            .player_ui_screen_key()
            .ok_or_else(|| "contexto de pantalla UI no definido".to_string())?;

        let updated = match element_kind {
            "text" => {
                let Some(list) = self.player_ui_text_boxes.get_mut(&key) else {
                    return Err(format!("cuadro de texto no encontrado: {id}"));
                };
                let Some(b) = list.iter_mut().find(|b| b.id == id) else {
                    return Err(format!("cuadro de texto no encontrado: {id}"));
                };
                if let Some(l) = locked {
                    b.locked = l;
                }
                if let Some(z) = z_index {
                    b.z_index = z;
                }
                true
            }
            "button" => {
                let Some(list) = self.player_ui_buttons.get_mut(&key) else {
                    return Err(format!("botón no encontrado: {id}"));
                };
                let Some(b) = list.iter_mut().find(|b| b.id == id) else {
                    return Err(format!("botón no encontrado: {id}"));
                };
                if let Some(l) = locked {
                    b.locked = l;
                }
                if let Some(z) = z_index {
                    b.z_index = z;
                }
                true
            }
            "image" => {
                let Some(list) = self.player_ui_images.get_mut(&key) else {
                    return Err(format!("imagen no encontrada: {id}"));
                };
                let Some(img) = list.iter_mut().find(|i| i.id == id) else {
                    return Err(format!("imagen no encontrada: {id}"));
                };
                if let Some(l) = locked {
                    img.locked = l;
                }
                if let Some(z) = z_index {
                    img.z_index = z;
                }
                true
            }
            _ => return Err(format!("tipo de elemento HUD desconocido: {element_kind}")),
        };

        if !updated {
            return Err(format!("elemento HUD no encontrado: {element_kind} {id}"));
        }

        self.rebuild_player_ui_overlay();
        self.emit_player_ui_text_boxes_list();
        log::info!(
            "[player-ui] props HUD actualizadas: kind={element_kind} id={id} locked={locked:?} z_index={z_index:?}"
        );
        Ok(())
    }

    pub(crate) fn player_ui_text_locked(&self, id: u32) -> bool {
        self.player_ui_text_key()
            .and_then(|key| self.player_ui_text_boxes.get(&key))
            .and_then(|list| list.iter().find(|b| b.id == id))
            .is_some_and(|b| b.locked)
    }

    pub(crate) fn player_ui_button_locked(&self, id: u32) -> bool {
        self.player_ui_text_key()
            .and_then(|key| self.player_ui_buttons.get(&key))
            .and_then(|list| list.iter().find(|b| b.id == id))
            .is_some_and(|b| b.locked)
    }

    pub(crate) fn player_ui_image_locked(&self, id: u32) -> bool {
        self.player_ui_text_key()
            .and_then(|key| self.player_ui_images.get(&key))
            .and_then(|list| list.iter().find(|i| i.id == id))
            .is_some_and(|i| i.locked)
    }
}
