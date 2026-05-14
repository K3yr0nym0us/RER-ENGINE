use glam::{Mat4, Vec3};

#[derive(Debug, Clone)]
pub(crate) struct Camera2D {
    pub x: f32,
    pub y: f32,
    pub half_h: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera2D {
    pub(crate) fn position(&self) -> Vec3 {
        Vec3::new(self.x, self.y, 10.0)
    }

    pub(crate) fn view_proj(&self, aspect: f32) -> Mat4 {
        let half_w = self.half_h * aspect;
        let proj = Mat4::orthographic_rh(-half_w, half_w, -self.half_h, self.half_h, self.near, self.far);
        let view = Mat4::look_at_rh(
            Vec3::new(self.x, self.y, 10.0),
            Vec3::new(self.x, self.y, 0.0),
            Vec3::Y,
        );
        proj * view
    }

    pub(crate) fn pan(&mut self, _dx: f32, _dy: f32, _vw: f32, _vh: f32) {}
}
