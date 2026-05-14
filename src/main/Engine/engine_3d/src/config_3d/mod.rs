// ── Lógica exclusiva del modo 3D ─────────────────────────────────────────────
//
// Contiene:
//  · camera_3d        — Camera (órbita) + CameraUniform
//  · first_person     — movimiento y mouse look del runtime 3D
//  · load_model       — carga un archivo .glb/.gltf y puebla la escena
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

use std::path::Path;

use glam::Vec3 as GlamVec3;

use crate::config_shared::point_to_segment_2d;
use crate::ecs::{EntityId, MeshComponent, Transform};
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};

impl State {
    /// Carga un archivo .glb / .gltf desde disco y puebla la escena con sus mallas.
    pub(crate) fn load_model(&mut self, path: &str) {
        match mesh_3d::load_glb(&self.device, Path::new(path)) {
            Ok((gltf_meshes, images)) => {
                self.world.clear();
                self.meshes.clear();
                self.uv_rects.clear();

                let count = gltf_meshes.len();
                for gm in gltf_meshes {
                    let rgba: Vec<u8> = if let Some(img_idx) = gm.tex_index {
                        if let Some(img_data) = images.get(img_idx) {
                            use gltf::image::Format;
                            match img_data.format {
                                Format::R8G8B8 => img_data
                                    .pixels
                                    .chunks_exact(3)
                                    .flat_map(|p| [p[0], p[1], p[2], 255u8])
                                    .collect(),
                                Format::R8G8B8A8 => img_data.pixels.clone(),
                                _ => vec![255, 255, 255, 255],
                            }
                        } else {
                            vec![255, 255, 255, 255]
                        }
                    } else {
                        vec![255, 255, 255, 255]
                    };
                    let (img_w, img_h) = if let Some(img_idx) = gm.tex_index {
                        images.get(img_idx).map(|d| (d.width, d.height)).unwrap_or((1, 1))
                    } else {
                        (1, 1)
                    };

                    let mesh_idx = self.meshes.len();
                    let tex_idx = self.uv_rects.len();
                    self.meshes.push(gm.mesh);
                    let uv = self.atlas.pack(&self.queue, &rgba, img_w, img_h);
                    self.uv_rects.push(uv);

                    let label = self.next_numbered_entity_name("Mesh");
                    let id = self.world.spawn(Some(&label));
                    self.world.insert(id, MeshComponent { mesh_idx, tex_idx });
                    send_event(&EngineEvent::ModelLoaded { id });
                }
                log::info!("Modelo cargado: {path} ({count} malla/s)");
            }
            Err(e) => {
                log::error!("Error cargando modelo: {e}");
                send_event(&EngineEvent::Error { message: e });
            }
        }
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
            if let Some(transform) = self.world.get::<Transform>(entity) {
                let center = transform.position;
                let radius =
                    transform.scale.x.max(transform.scale.y).max(transform.scale.z) * 0.866;
                let oc = ray_origin - center;
                let b = oc.dot(world_dir);
                let c = oc.dot(oc) - radius * radius;
                let disc = b * b - c;
                if disc >= 0.0 {
                    let t = -b - disc.sqrt();
                    if t > 0.0 && closest.map_or(true, |(ct, _)| t < ct) {
                        closest = Some((t, entity));
                    }
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
