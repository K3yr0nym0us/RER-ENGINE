// ── Lógica exclusiva del modo 3D ─────────────────────────────────────────────
//
// Contiene:
//  · camera_3d        — Camera (órbita) + CameraUniform
//  · load_model       — carga un archivo .glb/.gltf y puebla la escena
//  · ray_cast         — proyecta un rayo desde píxel y devuelve la entidad más cercana
//  · pick_entity      — dispara el picking 3D y emite IPC
//  · project_to_screen — proyecta un punto 3D a píxeles de pantalla
//  · pick_gizmo_axis  — detecta el eje del gizmo más cercano al cursor
//  · drag_gizmo       — arrastra una entidad sobre un eje 3D
//  · update_hover     — actualiza el hover de entidad y gizmo en modo 3D

pub(crate) mod camera_3d;
pub(crate) use camera_3d::Camera;

pub(crate) mod mesh_3d;
pub(crate) mod physics_3d;

use std::path::Path;

use glam::Vec3 as GlamVec3;

use crate::ecs::{EntityId, MeshComponent, Transform};
use crate::engine::State;
use crate::config_shared::point_to_segment_2d;
use crate::ipc::{send_event, EngineEvent};
use crate::texture::GpuTexture;

impl State {
    // ── Carga de modelo 3D ────────────────────────────────────────────────────

