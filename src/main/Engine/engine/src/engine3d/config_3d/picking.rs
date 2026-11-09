//! Raycast y selección de entidades y ejes del gizmo.

use super::*;

impl State {
    pub(crate) fn entity_at_pixel(&self, pixel_x: f32, pixel_y: f32) -> Option<EntityId> {
        self.ray_cast(pixel_x, pixel_y)
    }

    pub(crate) fn ray_cast(&self, pixel_x: f32, pixel_y: f32) -> Option<EntityId> {
        use glam::Vec4;

        let w = self.size.width as f32;
        let h = self.size.height as f32;
        let aspect = w / h;

        let ndc_x = (2.0 * pixel_x / w) - 1.0;
        let ndc_y = -(2.0 * pixel_y / h) + 1.0;

        let inv_proj = self.camera.proj_matrix(aspect).inverse();
        let inv_view = self.camera_view_matrix().inverse();

        let clip_dir = Vec4::new(ndc_x, ndc_y, -1.0, 0.0);
        let view_dir = inv_proj * clip_dir;
        let view_dir = Vec4::new(view_dir.x, view_dir.y, -1.0, 0.0);
        let world_dir = (inv_view * view_dir).truncate().normalize();
        let ray_origin = self.camera_world_position();

        let mut closest: Option<(f32, EntityId)> = None;
        for &entity in self.world.entities() {
            if self.world.get::<NonSelectable>(entity).is_some() {
                continue;
            }
            let Some(transform) = self.world.get::<Transform>(entity) else {
                continue;
            };
            let (center, half) = if self.is_reflection_probe_entity(entity) {
                self.reflection_probe_pick_aabb(transform)
            } else if self.world.get::<MeshComponent>(entity).is_none() {
                continue;
            } else {
                self.entity_world_pick_aabb(entity, transform)
            };
            if let Some(t) = Self::ray_intersects_aabb(ray_origin, world_dir, center, half)
                && closest.is_none_or(|(ct, _)| t < ct)
            {
                closest = Some((t, entity));
            }
        }
        closest.map(|(_, id)| id)
    }

    pub fn pick_entity(&mut self, pixel_x: f32, pixel_y: f32) {
        if self.player_ui_edit_active {
            return;
        }
        if self.socket_bone_pick_entity.is_some() {
            let _ = self.try_pick_socket_bone_click(pixel_x, pixel_y);
            return;
        }
        if self.bone_physics_pick_entity.is_some() {
            let _ = self.try_pick_bone_physics_click(pixel_x, pixel_y);
            return;
        }
        match self.ray_cast(pixel_x, pixel_y) {
            Some(entity) => {
                if self.ctrl_held {
                    if let Some(idx) = self.selected_entities.iter().position(|&e| e == entity) {
                        self.selected_entities.swap_remove(idx);
                        if self.selected_entity == Some(entity) {
                            self.selected_entity = self.selected_entities.last().copied();
                        }
                        if self.selected_entities.is_empty() {
                            self.selected_entity = None;
                            send_event(&EngineEvent::EntityDeselected);
                        } else if let Some(active_id) = self.selected_entity {
                            self.send_entity_selected_event(active_id);
                        }
                        send_event(&EngineEvent::MultiSelectChanged {
                            ids: self.selected_entities.clone(),
                        });
                        self.sync_editor_camera_focus();
                        return;
                    } else {
                        self.selected_entities.push(entity);
                        self.selected_entity = Some(entity);
                    }
                } else {
                    if self.selected_entity == Some(entity)
                        && self.selected_entities.len() == 1
                        && self.selected_entities[0] == entity
                    {
                        return;
                    }
                    self.selected_entities.clear();
                    self.selected_entities.push(entity);
                    self.selected_entity = Some(entity);
                }
                self.send_entity_selected_event(entity);
                if self.editor_camera_entity == Some(entity) {
                    self.sync_editor_viewport_from_camera_entity();
                }
                if self.ctrl_held && self.selected_entities.len() > 1 {
                    send_event(&EngineEvent::MultiSelectChanged {
                        ids: self.selected_entities.clone(),
                    });
                }
                self.sync_editor_camera_focus();
            }
            None => {
                if !self.ctrl_held
                    && (self.selected_entity.is_some() || !self.selected_entities.is_empty())
                {
                    self.selected_entity = None;
                    self.selected_entities.clear();
                    send_event(&EngineEvent::EntityDeselected);
                    self.sync_editor_camera_focus();
                }
            }
        }
    }

