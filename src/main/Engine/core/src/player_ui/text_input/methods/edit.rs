//! Alta/baja de cuadros de texto y edición de contenido.
//!
//! Expande los métodos de este grupo dentro del `impl State` del motor.

#[doc(hidden)]
#[macro_export]
macro_rules! __rer_player_ui_text_edit {
    () => {
        pub(crate) fn add_player_ui_text_box(&mut self, font_path: &str) -> Result<u32, String> {
            if !self.player_ui_edit_active {
                return Err("modo edición UI inactivo".into());
            }
            let key = self
                .player_ui_text_key()
                .ok_or_else(|| "contexto de pantalla UI no definido".to_string())?;

            let font_name = self
                .font_store
                .get(font_path)
                .cloned()
                .ok_or_else(|| format!("fuente no registrada: {font_path}"))?;

            let id = self.player_ui_text_next_id;
            self.player_ui_text_next_id = self.player_ui_text_next_id.saturating_add(1);

            let index = self.player_ui_text_boxes.get(&key).map_or(0, |v| v.len());
            let center_y = 0.28_f32 - index as f32 * BOX_STACK_GAP;
            let z_index = rer_engine_core::player_ui::hud_layers::next_z_index_for_screen(
                self.player_ui_text_boxes.get(&key).map(|v| v.as_slice()),
                self.player_ui_buttons.get(&key).map(|v| v.as_slice()),
                self.player_ui_images.get(&key).map(|v| v.as_slice()),
                self.player_ui_objects.get(&key).map(|v| v.as_slice()),
            );

            self.push_undo_player_ui_hud();

            let entry = PlayerUiTextBox {
                id,
                font_path: font_path.to_string(),
                font_name,
                text: "Texto".to_string(),
                center_x: 0.0,
                center_y,
                width: DEFAULT_BOX_W,
                height: DEFAULT_BOX_H,
                z_index,
                locked: false,
            };

            self.player_ui_text_boxes
                .entry(key)
                .or_default()
                .push(entry.clone());

            self.player_ui_selected_text_id = Some(id);
            self.player_ui_selected_button_id = None;
            self.player_ui_selected_image_id = None;
            self.player_ui_selected_object_id = None;
            self.player_ui_text_editing_id = None;
            self.player_ui_text_drag = None;
            self.rebuild_player_ui_overlay();
            log::info!(
                "[player-ui] cuadro de texto añadido: id={} fuente={}",
                id,
                entry.font_name
            );
            Ok(id)
        }

        pub(crate) fn remove_selected_player_ui_text_box(&mut self) -> Option<u32> {
            let id = self.player_ui_selected_text_id?;
            self.remove_player_ui_text_box(id).then_some(id)
        }

        pub(crate) fn remove_player_ui_text_box(&mut self, id: u32) -> bool {
            let Some(key) = self.player_ui_text_key() else {
                return false;
            };
            if !self
                .player_ui_text_boxes
                .get(&key)
                .is_some_and(|list| list.iter().any(|b| b.id == id))
            {
                return false;
            }
            self.push_undo_player_ui_hud();
            let Some(list) = self.player_ui_text_boxes.get_mut(&key) else {
                return false;
            };
            list.retain(|b| b.id != id);
            if self.player_ui_selected_text_id == Some(id) {
                self.player_ui_selected_text_id = None;
            }
            if self.player_ui_text_editing_id == Some(id) {
                self.player_ui_text_editing_id = None;
            }
            self.player_ui_text_drag = None;
            self.player_ui_last_text_click = None;
            self.rebuild_player_ui_overlay();
            log::info!("[player-ui] cuadro de texto eliminado: id={id}");
            true
        }

        /// Teclas en modo edición UI sin edición de caracteres activa (p. ej. Supr → borrar cuadro).
        pub(crate) fn player_ui_edit_key_input(
            &mut self,
            code: winit::keyboard::KeyCode,
            pressed: bool,
            repeat: bool,
        ) -> bool {
            if !self.player_ui_edit_active || !pressed || repeat {
                return false;
            }
            if self.player_ui_text_editing_id.is_some() {
                return false;
            }
            use winit::keyboard::KeyCode;
            match code {
                KeyCode::Escape => {
                    if self.player_ui_object_draw_active() {
                        self.cancel_player_ui_object_draw();
                        return true;
                    }
                    false
                }
                KeyCode::Delete => {
                    if let Some(id) = self.player_ui_selected_object_id {
                        let _ = self.remove_player_ui_object(id);
                        return true;
                    }
                    if let Some(id) = self.player_ui_selected_image_id {
                        if self.remove_player_ui_image(id) {
                            self.player_ui_selected_image_id = None;
                        }
                        return true;
                    }
                    if let Some(id) = self.player_ui_selected_button_id {
                        if self.remove_player_ui_button(id) {
                            self.player_ui_selected_button_id = None;
                        }
                        return true;
                    }
                    self.remove_selected_player_ui_text_box();
                    true
                }
                _ => false,
            }
        }

        /// Fija `source_aspect` al tamaño NDC actual para que resize sin Shift use esa proporción.
        pub(crate) fn commit_player_ui_image_display_aspect(&mut self, id: u32) {
            let vw = self.size.width.max(1) as f32;
            let vh = self.size.height.max(1) as f32;
            let Some(images) = self.player_ui_images_mut() else {
                return;
            };
            if let Some(img) = images.iter_mut().find(|i| i.id == id) {
                img.source_aspect =
                    config::source_aspect_from_ndc_rect(img.width, img.height, vw, vh);
            }
        }

        /// Al soltar Shift: guardar la proporción actual y seguir en modo proporcional.
        pub(crate) fn player_ui_on_shift_released(&mut self) {
            if !self.player_ui_edit_active {
                return;
            }
            let Some(PuTextDrag::ImageResize { id, .. }) = self.player_ui_text_drag.clone() else {
                return;
            };
            self.commit_player_ui_image_display_aspect(id);
            self.rebuild_player_ui_overlay_live();
        }

        pub(crate) fn player_ui_text_key_input(
            &mut self,
            code: winit::keyboard::KeyCode,
            pressed: bool,
            repeat: bool,
            text: Option<&str>,
        ) -> bool {
            if !self.player_ui_edit_active {
                return false;
            }
            let Some(edit_id) = self.player_ui_text_editing_id else {
                return false;
            };
            if !pressed {
                return true;
            }
            use winit::keyboard::KeyCode;

            if repeat {
                let nav = matches!(
                    code,
                    KeyCode::Backspace
                        | KeyCode::Delete
                        | KeyCode::ArrowLeft
                        | KeyCode::ArrowRight
                        | KeyCode::Home
                        | KeyCode::End
                );
                if !nav && text.is_none() {
                    return true;
                }
            }

            if code == KeyCode::Backspace {
                self.player_ui_text_delete_before_caret(edit_id);
                return true;
            }

            if code == KeyCode::Delete {
                self.player_ui_text_delete_after_caret(edit_id);
                return true;
            }

            if let Some(s) = text.filter(|t| !t.is_empty()) {
                if s.chars().any(|c| c == '\u{8}' || c == '\u{7f}') {
                    self.player_ui_text_delete_before_caret(edit_id);
                } else {
                    self.player_ui_text_insert_at_caret(edit_id, s);
                }
                return true;
            }

            match code {
                KeyCode::ArrowLeft => {
                    self.player_ui_text_caret = self.player_ui_text_caret.saturating_sub(1);
                    self.player_ui_restart_caret_blink();
                }
                KeyCode::ArrowRight => {
                    if let Some(b) = self.player_ui_boxes_for_id(edit_id) {
                        let max = b.text.chars().count();
                        if self.player_ui_text_caret < max {
                            self.player_ui_text_caret += 1;
                        }
                    }
                    self.player_ui_restart_caret_blink();
                }
                KeyCode::Home => {
                    self.player_ui_text_caret = 0;
                    self.player_ui_restart_caret_blink();
                }
                KeyCode::End => {
                    if let Some(b) = self.player_ui_boxes_for_id(edit_id) {
                        self.player_ui_text_caret = b.text.chars().count();
                    }
                    self.player_ui_restart_caret_blink();
                }
                KeyCode::Escape => {
                    self.player_ui_text_editing_id = None;
                    self.player_ui_text_caret = 0;
                    self.rebuild_player_ui_overlay();
                }
                KeyCode::Enter => {}
                _ => {}
            }
            true
        }

        fn player_ui_text_delete_before_caret(&mut self, id: u32) {
            let caret = self.player_ui_text_caret;
            if caret == 0 {
                return;
            }
            let mut updated: Option<String> = None;
            if let Some(boxes) = self.player_ui_boxes_mut() {
                if let Some(b) = Self::find_box_by_id_mut(boxes, id) {
                    let start =
                        rer_engine_core::player_ui::text_input::hit_test::char_index_to_byte(
                            &b.text,
                            caret - 1,
                        );
                    let end = rer_engine_core::player_ui::text_input::hit_test::char_index_to_byte(
                        &b.text, caret,
                    );
                    b.text.replace_range(start..end, "");
                    updated = Some(b.text.clone());
                }
            }
            if updated.is_some() {
                self.player_ui_text_caret = caret - 1;
            }
            if let Some(text) = updated {
                self.player_ui_restart_caret_blink();
                self.rebuild_player_ui_overlay();
                self.emit_player_ui_text_updated(id, &text);
            }
        }

        fn player_ui_text_delete_after_caret(&mut self, id: u32) {
            let caret = self.player_ui_text_caret;
            let mut updated: Option<String> = None;
            if let Some(boxes) = self.player_ui_boxes_mut() {
                if let Some(b) = Self::find_box_by_id_mut(boxes, id) {
                    let char_count = b.text.chars().count();
                    if caret >= char_count {
                        return;
                    }
                    let start =
                        rer_engine_core::player_ui::text_input::hit_test::char_index_to_byte(
                            &b.text, caret,
                        );
                    let end = rer_engine_core::player_ui::text_input::hit_test::char_index_to_byte(
                        &b.text,
                        caret + 1,
                    );
                    b.text.replace_range(start..end, "");
                    updated = Some(b.text.clone());
                }
            }
            if let Some(text) = updated {
                self.player_ui_restart_caret_blink();
                self.rebuild_player_ui_overlay();
                self.emit_player_ui_text_updated(id, &text);
            }
        }

        pub(crate) fn player_ui_text_ime_commit(&mut self, text: &str) -> bool {
            if !self.player_ui_edit_active {
                return false;
            }
            let Some(edit_id) = self.player_ui_text_editing_id else {
                return false;
            };
            if text.is_empty() {
                return true;
            }
            self.player_ui_text_insert_at_caret(edit_id, text);
            true
        }

        fn player_ui_text_insert_at_caret(&mut self, id: u32, s: &str) {
            let caret = self.player_ui_text_caret;
            let mut updated: Option<String> = None;
            let mut inserted_chars = 0usize;
            if let Some(boxes) = self.player_ui_boxes_mut() {
                if let Some(b) = Self::find_box_by_id_mut(boxes, id) {
                    let byte_start =
                        rer_engine_core::player_ui::text_input::hit_test::char_index_to_byte(
                            &b.text, caret,
                        );
                    let mut chunk = String::new();
                    for ch in s.chars() {
                        if ch == '\n' || ch == '\r' || ch == '\u{8}' || ch == '\u{7f}' {
                            continue;
                        }
                        if b.text.chars().count() + chunk.chars().count() >= MAX_TEXT_CHARS {
                            break;
                        }
                        chunk.push(ch);
                    }
                    if !chunk.is_empty() {
                        inserted_chars = chunk.chars().count();
                        let mut new_text = String::with_capacity(b.text.len() + chunk.len());
                        new_text.push_str(&b.text[..byte_start]);
                        new_text.push_str(&chunk);
                        new_text.push_str(&b.text[byte_start..]);
                        b.text = new_text;
                        updated = Some(b.text.clone());
                    }
                }
            }
            if let Some(text) = updated {
                self.player_ui_text_caret = caret + inserted_chars;
                self.player_ui_clamp_caret_for_box(&text);
                self.player_ui_restart_caret_blink();
                self.rebuild_player_ui_overlay();
                self.emit_player_ui_text_updated(id, &text);
            }
        }
    };
}
