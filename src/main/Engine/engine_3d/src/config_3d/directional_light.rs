//! Luz direccional (sol) para iluminación PBR compatible con pipelines RTX.
//! La posición del sol en el mundo define la dirección de la luz.

use glam::{Mat4, Vec3};

use crate::ecs::{MeshComponent, Transform};
use crate::engine::{State, SHADOW_MAP_SIZE};
use crate::entity_save_meta::EntitySaveMeta;
use crate::mesh;

/// Dirección por defecto (hacia la luz, mismo sentido que el shader histórico).
pub const DEFAULT_LIGHT_DIR: Vec3 = Vec3::new(0.6, 1.0, 0.4);
/// Color de la luz direccional (intensidad ~1; evita saturar el framebuffer sRGB).
pub const DEFAULT_LIGHT_COLOR: Vec3 = Vec3::new(1.0, 0.96, 0.88);
/// Factor de ambiente (0–1). Bajo = caras no iluminadas más oscuras.
pub const DEFAULT_LIGHT_AMBIENT: f32 = 0.06;
/// Multiplicador de color de luz direccional.
pub const DEFAULT_LIGHT_INTENSITY: f32 = 1.0;
/// Factor mínimo en zona de sombra (0–1). Bajo = sombras más oscuras.
pub const DEFAULT_SHADOW_DARKNESS: f32 = 0.22;
/// Distancia típica del icono del sol respecto al origen.
pub const SUN_DISTANCE: f32 = 42.0;

impl State {
    pub(crate) fn default_sun_position(&self) -> Vec3 {
        let center = self.directional_light_scene_center();
        center + DEFAULT_LIGHT_DIR.normalize() * SUN_DISTANCE
    }

    /// Centro de la escena para calcular la dirección del sol (luz direccional).
    pub(crate) fn directional_light_scene_center(&self) -> Vec3 {
        let min = self.world_bounds_3d.min_corner();
        let max = self.world_bounds_3d.max_corner();
        (min + max) * 0.5
    }

    /// Actualiza la dirección de luz: del centro de la escena hacia el icono del sol.
    pub(crate) fn sync_directional_light_from_sun(&mut self) {
        let Some(sun_id) = self.sun_entity else {
            return;
        };
        let Some(t) = self.world.get::<Transform>(sun_id) else {
            return;
        };
        let to_sun = t.position - self.directional_light_scene_center();
        if to_sun.length_squared() > 1e-8 {
            self.directional_light_dir = to_sun.normalize();
        }
    }

    pub(crate) fn scene_light_dir(&self) -> [f32; 4] {
        [
            self.directional_light_dir.x,
            self.directional_light_dir.y,
            self.directional_light_dir.z,
            self.directional_light_ambient,
        ]
    }

    pub(crate) fn scene_light_color(&self) -> [f32; 4] {
        [
            self.directional_light_color.x,
            self.directional_light_color.y,
            self.directional_light_color.z,
            1.0,
        ]
    }

    /// x/y = iluminación; z = 1/shadow_map; w = radio PCF (menor cerca = borde más nítido).
    pub(crate) fn scene_light_params(&self) -> [f32; 4] {
        let dist = self.camera.distance;
        let pcf_radius = if dist < 12.0 {
            1.0
        } else if dist < 28.0 {
            1.5
        } else {
            2.25
        };
        [
            self.light_intensity,
            self.shadow_darkness,
            1.0 / SHADOW_MAP_SIZE as f32,
            pcf_radius,
        ]
    }

    pub(crate) fn apply_directional_light_settings(
        &mut self,
        ambient: Option<f32>,
        intensity: Option<f32>,
        shadow_darkness: Option<f32>,
    ) {
        if let Some(v) = ambient {
            self.directional_light_ambient = v.clamp(0.0, 1.0);
        }
        if let Some(v) = intensity {
            self.light_intensity = v.clamp(0.05, 4.0);
        }
        if let Some(v) = shadow_darkness {
            self.shadow_darkness = v.clamp(0.02, 1.0);
        }
    }

