//! Undo/redo del HUD Player UI compartido entre motores (snapshot por pantalla en edición).
//!
//! La única pieza dependiente del motor es la sesión de dibujo de objetos (`D`),
//! que sigue viviendo en `config_2d/3d::player_ui::object`.

use std::collections::HashMap;

use super::config::{PlayerUiButton, PlayerUiImage, PlayerUiObject, PlayerUiTextBox};

/// Snapshot de una pantalla del HUD, independiente del motor.
#[derive(Clone, Debug)]
pub struct PlayerUiHudUndoSnapshot<D = ()> {
    pub key: String,
    pub text_boxes: Vec<PlayerUiTextBox>,
    pub buttons: Vec<PlayerUiButton>,
    pub images: Vec<PlayerUiImage>,
    pub objects: Vec<PlayerUiObject>,
    pub object_draw: Option<D>,
    pub text_next_id: u32,
    pub selected_text_id: Option<u32>,
    pub selected_button_id: Option<u32>,
    pub selected_image_id: Option<u32>,
    pub selected_object_id: Option<u32>,
    pub text_editing_id: Option<u32>,
}

/// Inserta el vector de la pantalla, o lo elimina si quedó vacío.
pub fn restore_hud_vec<T: Clone>(map: &mut HashMap<String, Vec<T>>, key: &str, value: Vec<T>) {
    if value.is_empty() {
        map.remove(key);
    } else {
        map.insert(key.to_string(), value);
    }
}

/// Genera en el motor el alias `PlayerUiHudUndoSnapshot` y el `impl State` de undo.
///
/// `on_push` es el nombre de un método `&self` opcional que el motor 3D invoca
/// tras apilar el undo (sincronización de escenas del editor).
#[macro_export]
macro_rules! player_ui_hud_undo_impl {
    (
        $state:ty,
        $undo_action:ident,
        $object_draw:ty
        $(, on_push = $on_push:ident)?
        $(,)?
    ) => {
        /// Snapshot de undo del HUD con la sesión de dibujo propia del motor.
        pub(crate) type PlayerUiHudUndoSnapshot =
            $crate::player_ui::hud_undo::PlayerUiHudUndoSnapshot<$object_draw>;

        impl $state {
            pub(crate) fn capture_player_ui_hud_undo_snapshot_with_key(
                &self,
                key: &str,
            ) -> Option<PlayerUiHudUndoSnapshot> {
                if !self.player_ui_edit_active {
                    return None;
                }
                Some(PlayerUiHudUndoSnapshot {
                    key: key.to_string(),
                    text_boxes: self
                        .player_ui_text_boxes
                        .get(key)
                        .cloned()
                        .unwrap_or_default(),
                    buttons: self.player_ui_buttons.get(key).cloned().unwrap_or_default(),
                    images: self.player_ui_images.get(key).cloned().unwrap_or_default(),
                    objects: self.player_ui_objects.get(key).cloned().unwrap_or_default(),
                    object_draw: self.player_ui_object_draw.clone(),
                    text_next_id: self.player_ui_text_next_id,
                    selected_text_id: self.player_ui_selected_text_id,
                    selected_button_id: self.player_ui_selected_button_id,
                    selected_image_id: self.player_ui_selected_image_id,
                    selected_object_id: self.player_ui_selected_object_id,
                    text_editing_id: self.player_ui_text_editing_id,
                })
            }

            /// Registra el estado actual del HUD antes de una mutación (Ctrl+Z).
            pub(crate) fn push_undo_player_ui_hud(&mut self) {
                if self.is_applying_undo || !self.player_ui_edit_active {
                    return;
                }
                let Some(key) = self.player_ui_screen_key() else {
                    return;
                };
                let Some(snapshot) = self.capture_player_ui_hud_undo_snapshot_with_key(&key) else {
                    return;
                };
                self.redo_stack.clear();
                self.undo_stack
                    .push($undo_action::RestorePlayerUiHud { snapshot });
                $( self.$on_push(); )?
            }

            pub(crate) fn restore_player_ui_hud_undo_snapshot(
                &mut self,
                snap: PlayerUiHudUndoSnapshot,
            ) {
                let key = snap.key.clone();
                $crate::player_ui::hud_undo::restore_hud_vec(
                    &mut self.player_ui_text_boxes,
                    &key,
                    snap.text_boxes,
                );
                $crate::player_ui::hud_undo::restore_hud_vec(
                    &mut self.player_ui_buttons,
                    &key,
                    snap.buttons,
                );
                $crate::player_ui::hud_undo::restore_hud_vec(
                    &mut self.player_ui_images,
                    &key,
                    snap.images,
                );
                $crate::player_ui::hud_undo::restore_hud_vec(
                    &mut self.player_ui_objects,
                    &key,
                    snap.objects,
                );

                self.player_ui_object_draw = snap.object_draw;
                self.player_ui_text_next_id = snap.text_next_id;
                self.player_ui_selected_text_id = snap.selected_text_id;
                self.player_ui_selected_button_id = snap.selected_button_id;
                self.player_ui_selected_image_id = snap.selected_image_id;
                self.player_ui_selected_object_id = snap.selected_object_id;
                self.player_ui_text_editing_id = snap.text_editing_id;
                self.player_ui_text_drag = None;
                self.player_ui_last_text_click = None;

                self.rebuild_player_ui_overlay();
                self.rebuild_player_ui_object_draw_overlay();
                self.emit_player_ui_text_boxes_list();
            }
        }
    };
}
