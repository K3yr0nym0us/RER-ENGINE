use std::collections::HashSet;

use glam::Vec3;

use crate::ecs::Transform;
use crate::engine::State;

impl State {
    pub(crate) fn clear_first_person_script_frame(&mut self) {
        self.first_person_script_input.clear();
        self.first_person_lua_walk_speed = None;
        self.first_person_lua_sprint_multiplier = None;
        self.first_person_lua_jump_speed = None;
    }

    pub(crate) fn uses_scripted_first_person_controls(&self) -> bool {
        let Some(player_id) = self.first_person_player_entity else {
            return false;
        };
        self.control_bindings_by_entity
            .get(&player_id)
            .map(|bindings| !bindings.keyboard_mouse.is_empty())
            .unwrap_or(false)
    }

    pub(crate) fn first_person_effective_inputs(
        &self,
        hardware_pressed: &HashSet<String>,
    ) -> HashSet<String> {
        if self.uses_scripted_first_person_controls() {
            self.first_person_script_input.clone()
        } else {
            hardware_pressed.clone()
        }
    }

    fn first_person_jump_speed(&self) -> f32 {
        self.first_person_lua_jump_speed
            .unwrap_or(FIRST_PERSON_JUMP_SPEED)
    }
}

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
/// Altura del cubo-cuerpo del jugador (placeholder visual).
pub(crate) const FIRST_PERSON_BODY_HEIGHT: f32 = 1.7;
pub(crate) const FIRST_PERSON_BODY_HALF_H: f32 = FIRST_PERSON_BODY_HEIGHT * 0.5;
/// Distancia de la cámara orbital en editor (detrás del jugador, fuera de play).
pub(crate) const FIRST_PERSON_EDITOR_ORBIT_DISTANCE: f32 = 3.0;

pub(crate) fn player_body_center_from_feet(feet: Vec3) -> Vec3 {
    feet + Vec3::new(0.0, FIRST_PERSON_BODY_HALF_H, 0.0)
}

pub(crate) fn feet_from_player_body_center(center: Vec3) -> Vec3 {
    center - Vec3::new(0.0, FIRST_PERSON_BODY_HALF_H, 0.0)
}

impl State {
    /// Posición de los pies del jugador (= cámara FP). Fuente única con la entidad Player.
    pub(crate) fn first_person_feet_position(&self) -> Vec3 {
        if let Some(id) = self.first_person_player_entity {
            if let Some(t) = self.world.get::<Transform>(id) {
                return feet_from_player_body_center(t.position);
            }
        }
        self.camera.target
    }

    pub(crate) fn set_first_person_feet_position(&mut self, feet: Vec3) {
        if let Some(id) = self.first_person_player_entity {
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = player_body_center_from_feet(feet);
            }
        }
        self.camera.target = feet;
    }

    pub(crate) fn sync_player_rotation_from_look(&mut self) {
        if let Some(id) = self.first_person_player_entity {
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.rotation = glam::Quat::from_rotation_y(self.camera.yaw);
            }
        }
    }

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
        let mut speed = self
            .first_person_lua_walk_speed
            .unwrap_or(FIRST_PERSON_KEYBOARD_SPEED);
        if pressed_inputs.contains("SHIFT") {
            let sprint = self
                .first_person_lua_sprint_multiplier
                .unwrap_or(FIRST_PERSON_SPRINT_MULTIPLIER);
            speed *= sprint;
        }
        speed
    }

    pub(crate) fn reset_first_person_motion(&mut self) {
        self.first_person_velocity = Vec3::ZERO;
        self.first_person_on_floor = true;
        self.first_person_jump_queued = false;
    }

    pub(crate) fn has_first_person_player(&self) -> bool {
        self.first_person_player_entity.is_some() && self.camera_2d.is_none()
    }

    pub(crate) fn queue_first_person_jump(&mut self) {
        if !self.is_first_person_runtime_active() {
            return;
        }
        let position = self.first_person_feet_position();
        let velocity_y = self.first_person_velocity.y;
        if self.is_first_person_grounded(position, velocity_y) {
            self.first_person_velocity.y = self.first_person_jump_speed();
            self.first_person_on_floor = false;
        }
        self.first_person_jump_queued = true;
    }

    pub(crate) fn sync_first_person_camera_mode(&mut self) {
        if self.is_first_person_runtime_active() {
            self.camera.orbit_pivot_offset = Vec3::ZERO;
            self.camera.eye_height_offset = FIRST_PERSON_EYE_OFFSET;
            self.camera.distance = 0.01;
        } else if self.has_first_person_player() {
            // Editor: orbitar a la altura de los ojos (misma altura que en play).
            self.camera.orbit_pivot_offset =
                Vec3::new(0.0, FIRST_PERSON_EYE_OFFSET, 0.0);
            self.camera.eye_height_offset = 0.0;
            self.camera.distance = FIRST_PERSON_EDITOR_ORBIT_DISTANCE;
        } else {
            self.camera.orbit_pivot_offset = Vec3::ZERO;
            self.camera.eye_height_offset = 0.0;
        }
    }

    /// Aplica vista guardada (carga de proyecto). Funciona en editor y en preview.
    pub(crate) fn apply_first_person_saved_view(
        &mut self,
        position: [f32; 3],
        yaw: f32,
        pitch: f32,
    ) {
        if self.camera_2d.is_some() {
            return;
        }

        self.set_first_person_feet_position(Vec3::from_array(position));
        self.camera.yaw = yaw;
        self.camera.pitch = pitch.clamp(
            -std::f32::consts::FRAC_PI_2 + 0.05,
            std::f32::consts::FRAC_PI_2 - 0.05,
        );
        self.sync_player_rotation_from_look();

        self.sync_first_person_camera_mode();

        self.first_person_velocity = Vec3::ZERO;
        self.first_person_on_floor =
            self.is_first_person_grounded(self.first_person_feet_position(), 0.0);
        self.clamp_first_person_camera_to_bounds();
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
        self.sync_player_rotation_from_look();
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
        let mut position = self.first_person_feet_position();
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
            velocity.y = self.first_person_jump_speed();
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

        self.set_first_person_feet_position(position);
        self.camera.distance = 0.01;
        self.first_person_velocity = velocity;
        self.first_person_on_floor = on_floor;
        self.first_person_jump_queued = false;
    }

    pub(crate) fn clamp_first_person_camera_to_bounds(&mut self) {
        if self.camera_2d.is_some() {
            return;
        }

        let feet = self.world_bounds_3d.clamp_sphere_center(
            self.first_person_feet_position(),
            FIRST_PERSON_COLLIDER_RADIUS,
        );
        self.set_first_person_feet_position(feet);
        if self.is_first_person_runtime_active() {
            self.camera.distance = 0.01;
        }
    }
}