    pub(crate) fn project_to_screen(&self, p: GlamVec3) -> Option<(f32, f32)> {
        let w = self.size.width as f32;
        let h = self.size.height as f32;
        let vp = self.camera.proj_matrix(w / h) * self.camera_view_matrix();
        let c = vp * glam::Vec4::new(p.x, p.y, p.z, 1.0);
        if c.w <= 0.0 {
            return None;
        }
        Some(((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
    }

    pub fn pick_gizmo_axis(&self, pixel_x: f32, pixel_y: f32) -> Option<usize> {
        if self.socket_bone_pick_entity.is_some() || self.bone_physics_pick_entity.is_some() {
            return None;
        }
        let origin = self.selection_center()?;
        match self.transform_gizmo_mode {
            TransformGizmoMode::Translate => {
                self.pick_translate_gizmo_axis(origin, pixel_x, pixel_y)
            }
            TransformGizmoMode::Rotate => self.pick_rotate_gizmo_axis(origin, pixel_x, pixel_y),
        }
    }

    pub(crate) fn pick_translate_gizmo_axis(
        &self,
        origin: GlamVec3,
        pixel_x: f32,
        pixel_y: f32,
    ) -> Option<usize> {
        let so = self.project_to_screen(origin)?;

        let center_dist = ((pixel_x - so.0).powi(2) + (pixel_y - so.1).powi(2)).sqrt();
        if center_dist < editor_viewport_controls::GIZMO_CENTER_PICK_RADIUS_PX {
            return Some(editor_viewport_controls::GIZMO_CENTER_AXIS);
        }

        let gizmo_scale = self.transform_gizmo_world_scale().unwrap_or(1.0);
        let len = rer_engine_core::gizmo::axis_world_length(gizmo_scale);
        let axis_starts = self
            .transform_gizmo_axis_start_mesh()
            .unwrap_or([0.0, 0.0, 0.0]);
        const THRESH: f32 = 16.0;
        let dirs = [GlamVec3::X, GlamVec3::Y, GlamVec3::Z];

        let mut best: Option<(f32, usize)> = None;
        for (i, &dir) in dirs.iter().enumerate() {
            let start_world = origin + dir * axis_starts[i] * gizmo_scale;
            if let (Some(base), Some(tip)) = (
                self.project_to_screen(start_world),
                self.project_to_screen(start_world + dir * len),
            ) {
                let d = point_to_segment_2d(pixel_x, pixel_y, base.0, base.1, tip.0, tip.1);
                if d < THRESH && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, i));
                }
            }
        }
        best.map(|(_, i)| i)
    }

    pub(crate) fn pick_rotate_gizmo_axis(
        &self,
        origin: GlamVec3,
        pixel_x: f32,
        pixel_y: f32,
    ) -> Option<usize> {
        let gizmo_scale = self.transform_gizmo_rotation_world_scale().unwrap_or(1.0);
        let radius = crate::engine3d::gizmo::GIZMO_ROTATION_RING_RADIUS * gizmo_scale;
        const THRESH: f32 = 18.0;
        const SEGMENTS: u32 = 48;

        let mut best: Option<(f32, usize)> = None;
        for axis in 0..3 {
            let mut prev_screen = None;
            for seg in 0..=SEGMENTS {
                let t = std::f32::consts::TAU * seg as f32 / SEGMENTS as f32;
                let local = crate::engine3d::gizmo::rotation_ring_point(axis, radius, t);
                let world = origin + GlamVec3::from_array(local);
                if let Some(screen) = self.project_to_screen(world) {
                    if let Some((px, py)) = prev_screen {
                        let d = point_to_segment_2d(pixel_x, pixel_y, px, py, screen.0, screen.1);
                        if d < THRESH && best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, axis));
                        }
                    }
                    prev_screen = Some(screen);
                }
            }
        }
        best.map(|(_, i)| i)
    }

    pub(crate) fn is_entity_selected(&self, id: EntityId) -> bool {
        self.selected_entities.contains(&id) || self.selected_entity == Some(id)
    }

    pub(crate) fn selected_entity_at_pixel(&self, pixel_x: f32, pixel_y: f32) -> Option<EntityId> {
        let hit = self.ray_cast(pixel_x, pixel_y)?;
        if self.is_entity_selected(hit) {
            Some(hit)
        } else {
            None
        }
    }

    pub(crate) fn selected_entity_ids(&self) -> Vec<EntityId> {
        if !self.selected_entities.is_empty() {
            self.selected_entities.clone()
        } else {
            self.selected_entity.into_iter().collect()
        }
    }

    pub fn update_hover(&mut self, pixel_x: f32, pixel_y: f32) {
        if self.player_ui_edit_active {
            return;
        }
        if self.socket_bone_pick_entity.is_some() {
            self.update_socket_bone_pick_hover(pixel_x, pixel_y);
            if self.hovered_entity.is_some() {
                self.hovered_entity = None;
                crate::engine3d::ipc::send_event(
                    &crate::engine3d::ipc::EngineEvent::EntityUnhovered,
                );
            }
            self.hovered_gizmo_axis = None;
            return;
        }
        if self.bone_physics_pick_entity.is_some() {
            self.update_bone_physics_pick_hover(pixel_x, pixel_y);
            if self.hovered_entity.is_some() {
                self.hovered_entity = None;
                crate::engine3d::ipc::send_event(
                    &crate::engine3d::ipc::EngineEvent::EntityUnhovered,
                );
            }
            self.hovered_gizmo_axis = None;
            return;
        }
        let prev_hover = self.hovered_entity;
        self.hovered_entity = self.ray_cast(pixel_x, pixel_y);
        self.hovered_gizmo_axis = self.pick_gizmo_axis(pixel_x, pixel_y);
        match (prev_hover, self.hovered_entity) {
            (None, Some(id)) => crate::engine3d::ipc::send_event(
                &crate::engine3d::ipc::EngineEvent::EntityHovered { id },
            ),
            (Some(_), None) => crate::engine3d::ipc::send_event(
                &crate::engine3d::ipc::EngineEvent::EntityUnhovered,
            ),
            (Some(a), Some(b)) if a != b => crate::engine3d::ipc::send_event(
                &crate::engine3d::ipc::EngineEvent::EntityHovered { id: b },
            ),
            _ => {}
        }
    }
}
