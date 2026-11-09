//! Arrastre libre, traslación y rotación de la selección con gizmo.

use super::*;

impl State {
    pub(crate) fn apply_selection_translation_delta(
        &mut self,
        selected_ids: &[EntityId],
        delta: GlamVec3,
        snap: bool,
        sync_camera_focus: bool,
    ) {
        if selected_ids.is_empty() || delta.length_squared() < 1e-12 {
            return;
        }

        let cell = if snap {
            self.grid_config.cell_size.max(0.05)
        } else {
            0.0
        };

        for &sel_id in selected_ids {
            if let Some(t) = self.world.get_mut::<Transform>(sel_id) {
                t.position += delta;
                if cell > 0.0 {
                    t.position = editor_viewport_controls::snap_vec3_to_grid(t.position, cell);
                }
            }
            if let Some(t) = self.world.get::<Transform>(sel_id).cloned() {
                if self.is_plane_wall_entity(sel_id) && self.collider_entities.contains(&sel_id) {
                    self.sync_plane_wall_physics(sel_id);
                } else if self.physics.has_physics(sel_id) {
                    let half = [
                        (t.scale.x * 0.5).max(0.01),
                        (t.scale.y * 0.5).max(0.01),
                        (t.scale.z * 0.5).max(0.01),
                    ];
                    let pos = t.position.to_array();
                    let model_path = self
                        .save_registry
                        .meta
                        .get(&sel_id)
                        .map(|m| m.path.as_str())
                        .unwrap_or("");
                    let body_pos = physics_body_position_for_model_path(model_path, pos, half);
                    self.physics
                        .sync_entity_physics_from_transform(sel_id, body_pos, half);
                }
            }
        }

        if selected_ids.iter().any(|id| self.sun_entity == Some(*id)) {
            self.sync_directional_light_from_sun();
        }
        if !self.is_play_controller_active() && sync_camera_focus {
            self.sync_editor_camera_focus();
        }

        if let Some(player_id) = self.play_character_entity
            && selected_ids.contains(&player_id)
        {
            self.emit_play_character_view_changed(false);
        }

        let lead_id = self
            .selected_entity
            .or_else(|| selected_ids.last().copied());
        if let Some(sel_id) = lead_id
            && self.world.get::<Transform>(sel_id).is_some()
        {
            self.send_entity_selected_event(sel_id);
        }

        self.handle_entity_attachment_after_transform(selected_ids);
        self.notify_reflection_probe_transform_changed(selected_ids);
    }

    pub(crate) fn apply_selection_rotation_from_snapshots(
        &mut self,
        pivot: GlamVec3,
        snapshots: &[crate::engine3d::engine::types::EntityTransformSnapshot],
        delta_quat: glam::Quat,
    ) {
        if snapshots.is_empty() {
            return;
        }

        let selected_ids: Vec<EntityId> = snapshots.iter().map(|(id, _, _, _)| *id).collect();
        for &(id, start_pos, start_rot, start_scl) in snapshots {
            if self.editor_camera_entity == Some(id) {
                continue;
            }
            let start_pos = GlamVec3::from_array(start_pos);
            let start_rot =
                glam::Quat::from_xyzw(start_rot[0], start_rot[1], start_rot[2], start_rot[3]);
            let start_scale = GlamVec3::from_array(start_scl);
            let start_transform = Transform {
                position: start_pos,
                rotation: start_rot,
                scale: start_scale,
            };
            let start_visual = self.entity_world_pick_aabb(id, &start_transform).0;
            let model_path = self.entity_asset_path_for_bounds(id).unwrap_or_default();
            let bounds = self.resolve_entity_visual_local_bounds(id);
            let (new_pos, new_rot) = rotate_entity_transform_about_visual_center(
                &start_transform,
                start_visual,
                pivot,
                delta_quat,
                model_path.as_str(),
                bounds,
            );

            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = new_pos;
                t.rotation = new_rot;
            }
            if self.is_plane_wall_entity(id) && self.collider_entities.contains(&id) {
                self.sync_plane_wall_physics(id);
            } else if self.physics.has_physics(id) {
                self.sync_entity_physics_collider(id);
            }
            if self.sun_entity == Some(id) {
                self.sync_directional_light_from_sun();
            }
        }

