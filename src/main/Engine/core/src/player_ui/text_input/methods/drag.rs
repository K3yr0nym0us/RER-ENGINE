//! Ratón: press, move y release con resize/arrastre.
//!
//! Expande los métodos de este grupo dentro del `impl State` del motor.

#[doc(hidden)]
#[macro_export]
macro_rules! __rer_player_ui_text_drag {
    () => {
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
                        .is_some_and(|boxes| rer_engine_core::player_ui::text_input::hit_test::hit_test_box(boxes, edit_id, ndc))
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
                            let (cx, cy) = rer_engine_core::player_ui::text_input::hit_test::snap_player_ui_element_move(
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
                            let (cx, cy) = rer_engine_core::player_ui::text_input::hit_test::snap_player_ui_element_move(
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
                            let (cx, cy) = rer_engine_core::player_ui::text_input::hit_test::snap_player_ui_element_move(
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
                        rer_engine_core::player_ui::text_input::hit_test::apply_resize(b, &start_box, handle, start_mouse, ndc);
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
                    let c0 = rer_engine_core::player_ui::geometry::polygon_centroid(&start_vertices);
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
                        log::info!("[player-ui] edición de texto: id={id}");
                    }
                    self.rebuild_player_ui_overlay();
                    return true;
                }
            }
        }

        self.player_ui_last_text_click = None;
        true
    }
    };
}
