use std::collections::HashSet;

use glam::Vec3;

use crate::config_3d::character_anchor::{
    PLAY_CHARACTER_GROUND_PROBE, PLAY_CHARACTER_JUMP_SPEED, PLAY_CHARACTER_KEYBOARD_SPEED,
    PLAY_CHARACTER_SPRINT_MULTIPLIER,
};
use crate::engine::State;

impl State {
    pub(crate) fn clear_play_controller_script_frame(&mut self) {
        self.play_controller_script_input.clear();
        self.play_controller_lua_walk_speed = None;
        self.play_controller_lua_sprint_multiplier = None;
        self.play_controller_lua_jump_speed = None;
    }

    pub(crate) fn uses_scripted_play_controller(&self) -> bool {
        let Some(player_id) = self.play_character_entity else {
            return false;
        };
        self.control_bindings_by_entity
            .get(&player_id)
            .map(|bindings| !bindings.keyboard_mouse.is_empty())
            .unwrap_or(false)
    }

    pub(crate) fn play_controller_effective_inputs(
        &self,
        hardware_pressed: &HashSet<String>,
    ) -> HashSet<String> {
        if self.uses_scripted_play_controller() {
            self.play_controller_script_input.clone()
        } else {
            hardware_pressed.clone()
        }
    }

    fn play_controller_jump_speed(&self) -> f32 {
        self.play_controller_lua_jump_speed
            .unwrap_or(PLAY_CHARACTER_JUMP_SPEED)
    }

    pub(crate) fn is_play_controller_active(&self) -> bool {
        self.preview_playing
    }

    fn play_controller_move_speed(&self, pressed_inputs: &HashSet<String>) -> f32 {
        let mut speed = self
            .play_controller_lua_walk_speed
            .unwrap_or(PLAY_CHARACTER_KEYBOARD_SPEED);
        if pressed_inputs.contains("SHIFT") {
            let sprint = self
                .play_controller_lua_sprint_multiplier
                .unwrap_or(PLAY_CHARACTER_SPRINT_MULTIPLIER);
            speed *= sprint;
        }
        speed
    }

    pub(crate) fn reset_play_controller_motion(&mut self) {
        self.play_controller_velocity = Vec3::ZERO;
        self.play_controller_on_floor = false;
        self.play_controller_jump_queued = false;
        self.play_controller_jump_request_active = false;
        self.play_controller_jump_request_prev = false;
    }

    /// Solicita salto. NO aplica velocidad aquí (eso lo decide `apply_play_controller_keyboard`).
    pub(crate) fn queue_play_controller_jump(&mut self) {
        if !self.is_play_controller_active() {
            return;
        }
        self.play_controller_jump_request_active = true;
    }

    pub(crate) fn apply_play_controller_keyboard(
        &mut self,
        pressed_inputs: &HashSet<String>,
        delta_time: f32,
    ) {
        if !self.is_play_controller_active() || delta_time <= 0.0 {
            return;
        }

        let Some((radius, half_height, _, _)) = self.play_character_capsule_for_controller()
        else {
            return;
        };
        let up = Vec3::Y;
        let exclude = self.play_character_exclude_collider_for_controller();

        let dt = delta_time.min(0.05);
        let move_speed = self.play_controller_move_speed(pressed_inputs);
        let mut feet = self.play_character_feet_position();
        let old_feet = feet;
        let mut velocity = self.play_controller_velocity;

        let jump_held_now = self.play_controller_jump_request_active
            || pressed_inputs.contains("SPACE");
        let jump_just_pressed = jump_held_now && !self.play_controller_jump_request_prev;
        self.play_controller_jump_request_prev = jump_held_now;
        self.play_controller_jump_request_active = false;

        let mut on_floor = self.play_controller_on_floor;
        let gravity = self.physics.gravity_magnitude();

        if jump_just_pressed && on_floor {
            velocity.y = self.play_controller_jump_speed();
            on_floor = false;
        }

        if !on_floor {
            velocity.y -= gravity * dt;
        } else if velocity.y < 0.0 {
            velocity.y = 0.0;
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

        let (new_feet, slide_on_floor) = self.physics.move_character_capsule_at_feet(
            feet,
            up,
            velocity,
            dt,
            radius,
            half_height,
            PLAY_CHARACTER_GROUND_PROBE,
            exclude,
        );
        feet = new_feet;
        on_floor = slide_on_floor;

        if on_floor && velocity.y < 0.0 {
            velocity.y = 0.0;
        }

        feet = self.world_bounds_3d.clamp_sphere_center(feet, 0.0);

        self.set_play_character_feet_position(feet);
        self.sync_play_camera_on_player_feet_moved(old_feet, feet);
        self.play_controller_velocity = velocity;
        self.play_controller_on_floor = on_floor;
        self.play_controller_jump_queued = false;
    }
}
