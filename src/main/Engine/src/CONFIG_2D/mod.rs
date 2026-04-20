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
use crate::engine::State;
use crate::config_shared::point_to_segment_2d;
use crate::ipc::{send_event, EngineEvent};
use crate::mesh::{upload, Mesh, Vertex};
use crate::texture::GpuTexture;

// ── Componente exclusivo del modo 2D ─────────────────────────────────────────

/// Marca una entidad como escenario PNG en una escena 2D.
/// Guarda las dimensiones originales de la imagen para escalar
/// proporcionalmente cuando el usuario mueve el slider.
#[derive(Debug, Clone)]
pub(crate) struct ScenarioMarker {
    pub img_width:    u32,
    pub img_height:   u32,
    /// Altura base en unidades de mundo (user_scale = 1.0).
    pub base_world_h: f32,
    /// Ruta del PNG original, necesaria para duplicar la entidad.
    pub path:         String,
}

impl State {
    // ── Inicialización ────────────────────────────────────────────────────────

    /// Configura la escena 2D de plataformas con un único rectángulo (player).
    pub(crate) fn setup_2d_platformer(&mut self) {
        // Limpiar escena previa y escenarios de fondo
        self.scenario_entities.clear();
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

    /// Carga una imagen PNG del disco y la registra como entidad ECS de escenario.
    /// La entidad se posiciona en Z=-1 (detrás de todo), mantiene las proporciones
    /// de la imagen y puede seleccionarse, arrastrarse y escalarse como cualquier entidad.
    pub(crate) fn load_scenario(&mut self, path: &str) {
        let bytes = match fs::read(path) {
            Ok(b)  => b,
            Err(e) => {
                log::error!("[load_scenario] error leyendo {path}: {e}");
                send_event(&EngineEvent::Error { message: format!("No se pudo leer el escenario: {e}") });
                return;
            }
        };

        use image::ImageReader;
        use std::io::Cursor;
        let img = match ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())
            .and_then(|r| r.decode().map_err(|e| e.to_string()))
        {
            Ok(i)  => i.to_rgba8(),
            Err(e) => {
                log::error!("[load_scenario] error decodificando PNG {path}: {e}");
                send_event(&EngineEvent::Error { message: format!("Error al decodificar PNG: {e}") });
                return;
            }
        };

        let (img_width, img_height) = img.dimensions();
        let aspect       = img_width as f32 / img_height.max(1) as f32;
        // Altura base fija en unidades de mundo, independiente del zoom actual.
        // Usar cam.half_h provocaría que el mismo PNG cargue a tamaños distintos
        // si el usuario ha hecho zoom entre cargas.
        // 7.0 = 2.0 × half_h inicial (3.5), y es la referencia para scale=1.0.
        let base_world_h = 7.0_f32;
        let base_world_w = base_world_h * aspect;

        let gpu_tex  = GpuTexture::from_rgba(&self.device, &self.queue, &img, img_width, img_height, "scenario");
        let tex_bg   = gpu_tex.create_bind_group(&self.device, &self.texture_bgl);
        let mesh     = create_quad_xy(&self.device, 0.0, 0.0, 1.0, 1.0, "scenario-quad");
        let mesh_idx = self.meshes.len();
        self.meshes.push(mesh);
        self.textures.push(tex_bg);
        let (buf, bg) = self.alloc_entity_uniform();
        self.entity_buffers.push(buf);
        self.entity_bind_groups.push(bg);

        let sc_id = self.world.spawn(Some("Escenario"));
        self.world.insert(sc_id, MeshComponent { mesh_idx });
        self.world.insert(sc_id, Transform {
            position: GlamVec3::new(0.0, 0.0, -1.0),
            scale:    GlamVec3::new(base_world_w, base_world_h, 1.0),
            ..Default::default()
        });
        self.world.insert(sc_id, ScenarioMarker { img_width, img_height, base_world_h, path: path.to_owned() });
        self.scenario_entities.push(sc_id);

        send_event(&EngineEvent::ScenarioLoaded { id: sc_id, path: path.to_owned() });
        log::info!("[load_scenario] entidad {sc_id} creada {img_width}×{img_height}: {path}");
    }

    /// Duplica un escenario existente: crea una nueva entidad con el mismo PNG
    /// ligeramente desplazada (offset +1 en X e Y) para que sea visible.
    pub(crate) fn duplicate_scenario(&mut self, id: u32) {
        let path = match self.world.get::<ScenarioMarker>(id) {
            Some(m) => m.path.clone(),
            None => {
                log::warn!("[duplicate_scenario] entidad {id} no tiene ScenarioMarker");
                return;
            }
        };
        // Offset para que el duplicado sea visible sobre el original
        let offset = {
            let count = self.scenario_entities.len() as f32;
            GlamVec3::new(count * 0.5, count * 0.5, 0.0)
        };
        self.load_scenario(&path);
        // Aplicar offset a la entidad recién creada
        if let Some(&new_id) = self.scenario_entities.last() {
            if let Some(t) = self.world.get_mut::<Transform>(new_id) {
                t.position += offset;
            }
        }
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
    /// Cuando varios AABBs se solapan (p.ej. escenario + player) se elige
    /// la entidad con mayor Z (más cercana a la cámara).
    pub fn pick_entity_2d(&mut self, pixel_x: f32, pixel_y: f32) {
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

        // Recoge todos los hits y elige el de mayor Z (más cercano a la cámara).
        let mut best: Option<(EntityId, f32)> = None;
        for &entity in self.world.entities() {
            if self.world.get::<crate::ecs::NonSelectable>(entity).is_some() { continue; }
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
        let mut best_hover: Option<(EntityId, f32)> = None;
        for &entity in self.world.entities() {
            if self.world.get::<crate::ecs::NonSelectable>(entity).is_some() { continue; }
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
