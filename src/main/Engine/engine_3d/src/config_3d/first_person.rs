use std::collections::HashSet;

use glam::Vec3;

use crate::engine::State;

pub(crate) const FIRST_PERSON_KEYBOARD_SPEED: f32 = 4.0;
pub(crate) const FIRST_PERSON_MOUSE_SPEED: f32 = 0.0020;

impl State {
    pub(crate) fn is_first_person_runtime_active(&self) -> bool {
        self.preview_playing && self.camera_2d.is_none()
    }

    pub(crate) fn apply_first_person_mouse_look(&mut self, dx: f32, dy: f32) {
        if !self.is_first_person_runtime_active() {
            return;
        }

        self.camera.yaw += dx * FIRST_PERSON_MOUSE_SPEED;
        self.camera.pitch = (self.camera.pitch + dy * FIRST_PERSON_MOUSE_SPEED).clamp(
            -std::f32::consts::FRAC_PI_2 + 0.05,
            std::f32::consts::FRAC_PI_2 - 0.05,
        );
        // Mantener la cámara en modo first-person aunque el editor haya orbitado antes.
        self.camera.distance = 0.01;
    }

    pub(crate) fn apply_first_person_keyboard(
        &mut self,
        pressed_inputs: &HashSet<String>,
        delta_time: f32,
    ) {
        if !self.is_first_person_runtime_active() || delta_time <= 0.0 {
            return;
        }

        let (sy, cy) = self.camera.yaw.sin_cos();
        let forward = Vec3::new(-cy, 0.0, -sy).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();

        let mut movement = Vec3::ZERO;
        if pressed_inputs.contains("W") {
            movement += forward;
        }
        if pressed_inputs.contains("S") {
            movement -= forward;
        }
        if pressed_inputs.contains("D") {
            movement += right;
        }
        if pressed_inputs.contains("A") {
            movement -= right;
        }
        if pressed_inputs.contains("SPACE") {
            movement.y += 1.0;
        }
        if pressed_inputs.contains("SHIFT") {
            movement.y -= 1.0;
        }

        if movement.length_squared() <= f32::EPSILON {
            return;
        }

        self.camera.target += movement.normalize() * FIRST_PERSON_KEYBOARD_SPEED * delta_time;
        self.camera.distance = 0.01;
    }
}
