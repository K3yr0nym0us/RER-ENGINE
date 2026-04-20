// ── Lógica exclusiva del modo 2D (plataformer, vista lateral) ────────────────
//
// Contiene:
//  · camera_2d            — Camera2D (ortográfica)
//  · setup_2d_platformer  — inicialización de la escena 2D
//  · load_scenario        — carga un PNG como fondo de escenario
//  · project_to_screen_2d — proyecta un punto 3D a píxeles (cámara ortográfica)
//  · pick_entity_2d       — picking por AABB en el plano XY
//  · pick_gizmo_axis_2d   — eje del gizmo más cercano al cursor
//  · drag_gizmo_2d        — arrastre de entidad sobre eje X o Y
//  · update_hover_2d      — hover AABB + detección de eje de gizmo

pub(crate) mod camera_2d;
pub(crate) use camera_2d::Camera2D;

use std::fs;

use glam::Vec3 as GlamVec3;
use crate::ecs::{EntityId, MeshComponent, Transform};
use crate::engine::{ScenarioBg, State};
use crate::config_shared::point_to_segment_2d;
use crate::ipc::{send_event, EngineEvent};
use crate::mesh::{upload, Mesh, Vertex};
use crate::texture::GpuTexture;

impl State {
    // ── Inicialización ────────────────────────────────────────────────────────

    /// Configura la escena 2D de plataformas con un único rectángulo (player).
    pub(crate) fn setup_2d_platformer(&mut self) {
        // Limpiar escena previa y escenario de fondo
        self.scenario_bg = None;
        self.world.clear();
        self.meshes.clear();
        self.textures.clear();
        self.entity_buffers.clear();
        self.entity_bind_groups.clear();
        self.selected_entity = None;
        self.hovered_entity  = None;

        // Quad unitario en el origen — el Transform lo escala/posiciona.
        // Imprescindible para que picking AABB, gizmo y hover funcionen.

        // -- Personaje: quad 1.0 × 1.5, centrado en (0, 0) -------------------
        let player_mesh = create_quad_xy(&self.device, 0.0, 0.0, 1.0, 1.0, "player-unit");
        self.meshes.push(player_mesh);
        let player_tex = GpuTexture::solid_color(&self.device, &self.queue, 232, 220, 200);
        self.textures.push(player_tex.create_bind_group(&self.device, &self.texture_bgl));
        let (b, bg) = self.alloc_entity_uniform();
        self.entity_buffers.push(b);
        self.entity_bind_groups.push(bg);
        let player_id = self.world.spawn(Some("Player"));
        self.world.insert(player_id, MeshComponent { mesh_idx: 0 });
        self.world.insert(player_id, crate::ecs::Transform {
            position: GlamVec3::new(0.0, 0.0, 0.0),
            scale:    GlamVec3::new(1.0, 1.5, 1.0),
            ..Default::default()
        });

        // -- Cámara ortográfica -----------------------------------------------
        self.camera_2d = Some(Camera2D {
            x:      0.0,
            y:      0.0,
            half_h: 3.5,
            near:  -100.0,
            far:    100.0,
        });

        // Fondo oscuro azulado (estilo Hollow Knight)
        self.clear_color = wgpu::Color { r: 0.04, g: 0.04, b: 0.10, a: 1.0 };

        log::info!("Escena 2D cargada: plataformer vista lateral");
    }

    // ── Escenario PNG de fondo ────────────────────────────────────────────────

    /// Carga una imagen PNG del disco y la establece como fondo de escenario.
    pub(crate) fn load_scenario(&mut self, path: &str) {
        let bytes = match fs::read(path) {
            Ok(b)  => b,
            Err(e) => {
                log::error!("[load_scenario] error leyendo {path}: {e}");
                send_event(&EngineEvent::Error { message: format!("No se pudo leer el escenario: {e}") });
                return;
            }
        };
        let gpu_tex = match GpuTexture::from_image_bytes(&self.device, &self.queue, &bytes, "scenario") {
            Ok(t)  => t,
            Err(e) => {
                log::error!("[load_scenario] error decodificando PNG {path}: {e}");
                send_event(&EngineEvent::Error { message: format!("Error al decodificar PNG: {e}") });
                return;
            }
        };
        let tex_bg = gpu_tex.create_bind_group(&self.device, &self.texture_bgl);
        let mesh   = create_quad_xy(&self.device, 0.0, 0.0, 1.0, 1.0, "scenario-quad");
        let (entity_buf, entity_bg) = self.alloc_entity_uniform();
        self.scenario_bg = Some(ScenarioBg { mesh, tex_bg, entity_buf, entity_bg });
        log::info!("[load_scenario] escenario cargado: {path}");
    }

    // ── Proyección 2D a pantalla ──────────────────────────────────────────────

