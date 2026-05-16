// ── Lógica exclusiva del modo 3D ─────────────────────────────────────────────
//
// Contiene:
//  · camera_3d        — Camera (órbita) + CameraUniform
//  · first_person     — movimiento y mouse look del runtime 3D
//  · load_model       — carga un .glb/.gltf/.fbx y añade mallas a la escena
//  · ray_cast         — proyecta un rayo desde píxel y devuelve la entidad más cercana
//  · pick_entity      — dispara el picking 3D y emite IPC
//  · project_to_screen — proyecta un punto 3D a píxeles de pantalla
//  · pick_gizmo_axis  — detecta el eje del gizmo más cercano al cursor
//  · drag_gizmo       — arrastra una entidad sobre un eje 3D
//  · update_hover     — actualiza el hover de entidad y gizmo en modo 3D

pub(crate) mod camera_3d;
pub(crate) use camera_3d::Camera;

pub(crate) mod first_person;
pub(crate) mod mesh_3d;
pub(crate) mod physics_3d;
pub(crate) mod world_bounds;
pub(crate) use world_bounds::WorldBounds3D;

use std::path::Path;

use glam::Vec3 as GlamVec3;

use crate::config_shared::point_to_segment_2d;
use crate::ecs::{EntityId, MeshComponent, NonSelectable, Transform};
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};

impl State {
    /// Registra un modelo 3D en el almacén de recursos (sin instanciar en escena).
    pub(crate) fn register_model_asset(&mut self, path: &str, name: &str) {
        if !Path::new(path).is_file() {
            send_event(&EngineEvent::Error {
                message: format!("No se encontró el modelo: {path}"),
            });
            return;
        }
        self.model_store
            .insert(path.to_string(), name.to_string());
        send_event(&EngineEvent::ModelAssetLoaded {
            path: path.to_string(),
            name: name.to_string(),
        });
        log::info!("[model] registrado en recursos: {name} ({path})");
    }

    /// Instancia un modelo 3D en la escena (añade mallas sin limpiar el mundo).
    pub(crate) fn load_model(&mut self, path: &str) {
        match mesh_3d::load_model_file(&self.device, Path::new(path)) {
            Ok(loaded) => {
                let count = loaded.len();
                for part in loaded {
                    let mesh_idx = self.meshes.len();
                    let tex_idx = self.uv_rects.len();
                    self.meshes.push(part.mesh);
                    let uv = self
                        .atlas
                        .pack(&self.queue, &part.rgba, part.width, part.height);
                    self.uv_rects.push(uv);

                    let label = self.next_numbered_entity_name("Mesh");
                    let id = self.world.spawn(Some(&label));
                    self.world.insert(id, MeshComponent { mesh_idx, tex_idx });
                    if let Some(t) = self.world.get_mut::<Transform>(id) {
                        let forward = self.camera.view_forward();
                        let spawn = self.camera.target + forward * 2.5;
                        t.position = glam::Vec3::new(spawn.x, spawn.y.max(0.0), spawn.z);
                    }
                    self.send_model_loaded_event(id, &label);
                }
                log::info!("Modelo cargado: {path} ({count} malla/s)");
            }
            Err(e) => {
                log::error!("Error cargando modelo: {e}");
                send_event(&EngineEvent::Error { message: e });
            }
        }
    }

    fn ray_intersects_aabb(
        origin: GlamVec3,
        dir: GlamVec3,
        center: GlamVec3,
        half: GlamVec3,
    ) -> Option<f32> {
        let min = center - half;
        let max = center + half;
        let mut tmin = f32::NEG_INFINITY;
        let mut tmax = f32::INFINITY;

        let oa = origin.to_array();
        let da = dir.to_array();
        let mna = min.to_array();
        let mxa = max.to_array();
        for i in 0..3 {
            let o = oa[i];
            let d = da[i];
            let mn = mna[i];
            let mx = mxa[i];
            if d.abs() < 1e-8 {
                if o < mn || o > mx {
                    return None;
                }
            } else {
                let inv = 1.0 / d;
                let mut t1 = (mn - o) * inv;
                let mut t2 = (mx - o) * inv;
                if t1 > t2 {
                    std::mem::swap(&mut t1, &mut t2);
                }
                tmin = tmin.max(t1);
                tmax = tmax.min(t2);
                if tmax < tmin {
                    return None;
                }
            }
        }

        let t = if tmin >= 0.0 { tmin } else { tmax };
        if t >= 0.0 { Some(t) } else { None }
    }

