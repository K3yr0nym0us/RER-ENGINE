use std::collections::HashSet;

use glam::Vec3;

use crate::engine::State;

pub(crate) const FIRST_PERSON_KEYBOARD_SPEED: f32 = 4.0;
pub(crate) const FIRST_PERSON_SPRINT_MULTIPLIER: f32 = 3.0;
pub(crate) const FIRST_PERSON_MOUSE_SPEED: f32 = 0.0020;
pub(crate) const FIRST_PERSON_COLLIDER_RADIUS: f32 = 0.40;
pub(crate) const FIRST_PERSON_EYE_OFFSET: f32 = 1.35;
pub(crate) const FIRST_PERSON_GROUND_REST_Y: f32 = FIRST_PERSON_COLLIDER_RADIUS + 0.05;
/// Respaldo solo para el plano del mundo en y=0 cuando el shape-cast no reporta suelo.
pub(crate) const FIRST_PERSON_FLOOR_EPSILON: f32 = 0.12;
pub(crate) const FIRST_PERSON_JUMP_SPEED: f32 = 6.0;
pub(crate) const FIRST_PERSON_GROUND_PROBE: f32 = 0.08;

impl State {
    pub(crate) fn is_first_person_runtime_active(&self) -> bool {
        self.preview_playing && self.camera_2d.is_none()
    }

    /// Pies sobre cualquier superficie con collider (cajas, suelo, etc.).
    fn is_first_person_grounded(&mut self, position: Vec3, velocity_y: f32) -> bool {
        if velocity_y > 0.5 {
            return false;
        }

        if self.physics.is_character_grounded(
            position,
            FIRST_PERSON_COLLIDER_RADIUS,
            FIRST_PERSON_GROUND_PROBE,
        ) {
            return true;
        }

        // Plano del mundo (checker) sin depender solo de la altura para plataformas elevadas.
        position.y <= FIRST_PERSON_GROUND_REST_Y + FIRST_PERSON_FLOOR_EPSILON
    }

    fn first_person_move_speed(&self, pressed_inputs: &HashSet<String>) -> f32 {
        let mut speed = FIRST_PERSON_KEYBOARD_SPEED;
        if pressed_inputs.contains("SHIFT") {
            speed *= FIRST_PERSON_SPRINT_MULTIPLIER;
        }
        speed
    }

    pub(crate) fn reset_first_person_motion(&mut self) {
        self.first_person_velocity = Vec3::ZERO;
        self.first_person_on_floor = true;
        self.first_person_jump_queued = false;
        self.camera.eye_height_offset = 0.0;
    }

    pub(crate) fn queue_first_person_jump(&mut self) {
        if !self.is_first_person_runtime_active() {
            return;
        }
        let position = self.camera.target;
        let velocity_y = self.first_person_velocity.y;
        if self.is_first_person_grounded(position, velocity_y) {
            self.first_person_velocity.y = FIRST_PERSON_JUMP_SPEED;
            self.first_person_on_floor = false;
        }
        self.first_person_jump_queued = true;
    }

    pub(crate) fn sync_first_person_camera_mode(&mut self) {
        if self.is_first_person_runtime_active() {
            self.camera.eye_height_offset = FIRST_PERSON_EYE_OFFSET;
            self.camera.distance = 0.01;
        } else {
            self.camera.eye_height_offset = 0.0;
        }
    }

    pub(crate) fn normalize_first_person_spawn_position(&mut self) {
        let eye_y = FIRST_PERSON_GROUND_REST_Y + FIRST_PERSON_EYE_OFFSET;
        if self.camera.target.y > eye_y - 0.25 {
            self.camera.target.y = FIRST_PERSON_GROUND_REST_Y;
        }
        self.first_person_on_floor = self.is_first_person_grounded(
            self.camera.target,
            self.first_person_velocity.y,
        );
        self.first_person_velocity = Vec3::ZERO;
    }

    pub(crate) fn apply_first_person_mouse_look(&mut self, dx: f32, dy: f32) {
        if !self.is_first_person_runtime_active() {
            return;
        }

        self.sync_first_person_camera_mode();
        self.camera.yaw += dx * FIRST_PERSON_MOUSE_SPEED;
        self.camera.pitch = (self.camera.pitch + dy * FIRST_PERSON_MOUSE_SPEED).clamp(
            -std::f32::consts::FRAC_PI_2 + 0.05,
            std::f32::consts::FRAC_PI_2 - 0.05,
        );
    }

    pub(crate) fn apply_first_person_keyboard(
        &mut self,
        pressed_inputs: &HashSet<String>,
        delta_time: f32,
    ) {
        if !self.is_first_person_runtime_active() || delta_time <= 0.0 {
            return;
        }

        self.sync_first_person_camera_mode();

        let dt = delta_time.min(0.05);
        let radius = FIRST_PERSON_COLLIDER_RADIUS;
        let move_speed = self.first_person_move_speed(pressed_inputs);
        let mut position = self.camera.target;
        let mut velocity = self.first_person_velocity;

        let mut on_floor = self.is_first_person_grounded(position, velocity.y);

        let gravity = self.physics.gravity_magnitude();

        if !on_floor {
            velocity.y -= gravity * dt;
        } else if velocity.y < 0.0 {
            velocity.y = 0.0;
        }

        let jump_requested =
            pressed_inputs.contains("SPACE") || self.first_person_jump_queued;
        if jump_requested && self.is_first_person_grounded(position, velocity.y) {
            velocity.y = FIRST_PERSON_JUMP_SPEED;
        }

        let (sy, cy) = self.camera.yaw.sin_cos();
        let forward = Vec3::new(-cy, 0.0, -sy).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();

        let mut wish = Vec3::ZERO;
        if pressed_inputs.contains("W") {
            wish += forward;
        }
        if pressed_inputs.contains("S") {
            wish -= forward;
        }
        if pressed_inputs.contains("D") {
            wish += right;
        }
        if pressed_inputs.contains("A") {
            wish -= right;
        }

        if wish.length_squared() > f32::EPSILON {
            wish = wish.normalize() * move_speed;
        }
        velocity.x = wish.x;
        velocity.z = wish.z;

        let (new_position, _) = self.physics.move_character_slide(
            position,
            velocity,
            dt,
            radius,
            FIRST_PERSON_GROUND_PROBE,
        );
        position = new_position;

        on_floor = self.is_first_person_grounded(position, velocity.y);
        if on_floor && velocity.y <= 0.0 {
            velocity.y = 0.0;
            // Snap solo en el plano base del mundo, no en plataformas elevadas.
            if position.y <= FIRST_PERSON_GROUND_REST_Y + FIRST_PERSON_FLOOR_EPSILON {
                position.y = FIRST_PERSON_GROUND_REST_Y;
            }
        }

        position = self
            .world_bounds_3d
            .clamp_sphere_center(position, radius);

        self.camera.target = position;
        self.camera.distance = 0.01;
        self.first_person_velocity = velocity;
        self.first_person_on_floor = on_floor;
        self.first_person_jump_queued = false;
    }

    pub(crate) fn clamp_first_person_camera_to_bounds(&mut self) {
        if self.camera_2d.is_some() {
            return;
        }

        self.camera.target = self.world_bounds_3d.clamp_sphere_center(
            self.camera.target,
            FIRST_PERSON_COLLIDER_RADIUS,
        );
        self.camera.distance = 0.01;
    }
}