        let lead_id = self
            .selected_entity
            .or_else(|| selected_ids.last().copied());
        if let Some(sel_id) = lead_id
            && self.world.get::<Transform>(sel_id).is_some()
        {
            self.send_entity_selected_event(sel_id);
        }

        self.handle_entity_attachment_after_transform(&selected_ids);
        self.notify_reflection_probe_transform_changed(&selected_ids);
    }

    pub(crate) fn apply_selection_translation_from_snapshots(
        &mut self,
        snapshots: &[crate::engine3d::engine::types::EntityTransformSnapshot],
        delta: GlamVec3,
        snap: bool,
    ) {
        if snapshots.is_empty() || delta.length_squared() < 1e-12 {
            return;
        }

        let cell = if snap {
            self.grid_config.cell_size.max(0.05)
        } else {
            0.0
        };

        let selected_ids: Vec<EntityId> = snapshots.iter().map(|(id, _, _, _)| *id).collect();
        for &(id, start_pos, start_rot, start_scl) in snapshots {
            if self.editor_camera_entity == Some(id) {
                continue;
            }
            let mut new_pos = GlamVec3::from_array(start_pos) + delta;
            if cell > 0.0 {
                new_pos = editor_viewport_controls::snap_vec3_to_grid(new_pos, cell);
            }
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = new_pos;
                t.rotation =
                    glam::Quat::from_xyzw(start_rot[0], start_rot[1], start_rot[2], start_rot[3]);
                t.scale = GlamVec3::from_array(start_scl);
            }
            if self.is_plane_wall_entity(id) && self.collider_entities.contains(&id) {
                self.sync_plane_wall_physics(id);
            } else if self.physics.has_physics(id) {
                self.sync_entity_physics_collider(id);
            }
            if self.sun_entity == Some(id) {
                self.sync_directional_light_from_sun();
            }
        }

        if let Some(player_id) = self.play_character_entity
            && selected_ids.contains(&player_id)
        {
            self.emit_play_character_view_changed(false);
        }

        let lead_id = self
            .selected_entity
            .or_else(|| selected_ids.last().copied());
        if let Some(sel_id) = lead_id
            && self.world.get::<Transform>(sel_id).is_some()
        {
            self.send_entity_selected_event(sel_id);
        }

        self.handle_entity_attachment_after_transform(&selected_ids);
        self.notify_reflection_probe_transform_changed(&selected_ids);
    }

    pub fn drag_entity_free(
        &mut self,
        pixel_x: f32,
        pixel_y: f32,
        plane_point: GlamVec3,
        plane_normal: GlamVec3,
        last_world: &mut GlamVec3,
        shift_held: bool,
        ctrl_held: bool,
    ) {
        let selected_ids = self.selected_entity_ids();
        if selected_ids.is_empty() {
            return;
        }
        let Some(hit) = self.free_drag_world_point(pixel_x, pixel_y, plane_point, plane_normal)
        else {
            return;
        };

        let mut delta = hit - *last_world;
        let max_delta = self.editor_viewport_drag_max_delta();
        if delta.length() > max_delta {
            // Resincronizar sin teletransportar (p. ej. tras perder frames de intersección).
            *last_world = hit;
            return;
        }

        if shift_held {
            delta *= editor_viewport_controls::DRAG_PRECISION_FACTOR;
            *last_world += delta;
        } else {
            *last_world = hit;
        }

        delta = editor_viewport_controls::clamp_drag_delta(delta, max_delta);

        self.apply_selection_translation_delta(&selected_ids, delta, ctrl_held, false);
    }

    pub fn drag_gizmo(
        &mut self,
        pixel_x: f32,
        pixel_y: f32,
        last_x: f32,
        last_y: f32,
        axis_idx: usize,
        shift_held: bool,
        ctrl_held: bool,
    ) {
        let selected_ids = self.selected_entity_ids();
        if selected_ids.is_empty() || axis_idx > 2 {
            return;
        }

        let origin = match self.selection_center() {
            Some(c) => c,
            None => return,
        };

        let w = self.size.width as f32;
        let h = self.size.height as f32;
        let aspect = w / h;

        let vp = self.camera.proj_matrix(aspect) * self.camera_view_matrix();
        let axis_world = [GlamVec3::X, GlamVec3::Y, GlamVec3::Z][axis_idx];

        let project = |p: GlamVec3| -> Option<(f32, f32)> {
            let c = vp * glam::Vec4::new(p.x, p.y, p.z, 1.0);
            if c.w <= 0.0 {
                return None;
            }
            Some(((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
        };

        let (s0x, s0y) = match project(origin) {
            Some(p) => p,
            None => return,
        };
        let (s1x, s1y) = match project(origin + axis_world) {
            Some(p) => p,
            None => return,
        };

        let ax = s1x - s0x;
        let ay = s1y - s0y;
        let axis_len = (ax * ax + ay * ay).sqrt();
        if axis_len < 1e-4 {
            return;
        }

        let dx = pixel_x - last_x;
        let dy = pixel_y - last_y;
        let mut world_delta = (dx * ax + dy * ay) / (axis_len * axis_len);
        if shift_held {
            world_delta *= editor_viewport_controls::DRAG_PRECISION_FACTOR;
        }

        let axis_delta = axis_world * world_delta;
        let max_delta = self.editor_viewport_drag_max_delta();
        let clamped = editor_viewport_controls::clamp_drag_delta(axis_delta, max_delta);

        self.apply_selection_translation_delta(&selected_ids, clamped, ctrl_held, false);
    }

    pub fn drag_gizmo_rotate(
        &mut self,
        pivot: GlamVec3,
        snapshots: &[crate::engine3d::engine::types::EntityTransformSnapshot],
        start_mouse: (f32, f32),
        current_mouse: (f32, f32),
        axis_idx: usize,
        plane_u: GlamVec3,
        plane_v: GlamVec3,
        shift_held: bool,
        ctrl_held: bool,
    ) {
        if snapshots.is_empty() || axis_idx > 2 {
            return;
        }
        let axis_world = [GlamVec3::X, GlamVec3::Y, GlamVec3::Z][axis_idx];
        let Some(mut angle) = self.rotation_angle_on_axis_plane(
            pivot,
            axis_world,
            start_mouse,
            current_mouse,
            plane_u,
            plane_v,
        ) else {
            return;
        };
        if shift_held {
            angle *= editor_viewport_controls::DRAG_PRECISION_FACTOR;
        }
        let mut delta = glam::Quat::from_axis_angle(axis_world, angle);
        if ctrl_held {
            delta = editor_viewport_controls::snap_rotation_quat(delta);
        }
        self.apply_selection_rotation_from_snapshots(pivot, snapshots, delta);
    }

    pub(crate) fn rotation_angle_on_axis_plane(
        &self,
        pivot: GlamVec3,
        axis: GlamVec3,
        start_mouse: (f32, f32),
        current_mouse: (f32, f32),
        plane_u: GlamVec3,
        plane_v: GlamVec3,
    ) -> Option<f32> {
        let (ro0, rd0) = self.viewport_ray(start_mouse.0, start_mouse.1)?;
        let (ro1, rd1) = self.viewport_ray(current_mouse.0, current_mouse.1)?;
        editor_viewport_controls::rotation_drag_angle(
            ro0, rd0, ro1, rd1, pivot, axis, plane_u, plane_v,
        )
    }

    pub fn toggle_transform_gizmo_mode(&mut self) {
        self.transform_gizmo_mode = self.transform_gizmo_mode.toggle();
        self.hovered_gizmo_axis = None;
        self.active_gizmo_axis = None;
        log::info!(
            "[gizmo] modo {}",
            match self.transform_gizmo_mode {
                TransformGizmoMode::Translate => "traslación",
                TransformGizmoMode::Rotate => "rotación",
            }
        );
    }
}
