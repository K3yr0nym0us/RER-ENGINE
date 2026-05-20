use glam::Vec3 as GlamVec3;

use crate::config_shared::point_to_segment_2d;
use crate::ecs::{EntityId, Transform};
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};

impl State {
    // ── Proyeccion 2D a pantalla ──────────────────────────────────────────────

    /// Proyecta un punto de mundo XY a coordenadas de pantalla en píxeles.
    pub(crate) fn project_to_screen_2d(&self, cam: &super::Camera2D, p: GlamVec3) -> Option<(f32, f32)> {
        let w  = self.size.width  as f32;
        let h  = self.size.height as f32;
        let vp = cam.view_proj(w / h);
        let c  = vp * glam::Vec4::new(p.x, p.y, p.z, 1.0);
        if c.w.abs() < 1e-6 { return None; }
        Some(((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
    }

    // ── Picking 2D ────────────────────────────────────────────────────────────

    /// Selecciona la entidad bajo el cursor usando AABB en el plano XY.
    /// Cuando varios AABBs se solapan (p.ej. escenario + player) se elige
    /// la entidad con mayor Z (más cercana a la cámara).
    pub fn pick_entity_2d(&mut self, pixel_x: f32, pixel_y: f32) {
        let Some((wx, wy)) = self.screen_to_world_2d(pixel_x, pixel_y) else { return };

        // Mantener el picking del editor basado en Transform crudo.
        // El render puede aplicar `visual_offsets`, pero igualar ambas referencias
        // aqui cambiaria la seleccion actual de escenas/animaciones existentes.
        // Esa unificacion queda como trabajo separado de cambio de comportamiento.
        // Recoge todos los hits y elige el de mayor Z (más cercano a la cámara).
        let mut best: Option<(EntityId, f32)> = None;
        for &entity in self.world.entities() {
            if self.world.has::<crate::ecs::NonSelectable>(entity) { continue; }
            if let Some(transform) = self.world.get::<Transform>(entity) {
                let p  = transform.position;
                let sx = transform.scale.x * 0.5;
                let sy = transform.scale.y * 0.5;
                if wx >= p.x - sx && wx <= p.x + sx && wy >= p.y - sy && wy <= p.y + sy {
                    if best.map_or(true, |(_, bz)| p.z > bz) {
                        best = Some((entity, p.z));
                    }
                }
            }
        }
        let hit = best.map(|(id, _)| id);
        match hit {
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
                            let active_name      = self.world.name(active_id).unwrap_or("Entity").to_string();
                            let active_transform = self.world.get::<Transform>(active_id).cloned().unwrap_or_default();
                            let active_pos = active_transform.position.to_array();
                            let active_rot = [active_transform.rotation.x, active_transform.rotation.y,
                                              active_transform.rotation.z, active_transform.rotation.w];
                            let active_scl             = active_transform.scale.to_array();
                            let physics_enabled = self.physics_2d.has_physics(active_id);
                            let physics_type    = self.physics_2d.get_body_type(active_id).to_string();
                            send_event(&EngineEvent::EntitySelected {
                                id: active_id, name: active_name, position: active_pos, rotation: active_rot, scale: active_scl,
                                physics_enabled,
                                physics_type,
                            });
                        }
                        send_event(&EngineEvent::MultiSelectChanged { ids: self.selected_entities.clone() });
                        return;
                    } else {
                        self.selected_entities.push(entity);
                        self.selected_entity = Some(entity);
                    }
                } else {
                    if self.selected_entity == Some(entity)
                        && self.selected_entities.len() == 1
                        && self.selected_entities[0] == entity {
                        return;
                    }
                    self.selected_entities.clear();
                    self.selected_entities.push(entity);
                    self.selected_entity = Some(entity);
                }
                let name      = self.world.name(entity).unwrap_or("Entity").to_string();
                let transform = self.world.get::<Transform>(entity).cloned().unwrap_or_default();
                let pos = transform.position.to_array();
                let rot = [transform.rotation.x, transform.rotation.y,
                           transform.rotation.z, transform.rotation.w];
                let scl             = transform.scale.to_array();
                let physics_enabled = self.physics_2d.has_physics(entity);
                let physics_type    = self.physics_2d.get_body_type(entity).to_string();
                send_event(&EngineEvent::EntitySelected {
                    id: entity, name, position: pos, rotation: rot, scale: scl,
                    physics_enabled,
                    physics_type,
                });
                if self.ctrl_held && self.selected_entities.len() > 1 {
                    send_event(&EngineEvent::MultiSelectChanged { ids: self.selected_entities.clone() });
                }
            }
            None => {
                if !self.ctrl_held && (self.selected_entity.is_some() || !self.selected_entities.is_empty()) {
                    self.selected_entity = None;
                    self.selected_entities.clear();
                    send_event(&EngineEvent::EntityDeselected);
                }
            }
        }
    }

    // ── Picking de eje del gizmo 2D ───────────────────────────────────────────

    /// Devuelve el índice del eje del gizmo 2D más cercano al cursor (0=X, 1=Y).
    pub fn pick_gizmo_axis_2d(&self, pixel_x: f32, pixel_y: f32) -> Option<usize> {
        let origin = self.selection_center()?;
        let cam    = self.camera_2d.as_ref()?;
        let so     = self.project_to_screen_2d(cam, origin)?;

        const LEN:    f32 = 1.2;
        const THRESH: f32 = 16.0;
        let dirs = [GlamVec3::X, GlamVec3::Y];

        let mut best: Option<(f32, usize)> = None;
        for (i, &dir) in dirs.iter().enumerate() {
            if let Some(tip) = self.project_to_screen_2d(cam, origin + dir * LEN) {
                let d = point_to_segment_2d(pixel_x, pixel_y, so.0, so.1, tip.0, tip.1);
                if d < THRESH && best.map_or(true, |(bd, _)| d < bd) {
                    best = Some((d, i));
                }
            }
        }
        best.map(|(_, i)| i)
    }

    // ── Drag de gizmo 2D ──────────────────────────────────────────────────────

    /// Arrastra la entidad seleccionada sobre el eje X (0) o Y (1) en modo 2D.
    pub fn drag_gizmo_2d(&mut self, pixel_x: f32, pixel_y: f32, last_x: f32, last_y: f32, axis_idx: usize, snap: bool) {
        let selected_ids: Vec<EntityId> = if !self.selected_entities.is_empty() {
            self.selected_entities.clone()
        } else {
            self.selected_entity.into_iter().collect()
        };
        if selected_ids.is_empty() { return; }

        let cam = match &self.camera_2d {
            Some(c) => super::Camera2D { x: c.x, y: c.y, half_h: c.half_h, near: c.near, far: c.far },
            None    => return,
        };
        let mut sum = GlamVec3::ZERO;
        let mut count = 0usize;
        for &id in &selected_ids {
            if let Some(t) = self.world.get::<Transform>(id) {
                sum += t.position;
                count += 1;
            }
        }
        if count == 0 { return; }
        let origin = sum / count as f32;

        let axis_world = if axis_idx == 0 { GlamVec3::X } else { GlamVec3::Y };
        let so = match self.project_to_screen_2d(&cam, origin)               { Some(p) => p, None => return };
        let se = match self.project_to_screen_2d(&cam, origin + axis_world)  { Some(p) => p, None => return };
        let ax  = se.0 - so.0;
        let ay  = se.1 - so.1;
        let len = (ax * ax + ay * ay).sqrt();
        if len < 1e-4 { return; }
        let dx = pixel_x - last_x;
        let dy = pixel_y - last_y;
        let world_delta = (dx * ax + dy * ay) / (len * len);
        for &sel_id in &selected_ids {
            if let Some(t) = self.world.get_mut::<Transform>(sel_id) {
                t.position += axis_world * world_delta;
                // Snap a cuadrícula: alinea el borde más cercano a la línea de
                // cuadrícula más próxima. Se activa si snap=true (Ctrl desde
                // cualquier fuente: winit o IPC).
                let cell = self.grid_config.cell_size;
                if snap && cell > 1e-6 {
                    if axis_idx == 0 {
                        let hw = t.scale.x * 0.5;
                        let left  = t.position.x - hw;
                        let right = t.position.x + hw;
                        let left_snap  = (left  / cell).round() * cell;
                        let right_snap = (right / cell).round() * cell;
                        if (left - left_snap).abs() <= (right - right_snap).abs() {
                            t.position.x = left_snap + hw;
                        } else {
                            t.position.x = right_snap - hw;
                        }
                    } else {
                        let hh = t.scale.y * 0.5;
                        let bottom = t.position.y - hh;
                        let top    = t.position.y + hh;
                        let bottom_snap = (bottom / cell).round() * cell;
                        let top_snap    = (top    / cell).round() * cell;
                        if (bottom - bottom_snap).abs() <= (top - top_snap).abs() {
                            t.position.y = bottom_snap + hh;
                        } else {
                            t.position.y = top_snap - hh;
                        }
                    }
                }
            }
        }

        let lead_id = self.selected_entity.or_else(|| selected_ids.last().copied());
        if let Some(sel_id) = lead_id {
            let name = self.world.name(sel_id).unwrap_or("Entity").to_string();
            if let Some(t) = self.world.get::<Transform>(sel_id) {
                let pos = t.position.to_array();
                let rot = [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w];
                let scl             = t.scale.to_array();
                let physics_enabled = self.physics_2d.has_physics(sel_id);
                let physics_type    = self.physics_2d.get_body_type(sel_id).to_string();
                send_event(&EngineEvent::EntitySelected {
                    id: sel_id, name, position: pos, rotation: rot, scale: scl,
                    physics_enabled,
                    physics_type,
                });
            }
        }

        // Sincronizar el Rapier body con la nueva posición visual.
        // Sin esto, el cuerpo físico (y por tanto las colisiones) permanece
        // en la posición original aunque el cuadro visual se haya movido.
        for &sel_id in &selected_ids {
            let new_pos = self.world.get::<Transform>(sel_id)
                .map(|t| (t.position.x, t.position.y));
            if let Some((nx, ny)) = new_pos {
                // Si existe una base de animación guardada para la entidad,
                // mantenerla sincronizada con el drag del gizmo para que
                // play_animation_frame no ancle en una posición antigua.
                if let Some(saved) = self.anim_saved_transforms.get_mut(&sel_id) {
                    saved.0.x = nx;
                    saved.0.y = ny;
                }
                self.sync_physics_2d_body_from_xy(sel_id, nx, ny);
            }
        }

        // Emitir evento con transformaciones de TODAS las entidades en multiselección
        // para sincronizar correctamente entityTransformsRef en el frontend y guardar posiciones.
        if !self.selected_entities.is_empty() {
            let entities: Vec<crate::ipc::EntityTransformUpdate> = selected_ids.iter()
                .filter_map(|&id| {
                    self.world.get::<Transform>(id).map(|t| crate::ipc::EntityTransformUpdate {
                        id,
                        position: t.position.to_array(),
                        rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                        scale: t.scale.to_array(),
                    })
                })
                .collect();
            if !entities.is_empty() {
                send_event(&EngineEvent::MultiSelectionTransformed { entities });
            }
        }
    }

    // ── Hover 2D ─────────────────────────────────────────────────────────────

    /// Actualiza `hovered_entity` y `hovered_gizmo_axis` en modo 2D.
    /// Usa spatial grid para O(k) lookup en lugar de O(n) linear scan.
    pub fn update_hover_2d(&mut self, pixel_x: f32, pixel_y: f32) {
        let prev_hover = self.hovered_entity;
        let Some((wx, wy)) = self.screen_to_world_2d(pixel_x, pixel_y) else { return };

        self.hovered_entity = None;
        let mut best_hover: Option<(EntityId, f32)> = None;

        // Query spatial grid para entidades cerca del cursor
        let candidates = self.spatial_grid.query_cell(wx, wy);
        for entity in candidates {
            if self.world.has::<crate::ecs::NonSelectable>(entity) { continue; }
            if let Some(t) = self.world.get::<Transform>(entity) {
                let sx = t.scale.x * 0.5;
                let sy = t.scale.y * 0.5;
                if wx >= t.position.x - sx && wx <= t.position.x + sx
                && wy >= t.position.y - sy && wy <= t.position.y + sy {
                    if best_hover.map_or(true, |(_, bz)| t.position.z > bz) {
                        best_hover = Some((entity, t.position.z));
                    }
                }
            }
        }

        self.hovered_entity    = best_hover.map(|(id, _)| id);
        self.hovered_gizmo_axis = self.pick_gizmo_axis_2d(pixel_x, pixel_y);
        // Emitir evento solo si el hover cambió para no saturar el IPC
        match (prev_hover, self.hovered_entity) {
            (None, Some(id))              => send_event(&EngineEvent::EntityHovered { id }),
            (Some(_), None)               => send_event(&EngineEvent::EntityUnhovered),
            (Some(a), Some(b)) if a != b  => send_event(&EngineEvent::EntityHovered { id: b }),
            _                             => {}
        }
    }
}
