// ── Cámara 3D — modo órbita (perspectiva) ────────────────────────────────────
//
// Usada por el modo 3D y por el escenario BASE (cubo de referencia).

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

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
    /// Desplazamiento vertical de la vista en primera persona (ojos sobre el collider).
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
            pitch: 0.3,
            fov_y: 45_f32.to_radians(),
            near: 0.1,
            far: 1000.0,
            eye_height_offset: 0.0,
            eye_offset_local: Vec3::ZERO,
            orbit_pivot_offset: Vec3::ZERO,
        }
    }

    fn orbit_pivot(&self) -> Vec3 {
        self.target + self.orbit_pivot_offset
    }

    pub(crate) fn orbit_pivot_at(&self, anchor: Vec3) -> Vec3 {
        anchor + self.orbit_pivot_offset
    }

    pub(crate) fn view_forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(-cy * cp, -sp, -sy * cp).normalize_or_zero()
    }

    pub(crate) fn position(&self) -> Vec3 {
        self.position_at(self.orbit_pivot())
    }

    pub(crate) fn position_at(&self, anchor: Vec3) -> Vec3 {
        self.position_at_angles(anchor, self.yaw, self.pitch, self.distance)
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

    pub(crate) fn orbit(&mut self, dx: f32, dy: f32) {
        const SENSITIVITY: f32 = 0.005;
        self.yaw += dx * SENSITIVITY;
        self.pitch = (self.pitch - dy * SENSITIVITY).clamp(
            -std::f32::consts::FRAC_PI_2 + 0.05,
            std::f32::consts::FRAC_PI_2 - 0.05,
        );
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

    pub(crate) fn view_matrix_at(&self, anchor: Vec3) -> Mat4 {
        self.view_matrix_at_angles(anchor, self.yaw, self.pitch, self.distance)
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

    pub(crate) fn to_uniform(&self, aspect: f32) -> CameraUniform {
        self.to_uniform_at(self.orbit_pivot(), aspect)
    }

    pub(crate) fn to_uniform_at(&self, anchor: Vec3, aspect: f32) -> CameraUniform {
        CameraUniform {
            view_proj: (self.proj_matrix(aspect) * self.view_matrix_at(anchor)).to_cols_array_2d(),
        }
    }

}
