// ── Cámara 3D — modo órbita (perspectiva) ────────────────────────────────────
//
// Usada por el modo 3D y por el escenario BASE (cubo de referencia).

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

use super::character_anchor::EDITOR_DEFAULT_ORBIT_PITCH;
use super::world_bounds::SKY_SPHERE_RADIUS;

/// `far` de una cámara recién creada: cubre la esfera de cielo con el mismo
/// margen que `sync_world_bounds_3d_runtime`. Las plantillas 3D recrean la
/// cámara con `Camera::new()` DESPUÉS de ese sync (config_base.rs), así que si
/// aquí se deja 1000 m el sol (a 1360 m) queda más allá del far plane y su halo
/// se recorta aunque el sync haya calculado 1760 m.
pub(crate) const DEFAULT_CAMERA_FAR: f32 = SKY_SPHERE_RADIUS * 1.1;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct CameraUniform {
    pub(crate) view_proj: [[f32; 4]; 4],
}

pub(crate) struct Camera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    /// Desplazamiento vertical de la vista play character (ojos sobre el collider).
    pub(crate) eye_height_offset: f32,
    /// Offset de ojos en espacio local del jugador (Play FP).
    pub(crate) eye_offset_local: Vec3,
    /// Pivote extra para órbita en editor FP (target = pies, pivote = pies + offset).
    pub(crate) orbit_pivot_offset: Vec3,
}

impl Camera {
    pub(crate) fn new() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 3.0,
            yaw: -std::f32::consts::FRAC_PI_4,
            pitch: EDITOR_DEFAULT_ORBIT_PITCH,
            fov_y: 45_f32.to_radians(),
            near: 0.1,
            far: DEFAULT_CAMERA_FAR,
            eye_height_offset: 0.0,
            eye_offset_local: Vec3::ZERO,
            orbit_pivot_offset: Vec3::ZERO,
        }
    }

    pub(crate) fn orbit_pivot_at(&self, anchor: Vec3) -> Vec3 {
        anchor + self.orbit_pivot_offset
    }

    pub(crate) fn view_forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(-cy * cp, -sp, -sy * cp).normalize_or_zero()
    }

    pub(crate) fn position_at_angles(
        &self,
        anchor: Vec3,
        yaw: f32,
        pitch: f32,
        distance: f32,
    ) -> Vec3 {
        let (sy, cy) = yaw.sin_cos();
        let (sp, cp) = pitch.sin_cos();
        let pivot = self.orbit_pivot_at(anchor);
        pivot
            + Vec3::new(cy * cp, sp, sy * cp) * distance
            + Vec3::Y * self.eye_height_offset
            + self.eye_offset_local
    }

    pub(crate) fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance - delta * 0.3).clamp(0.5, 500.0);
    }

    pub(crate) fn pan_offset_with_distance(
        &self,
        anchor: Vec3,
        dx: f32,
        dy: f32,
        distance: f32,
        yaw: f32,
        pitch: f32,
    ) -> Vec3 {
        const SENSITIVITY: f32 = 0.002;
        let pos = self.position_at_angles(anchor, yaw, pitch, distance);
        let pivot = self.orbit_pivot_at(anchor);
        let fwd = (pivot - pos).normalize();
        let right = fwd.cross(Vec3::Y).normalize();
        let up = right.cross(fwd).normalize();
        right * (-dx * SENSITIVITY * distance) + up * (dy * SENSITIVITY * distance)
    }

    /// Vista FPS desde posición absoluta del ojo (acordeón Cámara en Play).
    pub(crate) fn view_matrix_from_eye(&self, eye: Vec3, yaw: f32, pitch: f32) -> Mat4 {
        let (sy, cy) = yaw.sin_cos();
        let (sp, cp) = pitch.sin_cos();
        let forward = Vec3::new(-cy * cp, -sp, -sy * cp).normalize_or_zero();
        Mat4::look_at_rh(eye, eye + forward, Vec3::Y)
    }

    pub(crate) fn view_matrix_at_angles(
        &self,
        anchor: Vec3,
        yaw: f32,
        pitch: f32,
        distance: f32,
    ) -> Mat4 {
        let pos = self.position_at_angles(anchor, yaw, pitch, distance);
        if distance < 0.5 {
            let (sy, cy) = yaw.sin_cos();
            let (sp, cp) = pitch.sin_cos();
            let forward = Vec3::new(-cy * cp, -sp, -sy * cp).normalize_or_zero();
            return Mat4::look_at_rh(pos, pos + forward, Vec3::Y);
        }
        Mat4::look_at_rh(pos, self.orbit_pivot_at(anchor), Vec3::Y)
    }

    pub(crate) fn proj_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, aspect, self.near, self.far)
    }
}

#[cfg(test)]
mod tests {
    use super::{Camera, DEFAULT_CAMERA_FAR};
    use crate::engine3d::config_3d::directional_light::sun_orbit_distance_from;
    use crate::engine3d::config_3d::world_bounds::WorldBounds3D;

    #[test]
    fn a_fresh_camera_reaches_the_fixed_sun_orbit() {
        let camera = Camera::new();
        let orbit = sun_orbit_distance_from(&WorldBounds3D::default());

        // El sol debe quedar dentro del far plane incluso en la cámara que
        // crean las plantillas, que no vuelve a pasar por el sync del mundo.
        assert!(
            camera.far > orbit,
            "far {} no alcanza la órbita del sol {}",
            camera.far,
            orbit
        );
        assert!(
            (camera.far - DEFAULT_CAMERA_FAR).abs() < 1e-3,
            "far por defecto {} != {}",
            camera.far,
            DEFAULT_CAMERA_FAR
        );
    }
}
