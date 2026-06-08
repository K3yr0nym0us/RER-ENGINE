//! Macro: impl `State` para edición de texto HUD (parametrizado por motor).

pub mod types;
pub mod hit_test;

pub use types::PlayerUiTextDrag;

/// Genera el bloque `impl State` de edición HUD. `$pu` = módulo `player_ui` del motor; `$ipc` = módulo IPC.
#[macro_export]
macro_rules! impl_player_ui_text_input {
    ($pu:path, $ipc:path) => {
        use std::time::Instant;

        use rer_engine_shared::player_ui::config::{
            self, apply_resize_free, apply_resize_locked_aspect, HasUiHudRect, PlayerUiButton,
            PlayerUiImage, PlayerUiResizeHandle, PlayerUiTextBox, UiHudRect, BOX_STACK_GAP,
        };
        use rer_engine_shared::player_ui::grid::{
            player_ui_grid_steps, snap_ndc_point_to_grid, snap_ui_hud_rect_to_grid,
        };
        use rer_engine_shared::platform::{query_ctrl_held_os, query_shift_held_os};
        use rer_engine_shared::player_ui::text_input::types::{
            PlayerUiHitTarget, PlayerUiTextDrag as PuTextDrag, CARET_BLINK_ON_MS, CARET_BLINK_PERIOD_MS,
            DEFAULT_BOX_H, DEFAULT_BOX_W, DOUBLE_CLICK, DRAG_THRESHOLD_PX, MAX_TEXT_CHARS,
            MIN_BOX_H, MIN_BOX_W,
        };
        use $ipc::{
            send_event, EngineEvent, PlayerUiButtonListItem, PlayerUiImageListItem,
            PlayerUiTextBoxListItem, SavePlayerUiTextBoxSnapshot,
        };
        use $pu::{button, image, object};
        use crate::engine::State;
impl State {
    pub(crate) fn clear_player_ui_text_interaction(&mut self) {
        self.clear_player_ui_hud_selection();
        self.player_ui_text_editing_id = None;
        self.player_ui_text_caret = 0;
        self.player_ui_text_drag = None;
        self.player_ui_last_text_click = None;
    }

    pub(crate) fn clear_player_ui_hud_selection(&mut self) {
        self.player_ui_selected_text_id = None;
        self.player_ui_selected_button_id = None;
        self.player_ui_selected_image_id = None;
        self.player_ui_selected_object_id = None;
    }

    pub(crate) fn player_ui_caret_blink_visible(&self) -> bool {
        self.player_ui_caret_blink_epoch.elapsed().as_millis() as u64 % CARET_BLINK_PERIOD_MS
            < CARET_BLINK_ON_MS
    }

    fn player_ui_restart_caret_blink(&mut self) {
        self.player_ui_caret_blink_epoch = Instant::now();
    }

    fn player_ui_clamp_caret_for_box(&mut self, text: &str) {
        self.player_ui_text_caret = self.player_ui_text_caret.min(text.chars().count());
    }

    fn player_ui_begin_text_edit(&mut self, id: u32) {
        let char_count = self
            .player_ui_boxes_for_id(id)
            .map(|b| b.text.chars().count())
            .unwrap_or(0);
        self.player_ui_text_editing_id = Some(id);
        self.player_ui_text_caret = char_count;
        self.player_ui_restart_caret_blink();
    }

    pub(crate) fn player_ui_boxes_for_id(&self, id: u32) -> Option<&PlayerUiTextBox> {
        let key = self.player_ui_text_key()?;
        self.player_ui_text_boxes
            .get(&key)
            .and_then(|list| list.iter().find(|b| b.id == id))
    }

    pub(crate) fn player_ui_text_key(&self) -> Option<String> {
        self.player_ui_screen_key()
    }

    pub(crate) fn set_player_ui_edit_context(
        &mut self,
        scope: Option<String>,
        screen_id: Option<String>,
    ) {
        self.player_ui_edit_scope = scope;
        self.player_ui_edit_screen_id = screen_id;
        self.clear_player_ui_text_interaction();
        self.rebuild_player_ui_overlay();
    }

    pub(crate) fn add_player_ui_text_box(&mut self, font_path: &str) -> Result<u32, String> {
        if !self.player_ui_edit_active {
            return Err("modo ediciÃ³n UI inactivo".into());
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
        let z_index = rer_engine_shared::player_ui::hud_layers::next_z_index_for_screen(
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
            "[player-ui] cuadro de texto aÃ±adido: id={} fuente={}",
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

    /// Teclas en modo ediciÃ³n UI sin ediciÃ³n de caracteres activa (p. ej. Supr â†’ borrar cuadro).
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

    pub(crate) fn pixel_to_ndc(&self, px: f32, py: f32) -> [f32; 2] {
        rer_engine_shared::player_ui::viewport::pixel_to_ndc(
            self.size.width.max(1) as f32,
            self.size.height.max(1) as f32,
            px,
            py,
        )
    }

    fn player_ui_boxes_mut(&mut self) -> Option<&mut Vec<PlayerUiTextBox>> {
        let key = self.player_ui_text_key()?;
        self.player_ui_text_boxes.get_mut(&key)
    }

    fn player_ui_buttons_mut(&mut self) -> Option<&mut Vec<PlayerUiButton>> {
        let key = self.player_ui_text_key()?;
        self.player_ui_buttons.get_mut(&key)
    }

    fn player_ui_images_mut(&mut self) -> Option<&mut Vec<PlayerUiImage>> {
        let key = self.player_ui_text_key()?;
        self.player_ui_images.get_mut(&key)
    }

    fn player_ui_objects_mut(&mut self) -> Option<&mut Vec<rer_engine_shared::player_ui::config::PlayerUiObject>> {
        let key = self.player_ui_text_key()?;
        self.player_ui_objects.get_mut(&key)
    }

    pub(crate) fn player_ui_object_locked(&self, id: u32) -> bool {
        self.player_ui_text_key()
            .and_then(|key| self.player_ui_objects.get(&key))
            .and_then(|list| list.iter().find(|o| o.id == id))
            .is_some_and(|o| o.locked)
    }

    fn clear_player_ui_selection_except_image(&mut self) {
        self.player_ui_selected_text_id = None;
        self.player_ui_selected_button_id = None;
        self.player_ui_selected_object_id = None;
        self.player_ui_text_editing_id = None;
    }

    fn clear_player_ui_selection_except_button(&mut self) {
        self.player_ui_selected_text_id = None;
        self.player_ui_selected_image_id = None;
        self.player_ui_selected_object_id = None;
        self.player_ui_text_editing_id = None;
    }

    fn clear_player_ui_selection_except_text(&mut self) {
        self.player_ui_selected_button_id = None;
        self.player_ui_selected_image_id = None;
        self.player_ui_selected_object_id = None;
    }

    fn clear_player_ui_selection_except_object(&mut self) {
        self.player_ui_selected_text_id = None;
        self.player_ui_selected_button_id = None;
        self.player_ui_selected_image_id = None;
        self.player_ui_text_editing_id = None;
    }

    fn find_box_by_id<'a>(boxes: &'a [PlayerUiTextBox], id: u32) -> Option<&'a PlayerUiTextBox> {
        boxes.iter().find(|b| b.id == id)
    }

    fn find_box_by_id_mut<'a>(
        boxes: &'a mut [PlayerUiTextBox],
        id: u32,
    ) -> Option<&'a mut PlayerUiTextBox> {
        boxes.iter_mut().find(|b| b.id == id)
    }

    pub(crate) fn player_ui_mouse_down(&mut self, px: f32, py: f32) -> bool {
        if !self.player_ui_edit_active {
            return false;
        }
        if self.handle_player_ui_object_draw_click(px, py) {
            return true;
        }
        let ndc = self.pixel_to_ndc(px, py);
        let editing_id = self.player_ui_text_editing_id;

        if let Some(edit_id) = editing_id {
            let hit = self
                .player_ui_text_key()
                .map(|key| {
                    self.player_ui_text_boxes
                        .get(&key)
                        .is_some_and(|boxes| rer_engine_shared::player_ui::text_input::hit_test::hit_test_box(boxes, edit_id, ndc))
                })
                .unwrap_or(false);
            if !hit {
                self.player_ui_text_editing_id = None;
                self.rebuild_player_ui_overlay();
            } else {
                return true;
            }
        }

        if let Some(selected_id) = self.player_ui_selected_text_id {
            if !self.player_ui_text_locked(selected_id) {
                if let Some(handle) = self.hit_test_text_handle( selected_id, ndc) {
                if let Some(start_box) = self.player_ui_text_key().and_then(|key| {
                    self.player_ui_text_boxes
                        .get(&key)
                        .and_then(|boxes| Self::find_box_by_id(boxes, selected_id).cloned())
                }) {
                    self.push_undo_player_ui_hud();
                    self.player_ui_text_drag = Some(PuTextDrag::Resize {
                        id: selected_id,
                        handle,
                        start_mouse: ndc,
                        start_box,
                    });
                    self.player_ui_text_editing_id = None;
                    return true;
                }
                }
            }
        }

        if let Some(selected_id) = self.player_ui_selected_image_id {
            if !self.player_ui_image_locked(selected_id) {
                if let Some(handle) = self.hit_test_image_handle( selected_id, ndc) {
                if let Some(start_image) = self.player_ui_text_key().and_then(|key| {
                    self.player_ui_images
                        .get(&key)
                        .and_then(|list| list.iter().find(|i| i.id == selected_id).cloned())
                }) {
                    self.clear_player_ui_selection_except_image();
                    self.push_undo_player_ui_hud();
                    self.player_ui_text_drag = Some(PuTextDrag::ImageResize {
                        id: selected_id,
                        handle,
                        start_mouse: ndc,
                        start_image,
                    });
                    return true;
                }
                }
            }
        }

        if let Some(selected_id) = self.player_ui_selected_button_id {
            if !self.player_ui_button_locked(selected_id) {
                if let Some(handle) = self.hit_test_button_handle( selected_id, ndc) {
                let resize_start = self.player_ui_text_key().and_then(|key| {
                    self.player_ui_buttons
                        .get(&key)
                        .and_then(|list| list.iter().find(|b| b.id == selected_id))
                        .map(|btn| (btn.ui_hud_rect(), btn.source_aspect))
                });
                if let Some((start_rect, source_aspect)) = resize_start {
                    self.clear_player_ui_selection_except_button();
                    self.push_undo_player_ui_hud();
                    self.player_ui_text_drag = Some(PuTextDrag::ButtonResize {
                        id: selected_id,
                        handle,
                        start_mouse: ndc,
                        start_rect,
                        source_aspect,
                    });
                    return true;
                }
                }
            }
        }

        if let Some(target) = self.hit_test_top_hud(ndc) {
            match target {
                PlayerUiHitTarget::Image(id) => {
                    self.player_ui_selected_image_id = Some(id);
                    self.clear_player_ui_selection_except_image();
                    if !self.player_ui_image_locked(id) {
                        let start_center = self
                            .player_ui_text_key()
                            .and_then(|key| {
                                self.player_ui_images.get(&key).and_then(|list| {
                                    list.iter()
                                        .find(|i| i.id == id)
                                        .map(|i| [i.center_x, i.center_y])
                                })
                            })
                            .unwrap_or([0.0, 0.0]);
                        self.push_undo_player_ui_hud();
                        self.player_ui_text_drag = Some(PuTextDrag::ImageMove {
                            id,
                            start_mouse: ndc,
                            start_center,
                        });
                    }
                    self.rebuild_player_ui_overlay();
                    return true;
                }
                PlayerUiHitTarget::Button(id) => {
                    self.player_ui_selected_button_id = Some(id);
                    self.clear_player_ui_selection_except_button();
                    if !self.player_ui_button_locked(id) {
                        let start_center = self
                            .player_ui_text_key()
                            .and_then(|key| {
                                self.player_ui_buttons.get(&key).and_then(|list| {
                                    list.iter()
                                        .find(|b| b.id == id)
                                        .map(|b| [b.center_x, b.center_y])
                                })
                            })
                            .unwrap_or([0.0, 0.0]);
                        self.push_undo_player_ui_hud();
                        self.player_ui_text_drag = Some(PuTextDrag::ButtonMove {
                            id,
                            start_mouse: ndc,
                            start_center,
                        });
                    }
                    self.player_ui_text_editing_id = None;
                    self.rebuild_player_ui_overlay();
                    return true;
                }
                PlayerUiHitTarget::Object(id) => {
                    self.player_ui_selected_object_id = Some(id);
                    self.clear_player_ui_selection_except_object();
                    if !self.player_ui_object_locked(id) {
                        if let Some(start_vertices) = self.player_ui_text_key().and_then(|key| {
                            self.player_ui_objects.get(&key).and_then(|list| {
                                list.iter()
                                    .find(|o| o.id == id)
                                    .map(|o| o.vertices.clone())
                            })
                        }) {
                            self.push_undo_player_ui_hud();
                            self.player_ui_text_drag = Some(PuTextDrag::ObjectMove {
                                id,
                                start_mouse: ndc,
                                start_vertices,
                            });
                        }
                    }
                    self.player_ui_text_editing_id = None;
                    self.rebuild_player_ui_overlay();
                    return true;
                }
                PlayerUiHitTarget::Text(id) => {
                    self.player_ui_selected_text_id = Some(id);
                    self.clear_player_ui_selection_except_text();
                    if !self.player_ui_text_locked(id) {
                        let start_center = self
                            .player_ui_text_key()
                            .and_then(|key| {
                                self.player_ui_text_boxes.get(&key).and_then(|boxes| {
                                    Self::find_box_by_id(boxes, id)
                                        .map(|b| [b.center_x, b.center_y])
                                })
                            })
                            .unwrap_or([0.0, 0.0]);
                        self.push_undo_player_ui_hud();
                        self.player_ui_text_drag = Some(PuTextDrag::Move {
                            id,
                            start_mouse: ndc,
                            start_center,
                        });
                    }
                    self.player_ui_text_editing_id = None;
                    self.rebuild_player_ui_overlay();
                    return true;
                }
            }
        }

        self.clear_player_ui_hud_selection();
        self.player_ui_text_editing_id = None;
        self.player_ui_text_drag = None;
        self.rebuild_player_ui_overlay();
        true
    }

    fn player_ui_shift_active(&self) -> bool {
        self.shift_held || query_shift_held_os()
    }

    fn player_ui_ctrl_active(&self) -> bool {
        self.ctrl_held || query_ctrl_held_os()
    }

    /// Fija `source_aspect` al tamaÃ±o NDC actual para que resize sin Shift use esa proporciÃ³n.
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

    /// Al soltar Shift: guardar la proporciÃ³n actual y seguir en modo proporcional.
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

    pub(crate) fn player_ui_mouse_move(&mut self, px: f32, py: f32) -> bool {
        if !self.player_ui_edit_active {
            return false;
        }
        if self.player_ui_object_draw_active() {
            self.update_player_ui_object_draw_cursor(px, py);
            return true;
        }
        let Some(drag) = self.player_ui_text_drag.clone() else {
            return false;
        };
        let ndc = self.pixel_to_ndc(px, py);
        let vw = self.size.width.max(1) as f32;
        let vh = self.size.height.max(1) as f32;
        let shift = self.player_ui_shift_active();
        let snap = self.player_ui_ctrl_active();

        match drag {
            PuTextDrag::ImageMove {
                id,
                start_mouse,
                start_center,
            } => {
                let dx = ndc[0] - start_mouse[0];
                let dy = ndc[1] - start_mouse[1];
                if let Some(images) = self.player_ui_images_mut() {
                    if let Some(img) = images.iter_mut().find(|i| i.id == id) {
                        img.center_x = start_center[0] + dx;
                        img.center_y = start_center[1] + dy;
                        if snap {
                            let (cx, cy) = rer_engine_shared::player_ui::text_input::hit_test::snap_player_ui_element_move(
                                img.center_x,
                                img.center_y,
                                img.width,
                                img.height,
                                vw,
                                vh,
                            );
                            img.center_x = cx;
                            img.center_y = cy;
                        }
                    }
                }
            }
            PuTextDrag::ImageResize {
                id,
                handle,
                start_mouse,
                start_image,
            } => {
                if let Some(images) = self.player_ui_images_mut() {
                    if let Some(img) = images.iter_mut().find(|i| i.id == id) {
                        let start_rect = start_image.ui_hud_rect();
                        let mut rect = start_rect;
                        if shift {
                            apply_resize_free(
                                &mut rect,
                                start_rect,
                                handle,
                                start_mouse,
                                ndc,
                                MIN_BOX_W,
                                MIN_BOX_H,
                            );
                        } else {
                            apply_resize_locked_aspect(
                                &mut rect,
                                start_rect,
                                handle,
                                start_mouse,
                                ndc,
                                start_image.source_aspect,
                                vw,
                                vh,
                                MIN_BOX_W,
                            );
                        }
                        img.center_x = rect.center_x;
                        img.center_y = rect.center_y;
                        img.width = rect.width;
                        img.height = rect.height;
                    }
                }
            }
            PuTextDrag::ButtonResize {
                id,
                handle,
                start_mouse,
                start_rect,
                source_aspect,
            } => {
                if let Some(buttons) = self.player_ui_buttons_mut() {
                    if let Some(b) = buttons.iter_mut().find(|b| b.id == id) {
                        let mut rect = start_rect;
                        apply_resize_locked_aspect(
                            &mut rect,
                            start_rect,
                            handle,
                            start_mouse,
                            ndc,
                            source_aspect,
                            vw,
                            vh,
                            MIN_BOX_W,
                        );
                        b.center_x = rect.center_x;
                        b.center_y = rect.center_y;
                        b.width = rect.width;
                        b.height = rect.height;
                    }
                }
            }
            PuTextDrag::ButtonMove {
                id,
                start_mouse,
                start_center,
            } => {
                let dx = ndc[0] - start_mouse[0];
                let dy = ndc[1] - start_mouse[1];
                if let Some(buttons) = self.player_ui_buttons_mut() {
                    if let Some(b) = buttons.iter_mut().find(|b| b.id == id) {
                        b.center_x = start_center[0] + dx;
                        b.center_y = start_center[1] + dy;
                        if snap {
                            let (cx, cy) = rer_engine_shared::player_ui::text_input::hit_test::snap_player_ui_element_move(
                                b.center_x,
                                b.center_y,
                                b.width,
                                b.height,
                                vw,
                                vh,
                            );
                            b.center_x = cx;
                            b.center_y = cy;
                        }
                    }
                }
            }
            PuTextDrag::Move {
                id,
                start_mouse,
                start_center,
            } => {
                let dx = ndc[0] - start_mouse[0];
                let dy = ndc[1] - start_mouse[1];
                if let Some(boxes) = self.player_ui_boxes_mut() {
                    if let Some(b) = Self::find_box_by_id_mut(boxes, id) {
                        b.center_x = start_center[0] + dx;
                        b.center_y = start_center[1] + dy;
                        if snap {
                            let (cx, cy) = rer_engine_shared::player_ui::text_input::hit_test::snap_player_ui_element_move(
                                b.center_x,
                                b.center_y,
                                b.width,
                                b.height,
                                vw,
                                vh,
                            );
                            b.center_x = cx;
                            b.center_y = cy;
                        }
                    }
                }
            }
            PuTextDrag::Resize {
                id,
                handle,
                start_mouse,
                start_box,
            } => {
                if let Some(boxes) = self.player_ui_boxes_mut() {
                    if let Some(b) = Self::find_box_by_id_mut(boxes, id) {
                        rer_engine_shared::player_ui::text_input::hit_test::apply_resize(b, &start_box, handle, start_mouse, ndc);
                    }
                }
            }
            PuTextDrag::ObjectMove {
                id,
                start_mouse,
                start_vertices,
            } => {
                let dx = ndc[0] - start_mouse[0];
                let dy = ndc[1] - start_mouse[1];
                let (step_x, step_y) = player_ui_grid_steps(vw, vh);
                let (ox, oy) = if snap {
                    let c0 = rer_engine_shared::player_ui::geometry::polygon_centroid(&start_vertices);
                    let target = [c0[0] + dx, c0[1] + dy];
                    let snapped = snap_ndc_point_to_grid(target[0], target[1], step_x, step_y);
                    (snapped[0] - c0[0], snapped[1] - c0[1])
                } else {
                    (dx, dy)
                };
                if let Some(objects) = self.player_ui_objects_mut() {
                    if let Some(obj) = objects.iter_mut().find(|o| o.id == id) {
                        obj.vertices = start_vertices
                            .iter()
                            .map(|v| [v[0] + ox, v[1] + oy])
                            .collect();
                    }
                }
            }
        }
        self.rebuild_player_ui_overlay_live();
        true
    }

    pub(crate) fn player_ui_mouse_up(
        &mut self,
        px: f32,
        py: f32,
        press_px: f32,
        press_py: f32,
    ) -> bool {
        if !self.player_ui_edit_active {
            return false;
        }

        let drag = self.player_ui_text_drag.clone();
        let was_drag = self.player_ui_text_drag.take().is_some();
        let dx = (px - press_px).abs();
        let dy = (py - press_py).abs();
        let is_click = dx < DRAG_THRESHOLD_PX && dy < DRAG_THRESHOLD_PX;

        if was_drag && !is_click {
            if let Some(PuTextDrag::ImageResize { id, .. }) = drag {
                self.commit_player_ui_image_display_aspect(id);
            }
            self.rebuild_player_ui_overlay();
            return true;
        }

        if !is_click {
            return true;
        }

        let ndc = self.pixel_to_ndc(px, py);
        if let Some(target) = self.hit_test_top_hud(ndc) {
            match target {
                PlayerUiHitTarget::Image(id) => {
                    self.player_ui_selected_image_id = Some(id);
                    self.clear_player_ui_selection_except_image();
                    self.player_ui_last_text_click = None;
                    self.rebuild_player_ui_overlay();
                    return true;
                }
                PlayerUiHitTarget::Button(id) => {
                    self.player_ui_selected_button_id = Some(id);
                    self.clear_player_ui_selection_except_button();
                    self.player_ui_text_editing_id = None;
                    self.player_ui_last_text_click = None;
                    self.rebuild_player_ui_overlay();
                    return true;
                }
                PlayerUiHitTarget::Object(id) => {
                    self.player_ui_selected_object_id = Some(id);
                    self.clear_player_ui_selection_except_object();
                    self.player_ui_last_text_click = None;
                    self.rebuild_player_ui_overlay();
                    return true;
                }
                PlayerUiHitTarget::Text(id) => {
                    let now = Instant::now();
                    let double = self.player_ui_last_text_click.is_some_and(
                        |(last_id, t)| last_id == id && now.duration_since(t) <= DOUBLE_CLICK,
                    );
                    self.player_ui_last_text_click = Some((id, now));
                    self.player_ui_selected_text_id = Some(id);
                    self.clear_player_ui_selection_except_text();
                    if double {
                        self.player_ui_begin_text_edit(id);
                        log::info!("[player-ui] ediciÃ³n de texto: id={id}");
                    }
                    self.rebuild_player_ui_overlay();
                    return true;
                }
            }
        }

        self.player_ui_last_text_click = None;
        true
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
                let start = rer_engine_shared::player_ui::text_input::hit_test::char_index_to_byte(&b.text, caret - 1);
                let end = rer_engine_shared::player_ui::text_input::hit_test::char_index_to_byte(&b.text, caret);
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
                let start = rer_engine_shared::player_ui::text_input::hit_test::char_index_to_byte(&b.text, caret);
                let end = rer_engine_shared::player_ui::text_input::hit_test::char_index_to_byte(&b.text, caret + 1);
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
                let byte_start = rer_engine_shared::player_ui::text_input::hit_test::char_index_to_byte(&b.text, caret);
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
                    let mut new_text =
                        String::with_capacity(b.text.len() + chunk.len());
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
            self.player_ui_text_next_id = self
                .player_ui_text_next_id
                .max(max_id.saturating_add(1));
        }
        if self.player_ui_edit_active {
            self.rebuild_player_ui_overlay();
        }
        log::info!(
            "[player-ui] importados {} cuadros de texto desde .save",
            boxes.len()
        );
    }

    pub(crate) fn draw_player_ui_text_boxes(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) -> u32 {
        self.draw_player_ui_hud(enc, view)
    }

    fn hit_test_text_handle(&self, selected_id: u32, ndc: [f32; 2]) -> Option<rer_engine_shared::player_ui::config::PlayerUiResizeHandle> {
        let key = self.player_ui_text_key()?;
        let boxes = self.player_ui_text_boxes.get(&key)?;
        rer_engine_shared::player_ui::text_input::hit_test::hit_test_text_handle(boxes, selected_id, ndc)
    }

    fn hit_test_image_handle(&self, selected_id: u32, ndc: [f32; 2]) -> Option<rer_engine_shared::player_ui::config::PlayerUiResizeHandle> {
        let key = self.player_ui_text_key()?;
        let list = self.player_ui_images.get(&key)?;
        rer_engine_shared::player_ui::text_input::hit_test::hit_test_image_handle(list, selected_id, ndc)
    }

    fn hit_test_button_handle(&self, selected_id: u32, ndc: [f32; 2]) -> Option<rer_engine_shared::player_ui::config::PlayerUiResizeHandle> {
        let key = self.player_ui_text_key()?;
        let list = self.player_ui_buttons.get(&key)?;
        rer_engine_shared::player_ui::text_input::hit_test::hit_test_button_handle(list, selected_id, ndc)
    }

    fn hit_test_top_hud(&self, ndc: [f32; 2]) -> Option<rer_engine_shared::player_ui::text_input::types::PlayerUiHitTarget> {
        let key = self.player_ui_text_key()?;
        let texts = self.player_ui_text_boxes.get(&key).map(|v| v.as_slice()).unwrap_or(&[]);
        let buttons = self.player_ui_buttons.get(&key).map(|v| v.as_slice()).unwrap_or(&[]);
        let images = self.player_ui_images.get(&key).map(|v| v.as_slice()).unwrap_or(&[]);
        let objects = self.player_ui_objects.get(&key).map(|v| v.as_slice()).unwrap_or(&[]);
        rer_engine_shared::player_ui::text_input::hit_test::hit_test_top_hud(texts, buttons, images, objects, ndc)
    }}
    };
}