    fn ray_cast(&self, pixel_x: f32, pixel_y: f32) -> Option<EntityId> {
        use glam::Vec4;

        let w = self.size.width as f32;
        let h = self.size.height as f32;
        let aspect = w / h;

        let ndc_x = (2.0 * pixel_x / w) - 1.0;
        let ndc_y = -(2.0 * pixel_y / h) + 1.0;

        let inv_proj = self.camera.proj_matrix(aspect).inverse();
        let inv_view = self.camera.view_matrix().inverse();

        let clip_dir = Vec4::new(ndc_x, ndc_y, -1.0, 0.0);
        let view_dir = inv_proj * clip_dir;
        let view_dir = Vec4::new(view_dir.x, view_dir.y, -1.0, 0.0);
        let world_dir = (inv_view * view_dir).truncate().normalize();
        let ray_origin = self.camera.position();

        let mut closest: Option<(f32, EntityId)> = None;
        for &entity in self.world.entities() {
            if self.world.get::<NonSelectable>(entity).is_some()
                || self.world.get::<MeshComponent>(entity).is_none()
            {
                continue;
            }
            let Some(transform) = self.world.get::<Transform>(entity) else {
                continue;
            };
            let half = transform.scale * 0.5;
            if let Some(t) =
                Self::ray_intersects_aabb(ray_origin, world_dir, transform.position, half)
            {
                if closest.map_or(true, |(ct, _)| t < ct) {
                    closest = Some((t, entity));
                }
            }
        }
        closest.map(|(_, id)| id)
    }