    /// Carga un archivo .glb / .gltf desde disco y puebla la escena con sus mallas.
    pub(crate) fn load_model(&mut self, path: &str) {
        match mesh_3d::load_glb(&self.device, Path::new(path)) {
            Ok((gltf_meshes, images)) => {
                self.world.clear();
                self.meshes.clear();
                self.textures.clear();
                self.entity_buffers.clear();
                self.entity_bind_groups.clear();

                let count = gltf_meshes.len();
                for (i, gm) in gltf_meshes.into_iter().enumerate() {
                    let tex_bg = if let Some(tex_idx) = gm.tex_index {
                        if let Some(img_data) = images.get(tex_idx) {
                            let gpu_tex = GpuTexture::from_gltf_image(
                                &self.device, &self.queue, img_data,
                                &format!("tex-{tex_idx}"),
                            );
                            gpu_tex.create_bind_group(&self.device, &self.texture_bgl)
                        } else {
                            GpuTexture::white(&self.device, &self.queue)
                                .create_bind_group(&self.device, &self.texture_bgl)
                        }
                    } else {
                        GpuTexture::white(&self.device, &self.queue)
                            .create_bind_group(&self.device, &self.texture_bgl)
                    };

                    self.meshes.push(gm.mesh);
                    self.textures.push(tex_bg);
                    let (b, bg) = self.alloc_entity_uniform();
                    self.entity_buffers.push(b);
                    self.entity_bind_groups.push(bg);

                    let label = format!("Mesh {i}");
                    let id = self.world.spawn(Some(&label));
                    self.world.insert(id, MeshComponent { mesh_idx: i });
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

    // ── Ray cast ──────────────────────────────────────────────────────────────

    /// Proyecta un rayo desde el píxel de pantalla y devuelve la entidad más cercana.
    fn ray_cast(&self, pixel_x: f32, pixel_y: f32) -> Option<EntityId> {
        use glam::Vec4;

        let w      = self.size.width  as f32;
        let h      = self.size.height as f32;
        let aspect = w / h;

        let ndc_x =  (2.0 * pixel_x / w) - 1.0;
        let ndc_y = -(2.0 * pixel_y / h) + 1.0;

        let inv_proj = self.camera.proj_matrix(aspect).inverse();
        let inv_view = self.camera.view_matrix().inverse();

        let clip_dir  = Vec4::new(ndc_x, ndc_y, -1.0, 0.0);
        let view_dir  = inv_proj * clip_dir;
        let view_dir  = Vec4::new(view_dir.x, view_dir.y, -1.0, 0.0);
        let world_dir = (inv_view * view_dir).truncate().normalize();
        let ray_origin = self.camera.position();

        let mut closest: Option<(f32, EntityId)> = None;
        for &entity in self.world.entities() {
            if let Some(transform) = self.world.get::<Transform>(entity) {
                let center = transform.position;
                let radius = transform.scale.x.max(transform.scale.y).max(transform.scale.z) * 0.866;
                let oc   = ray_origin - center;
                let b    = oc.dot(world_dir);
                let c    = oc.dot(oc) - radius * radius;
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

    // ── Picking 3D ────────────────────────────────────────────────────────────

    /// Selecciona la entidad bajo el cursor mediante ray casting.
    pub fn pick_entity(&mut self, pixel_x: f32, pixel_y: f32) {
        match self.ray_cast(pixel_x, pixel_y) {
            Some(entity) => {
                if self.selected_entity == Some(entity) { return; }
                self.selected_entity = Some(entity);
                let name      = self.world.name(entity).unwrap_or("Entity").to_string();
                let transform = self.world.get::<Transform>(entity).cloned().unwrap_or_default();
                let position  = transform.position.to_array();
                let rotation  = [
                    transform.rotation.x, transform.rotation.y,
                    transform.rotation.z, transform.rotation.w,
                ];
                let scale = transform.scale.to_array();
                send_event(&EngineEvent::EntitySelected { id: entity, name, position, rotation, scale });
            }
            None => {
                if self.selected_entity.is_some() {
                    self.selected_entity = None;
                    send_event(&EngineEvent::EntityDeselected);
                }
            }
        }
    }

    // ── Proyección 3D a pantalla ──────────────────────────────────────────────

    /// Proyecta un punto 3D a coordenadas de pantalla en píxeles.
    pub(crate) fn project_to_screen(&self, p: GlamVec3) -> Option<(f32, f32)> {
        let w  = self.size.width  as f32;
        let h  = self.size.height as f32;
        let vp = self.camera.proj_matrix(w / h) * self.camera.view_matrix();
        let c  = vp * glam::Vec4::new(p.x, p.y, p.z, 1.0);
        if c.w <= 0.0 { return None; }
        Some(((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
    }

    // ── Picking de eje del gizmo 3D ───────────────────────────────────────────

    /// Devuelve el índice del eje del gizmo 3D más cercano al cursor (0=X, 1=Y, 2=Z).
    pub fn pick_gizmo_axis(&self, pixel_x: f32, pixel_y: f32) -> Option<usize> {
        let sel_id = self.selected_entity?;
        let origin = self.world.get::<Transform>(sel_id)?.position;
        let so     = self.project_to_screen(origin)?;

        const LEN:    f32 = 1.2;
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

    // ── Drag de gizmo 3D ──────────────────────────────────────────────────────

    /// Arrastra la entidad seleccionada a lo largo del eje `axis_idx` (0=X, 1=Y, 2=Z).
    pub fn drag_gizmo(&mut self, pixel_x: f32, pixel_y: f32, last_x: f32, last_y: f32, axis_idx: usize) {
        let sel_id = match self.selected_entity { Some(id) => id, None => return };
        let w      = self.size.width  as f32;
        let h      = self.size.height as f32;
        let aspect = w / h;

        let origin = match self.world.get::<Transform>(sel_id) {
            Some(t) => t.position,
            None    => return,
        };

        let vp         = self.camera.proj_matrix(aspect) * self.camera.view_matrix();
        let axis_world = [GlamVec3::X, GlamVec3::Y, GlamVec3::Z][axis_idx];

        let project = |p: GlamVec3| -> Option<(f32, f32)> {
            let c = vp * glam::Vec4::new(p.x, p.y, p.z, 1.0);
            if c.w <= 0.0 { return None; }
            Some(((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
        };

        let (s0x, s0y) = match project(origin)               { Some(p) => p, None => return };
        let (s1x, s1y) = match project(origin + axis_world)  { Some(p) => p, None => return };

        let ax       = s1x - s0x;
        let ay       = s1y - s0y;
        let axis_len = (ax * ax + ay * ay).sqrt();
        if axis_len < 1e-4 { return; }

        let dx          = pixel_x - last_x;
        let dy          = pixel_y - last_y;
        let world_delta = (dx * ax + dy * ay) / (axis_len * axis_len);

        let name = self.world.name(sel_id).unwrap_or("Entity").to_string();
        if let Some(t) = self.world.get_mut::<Transform>(sel_id) {
            t.position += axis_world * world_delta;
            let pos = t.position.to_array();
            let rot = [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w];
            let scl = t.scale.to_array();
            send_event(&EngineEvent::EntitySelected { id: sel_id, name, position: pos, rotation: rot, scale: scl });
        }
    }

    // ── Hover 3D ─────────────────────────────────────────────────────────────

    /// Actualiza `hovered_entity` y `hovered_gizmo_axis` en modo 3D.
    pub fn update_hover(&mut self, pixel_x: f32, pixel_y: f32) {
        self.hovered_entity     = self.ray_cast(pixel_x, pixel_y);
        self.hovered_gizmo_axis = self.pick_gizmo_axis(pixel_x, pixel_y);
    }
}
