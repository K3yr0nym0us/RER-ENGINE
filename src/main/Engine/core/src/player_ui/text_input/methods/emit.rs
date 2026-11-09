//! Emisión de listas/estado y restauración desde `.save`.
//!
//! Expande los métodos de este grupo dentro del `impl State` del motor.

#[doc(hidden)]
#[macro_export]
macro_rules! __rer_player_ui_text_emit {
    () => {
        fn emit_player_ui_text_updated(&self, id: u32, text: &str) {
            send_event(&EngineEvent::PlayerUiTextBoxUpdated {
                id,
                text: text.to_string(),
            });
        }

        pub(crate) fn emit_player_ui_text_boxes_list(&self) {
            if !self.player_ui_edit_active {
                return;
            }
            let Some(scope) = self.player_ui_edit_scope.clone() else {
                return;
            };
            let Some(screen_id) = self.player_ui_edit_screen_id.clone() else {
                return;
            };
            let key = format!("{scope}:{screen_id}");
            let boxes: Vec<PlayerUiTextBoxListItem> = self
                .player_ui_text_boxes
                .get(&key)
                .map(|list| {
                    list.iter()
                        .map(|b| PlayerUiTextBoxListItem {
                            id: b.id,
                            font_name: b.font_name.clone(),
                            text: b.text.clone(),
                            z_index: b.z_index,
                            locked: b.locked,
                        })
                        .collect()
                })
                .unwrap_or_default();
            let buttons: Vec<PlayerUiButtonListItem> = button::list_buttons_for_event(self, &key);
            let images: Vec<PlayerUiImageListItem> = image::list_images_for_event(self, &key);
            let objects = object::list_objects_for_event(self, &key);
            send_event(&EngineEvent::PlayerUiTextBoxesList {
                scope,
                screen_id,
                boxes,
                buttons,
                images,
                objects,
            });
        }

        pub(crate) fn import_player_ui_text_boxes_from_save(
            &mut self,
            boxes: &[SavePlayerUiTextBoxSnapshot],
        ) {
            self.player_ui_text_boxes.clear();
            let mut max_id = 0u32;
            for snap in boxes {
                max_id = max_id.max(snap.id);
                let key = format!("{}:{}", snap.scope, snap.screen_id);
                self.player_ui_text_boxes
                    .entry(key)
                    .or_default()
                    .push(PlayerUiTextBox {
                        id: snap.id,
                        font_path: snap.font_path.clone(),
                        font_name: snap.font_name.clone(),
                        text: snap.text.clone(),
                        center_x: snap.center_x,
                        center_y: snap.center_y,
                        width: snap.width,
                        height: snap.height,
                        z_index: snap.z_index,
                        locked: snap.locked,
                    });
            }
            if max_id > 0 {
                self.player_ui_text_next_id =
                    self.player_ui_text_next_id.max(max_id.saturating_add(1));
            }
            if self.player_ui_edit_active {
                self.rebuild_player_ui_overlay();
            }
            log::info!(
                "[player-ui] importados {} cuadros de texto desde .save",
                boxes.len()
            );
        }
    };
}