    pub fn pick_entity(&mut self, pixel_x: f32, pixel_y: f32) {
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
                            let active_name =
                                self.world.name(active_id).unwrap_or("Entity").to_string();
                            let active_transform = self
                                .world
                                .get::<Transform>(active_id)
                                .cloned()
                                .unwrap_or_default();
                            let active_position = active_transform.position.to_array();
                            let active_rotation = [
                                active_transform.rotation.x,
                                active_transform.rotation.y,
                                active_transform.rotation.z,
                                active_transform.rotation.w,
                            ];
                            let active_scale = active_transform.scale.to_array();
                            let physics_enabled = self.physics.has_physics(active_id);
                            let physics_type = self.physics.get_body_type(active_id).to_string();
                            send_event(&EngineEvent::EntitySelected {
                                id: active_id,
                                name: active_name,
                                position: active_position,
                                rotation: active_rotation,
                                scale: active_scale,
                                physics_enabled,
                                physics_type,
                            });
                        }
                        send_event(&EngineEvent::MultiSelectChanged {
                            ids: self.selected_entities.clone(),
                        });
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
                let name = self.world.name(entity).unwrap_or("Entity").to_string();
                let transform = self.world.get::<Transform>(entity).cloned().unwrap_or_default();
                let position = transform.position.to_array();
                let rotation = [
                    transform.rotation.x,
                    transform.rotation.y,
                    transform.rotation.z,
                    transform.rotation.w,
                ];
                let scale = transform.scale.to_array();
                let physics_enabled = self.physics.has_physics(entity);
                let physics_type = self.physics.get_body_type(entity).to_string();
                send_event(&EngineEvent::EntitySelected {
                    id: entity,
                    name,
                    position,
                    rotation,
                    scale,
                    physics_enabled,
                    physics_type,
                });
                if self.ctrl_held && self.selected_entities.len() > 1 {
                    send_event(&EngineEvent::MultiSelectChanged {
                        ids: self.selected_entities.clone(),
                    });
                }
            }
            None => {
                if !self.ctrl_held
                    && (self.selected_entity.is_some() || !self.selected_entities.is_empty())
                {
                    self.selected_entity = None;
                    self.selected_entities.clear();
                    send_event(&EngineEvent::EntityDeselected);
                }
            }
        }
    }

    pub(crate) fn project_to_screen(&self, p: GlamVec3) -> Option<(f32, f32)> {
        let w = self.size.width as f32;
        let h = self.size.height as f32;
        let vp = self.camera.proj_matrix(w / h) * self.camera.view_matrix();
        let c = vp * glam::Vec4::new(p.x, p.y, p.z, 1.0);
        if c.w <= 0.0 {
            return None;
        }
        Some(((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
    }

    pub fn pick_gizmo_axis(&self, pixel_x: f32, pixel_y: f32) -> Option<usize> {
        let origin = self.selection_center()?;
        let so = self.project_to_screen(origin)?;

        const LEN: f32 = 1.2;
        const THRESH: f32 = 16.0;
        let dirs = [GlamVec3::X, GlamVec3::Y, GlamVec3::Z];

        let mut best: Option<(f32, usize)> = None;
        for (i, &dir) in dirs.iter().enumerate() {
            if let Some(tip) = self.project_to_screen(origin + dir * LEN) {
                let d = point_to_segment_2d(pixel_x, pixel_y, so.0, so.1, tip.0, tip.1);
                if d < THRESH && best.map_or(true, |(bd, _)| d < bd) {
                    best = Some((d, i));
                }
            }
        }
        best.map(|(_, i)| i)
    }

    pub fn drag_gizmo(
        &mut self,
        pixel_x: f32,
        pixel_y: f32,
        last_x: f32,
        last_y: f32,
        axis_idx: usize,
    ) {
        let selected_ids: Vec<EntityId> = if !self.selected_entities.is_empty() {
            self.selected_entities.clone()
        } else {
            self.selected_entity.into_iter().collect()
        };
        if selected_ids.is_empty() {
            return;
        }

        let w = self.size.width as f32;
        let h = self.size.height as f32;
        let aspect = w / h;

        let mut sum = GlamVec3::ZERO;
        let mut count = 0usize;
        for &id in &selected_ids {
            if let Some(t) = self.world.get::<Transform>(id) {
                sum += t.position;
                count += 1;
            }
        }
        if count == 0 {
            return;
        }
        let origin = sum / count as f32;

        let vp = self.camera.proj_matrix(aspect) * self.camera.view_matrix();
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
        let world_delta = (dx * ax + dy * ay) / (axis_len * axis_len);

        for &sel_id in &selected_ids {
            if let Some(t) = self.world.get_mut::<Transform>(sel_id) {
                t.position += axis_world * world_delta;
            }
            if let Some(t) = self.world.get::<Transform>(sel_id).cloned() {
                if self.physics.has_physics(sel_id) {
                    let half = [
                        (t.scale.x * 0.5).max(0.01),
                        (t.scale.y * 0.5).max(0.01),
                        (t.scale.z * 0.5).max(0.01),
                    ];
                    self.physics.sync_entity_physics_from_transform(
                        sel_id,
                        t.position.to_array(),
                        half,
                    );
                }
            }
        }

        let lead_id = self.selected_entity.or_else(|| selected_ids.last().copied());
        if let Some(sel_id) = lead_id {
            let name = self.world.name(sel_id).unwrap_or("Entity").to_string();
            if let Some(t) = self.world.get::<Transform>(sel_id) {
                let pos = t.position.to_array();
                let rot = [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w];
                let scl = t.scale.to_array();
                let physics_enabled = self.physics.has_physics(sel_id);
                let physics_type = self.physics.get_body_type(sel_id).to_string();
                send_event(&EngineEvent::EntitySelected {
                    id: sel_id,
                    name,
                    position: pos,
                    rotation: rot,
                    scale: scl,
                    physics_enabled,
                    physics_type,
                });
            }
        }
    }

    pub fn update_hover(&mut self, pixel_x: f32, pixel_y: f32) {
        let prev_hover = self.hovered_entity;
        self.hovered_entity = self.ray_cast(pixel_x, pixel_y);
        self.hovered_gizmo_axis = self.pick_gizmo_axis(pixel_x, pixel_y);
        match (prev_hover, self.hovered_entity) {
            (None, Some(id)) => crate::ipc::send_event(&crate::ipc::EngineEvent::EntityHovered { id }),
            (Some(_), None) => crate::ipc::send_event(&crate::ipc::EngineEvent::EntityUnhovered),
            (Some(a), Some(b)) if a != b => {
                crate::ipc::send_event(&crate::ipc::EngineEvent::EntityHovered { id: b })
            }
            _ => {}
        }
    }
}