    /// Proyecta un punto de mundo XY a coordenadas de pantalla en píxeles.
    pub(crate) fn project_to_screen_2d(&self, cam: &Camera2D, p: GlamVec3) -> Option<(f32, f32)> {
        let w  = self.size.width  as f32;
        let h  = self.size.height as f32;
        let vp = cam.view_proj(w / h);
        let c  = vp * glam::Vec4::new(p.x, p.y, p.z, 1.0);
        if c.w.abs() < 1e-6 { return None; }
        Some(((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
    }

    // ── Picking 2D ────────────────────────────────────────────────────────────

    /// Selecciona la entidad bajo el cursor usando AABB en el plano XY.
    pub fn pick_entity_2d(&mut self, pixel_x: f32, pixel_y: f32) {
        let cam = match &self.camera_2d {
            Some(c) => Camera2D { x: c.x, y: c.y, half_h: c.half_h, near: c.near, far: c.far },
            None    => return,
        };
        let w      = self.size.width  as f32;
        let h      = self.size.height as f32;
        let aspect = w / h;
        let half_w = cam.half_h * aspect;
        // NDC → coordenadas de mundo
        let wx = cam.x + ((pixel_x / w) * 2.0 - 1.0) * half_w;
        let wy = cam.y + (1.0 - (pixel_y / h) * 2.0) * cam.half_h;

        let mut hit: Option<EntityId> = None;
        for &entity in self.world.entities() {
            if self.world.get::<crate::ecs::NonSelectable>(entity).is_some() { continue; }
            if let Some(transform) = self.world.get::<Transform>(entity) {
                let p  = transform.position;
                let sx = transform.scale.x * 0.5;
                let sy = transform.scale.y * 0.5;
                if wx >= p.x - sx && wx <= p.x + sx && wy >= p.y - sy && wy <= p.y + sy {
                    hit = Some(entity);
                    break;
                }
            }
        }
        match hit {
            Some(entity) => {
                if self.selected_entity == Some(entity) { return; }
                self.selected_entity = Some(entity);
                let name      = self.world.name(entity).unwrap_or("Entity").to_string();
                let transform = self.world.get::<Transform>(entity).cloned().unwrap_or_default();
                let pos = transform.position.to_array();
                let rot = [transform.rotation.x, transform.rotation.y,
                           transform.rotation.z, transform.rotation.w];
                let scl = transform.scale.to_array();
                send_event(&EngineEvent::EntitySelected { id: entity, name, position: pos, rotation: rot, scale: scl });
            }
            None => {
                if self.selected_entity.is_some() {
                    self.selected_entity = None;
                    send_event(&EngineEvent::EntityDeselected);
                }
            }
        }
    }

    // ── Picking de eje del gizmo 2D ───────────────────────────────────────────

    /// Devuelve el índice del eje del gizmo 2D más cercano al cursor (0=X, 1=Y).
    pub fn pick_gizmo_axis_2d(&self, pixel_x: f32, pixel_y: f32) -> Option<usize> {
        let sel_id = self.selected_entity?;
        let origin = self.world.get::<Transform>(sel_id)?.position;
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
    pub fn drag_gizmo_2d(&mut self, pixel_x: f32, pixel_y: f32, last_x: f32, last_y: f32, axis_idx: usize) {
        let sel_id = match self.selected_entity { Some(id) => id, None => return };
        let cam = match &self.camera_2d {
            Some(c) => Camera2D { x: c.x, y: c.y, half_h: c.half_h, near: c.near, far: c.far },
            None    => return,
        };
        let origin = match self.world.get::<Transform>(sel_id) {
            Some(t) => t.position,
            None    => return,
        };
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
        let name = self.world.name(sel_id).unwrap_or("Entity").to_string();
        if let Some(t) = self.world.get_mut::<Transform>(sel_id) {
            t.position += axis_world * world_delta;
            let pos = t.position.to_array();
            let rot = [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w];
            let scl = t.scale.to_array();
            send_event(&EngineEvent::EntitySelected { id: sel_id, name, position: pos, rotation: rot, scale: scl });
        }
    }

    // ── Hover 2D ─────────────────────────────────────────────────────────────

    /// Actualiza `hovered_entity` y `hovered_gizmo_axis` en modo 2D.
    pub fn update_hover_2d(&mut self, pixel_x: f32, pixel_y: f32) {
        let cam = match &self.camera_2d {
            Some(c) => Camera2D { x: c.x, y: c.y, half_h: c.half_h, near: c.near, far: c.far },
            None    => return,
        };
        let w      = self.size.width  as f32;
        let h      = self.size.height as f32;
        let aspect = w / h;
        let half_w = cam.half_h * aspect;
        let wx = cam.x + ((pixel_x / w) * 2.0 - 1.0) * half_w;
        let wy = cam.y + (1.0 - (pixel_y / h) * 2.0) * cam.half_h;

        self.hovered_entity = None;
        for &entity in self.world.entities() {
            if self.world.get::<crate::ecs::NonSelectable>(entity).is_some() { continue; }
            if let Some(t) = self.world.get::<Transform>(entity) {
                let sx = t.scale.x * 0.5;
                let sy = t.scale.y * 0.5;
                if wx >= t.position.x - sx && wx <= t.position.x + sx
                && wy >= t.position.y - sy && wy <= t.position.y + sy {
                    self.hovered_entity = Some(entity);
                    break;
                }
            }
        }
        self.hovered_gizmo_axis = self.pick_gizmo_axis_2d(pixel_x, pixel_y);
    }
}

// ── Primitivas de malla para el modo 2D ───────────────────────────────────────

/// Quad en el plano XY (normal +Z).
/// `cx`, `cy` = centro en mundo  |  `w`, `h` = ancho y alto  |  UVs: 0..1
fn create_quad_xy(device: &wgpu::Device, cx: f32, cy: f32, w: f32, h: f32, label: &str) -> Mesh {
    let hw = w / 2.0;
    let hh = h / 2.0;
    let vertices = vec![
        Vertex { position: [cx - hw, cy - hh, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 1.0] },
        Vertex { position: [cx + hw, cy - hh, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 1.0] },
        Vertex { position: [cx + hw, cy + hh, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 0.0] },
        Vertex { position: [cx - hw, cy + hh, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 0.0] },
    ];
    let indices = vec![0u32, 1, 2, 2, 3, 0];
    upload(device, &vertices, &indices, label)
}