    /// Shadow map centrado en el frustum de la cámara (más texeles cerca, menos en lejanía).
    pub(crate) fn build_light_view_proj(&self, aspect: f32) -> [[f32; 4]; 4] {
        let dir = self.directional_light_dir.normalize();
        let up = if dir.y.abs() > 0.95 {
            Vec3::X
        } else {
            Vec3::Y
        };
        let cam = &self.camera;
        let shadow_dist = (cam.distance * 4.0 + 6.0).clamp(8.0, 40.0);
        let tan_half = (cam.fov_y * 0.5).tan();
        let inv_view = cam.view_matrix().inverse();

        let mut corners = [Vec3::ZERO; 8];
        let mut i = 0usize;
        for z_view in [cam.near, shadow_dist] {
            let h = z_view * tan_half;
            let w = h * aspect.max(0.1);
            for sx in [-1.0f32, 1.0] {
                for sy in [-1.0f32, 1.0] {
                    corners[i] = inv_view.transform_point3(Vec3::new(sx * w, sy * h, -z_view));
                    i += 1;
                }
            }
        }

        let focus = cam.target + cam.orbit_pivot_offset;
        let light_eye = focus + dir * (shadow_dist * 3.0 + 24.0);
        let light_view = Mat4::look_at_rh(light_eye, focus, up);

        let mut min_ls = Vec3::splat(f32::INFINITY);
        let mut max_ls = Vec3::splat(f32::NEG_INFINITY);
        for p in corners {
            let ls = light_view.transform_point3(p);
            min_ls = min_ls.min(ls);
            max_ls = max_ls.max(ls);
        }

        let xy_pad = 2.0;
        min_ls.x -= xy_pad;
        min_ls.y -= xy_pad;
        max_ls.x += xy_pad;
        max_ls.y += xy_pad;
        min_ls.z -= 12.0;
        max_ls.z += 12.0;

        let half = ((max_ls.x - min_ls.x).max(max_ls.y - min_ls.y) * 0.5).max(4.0);
        let map = SHADOW_MAP_SIZE as f32;
        let texel = (2.0 * half) / map;
        let cx = (min_ls.x + max_ls.x) * 0.5;
        let cy = (min_ls.y + max_ls.y) * 0.5;
        let cx = (cx / texel).floor() * texel + texel * 0.5;
        let cy = (cy / texel).floor() * texel + texel * 0.5;

        let z_near = (-max_ls.z).max(0.5);
        let z_far = (-min_ls.z).max(z_near + 4.0);
        let proj = Mat4::orthographic_rh(
            cx - half,
            cx + half,
            cy - half,
            cy + half,
            z_near,
            z_far,
        );
        (proj * light_view).to_cols_array_2d()
    }

    /// Icono del sol seleccionable con gizmo; sin física. Idempotente al recargar `.save`.
    pub(crate) fn spawn_sun(&mut self, name: &str, position: [f32; 3], scale: [f32; 3]) {
        if let Some(id) = self.sun_entity {
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = Vec3::from_array(position);
                t.scale = Vec3::from_array(scale);
            }
            self.sync_directional_light_from_sun();
            self.send_model_loaded_event(id, name);
            return;
        }

        let mesh_idx = self.meshes.len();
        self.meshes.push(mesh::create_cube(&self.device));
        let sun_px = [255u8, 210, 120, 255];
        let tex_idx = self.uv_rects.len();
        let uv = self.atlas.pack(&self.queue, &sun_px, 1, 1);
        self.uv_rects.push(uv);

        let id = self.world.spawn(Some(name));
        self.world.insert(
            id,
            MeshComponent {
                mesh_idx,
                tex_idx,
            },
        );
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = Vec3::from_array(position);
            t.scale = Vec3::from_array(scale);
        }

        self.sun_entity = Some(id);
        self.sync_directional_light_from_sun();
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "directional_light".to_string(),
                path: "[Sun]".to_string(),
                visual_model_path: None,
                points: None,
            },
        );
        self.send_model_loaded_event(id, name);
    }

    pub(crate) fn ensure_default_sun(&mut self) {
        if self.sun_entity.is_some() {
            return;
        }
        let pos = self.default_sun_position();
        self.spawn_sun("Sol", pos.to_array(), [2.5, 2.5, 2.5]);
    }
}
