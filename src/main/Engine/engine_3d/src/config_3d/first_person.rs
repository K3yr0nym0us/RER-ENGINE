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

    fn player_exclude_collider(&self) -> Option<rapier3d::prelude::ColliderHandle> {
        self.first_person_player_entity
            .and_then(|id| self.physics.collider_handle_for_entity(id))
    }

    fn player_capsule_params(&self) -> Option<(f32, f32, glam::Quat, Vec3)> {
        let id = self.first_person_player_entity?;
        let t = self.world.get::<Transform>(id)?;
        let radius = FIRST_PERSON_COLLIDER_RADIUS;
        // Altura de cápsula fija (malla normalizada o cubo con scale.y=BODY_HEIGHT).
        let half_height =
            crate::config_3d::physics_3d::PhysicsWorld::capsule_half_height_from_scale(
                FIRST_PERSON_BODY_HEIGHT,
                radius,
            );
        Some((radius, half_height, t.rotation, t.position))
    }

    /// Pies del controller en play: eje mundo Y (independiente de rotación del mesh).
    fn controller_feet_from_center(center: Vec3) -> Vec3 {
        Vec3::new(
            center.x,
            center.y - FIRST_PERSON_BODY_HEIGHT * 0.5,
            center.z,
        )
    }

    fn center_from_controller_feet(feet: Vec3) -> Vec3 {
        Vec3::new(
            feet.x,
            feet.y + FIRST_PERSON_BODY_HEIGHT * 0.5,
            feet.z,
        )
    }

    /// Offset pivot→pies. La malla del jugador SIEMPRE mide `FIRST_PERSON_BODY_HEIGHT` (1.7m):
    /// el cubo placeholder lo logra vía `scale.y = 1.7`, los modelos importados se normalizan
    /// a 1.7m con `scale.y = 1.0`. Por eso el offset es constante (no depende de `scale_y`).
    fn feet_offset_local(_scale_y: f32, rotation: glam::Quat) -> Vec3 {
        rotation * Vec3::new(0.0, -FIRST_PERSON_BODY_HEIGHT * 0.5, 0.0)
    }
}

pub(crate) const FIRST_PERSON_KEYBOARD_SPEED: f32 = 4.0;
pub(crate) const FIRST_PERSON_SPRINT_MULTIPLIER: f32 = 3.0;
pub(crate) const FIRST_PERSON_MOUSE_SPEED: f32 = 0.0020;
pub(crate) const FIRST_PERSON_COLLIDER_RADIUS: f32 = 0.40;
pub(crate) const FIRST_PERSON_EYE_OFFSET: f32 = 1.35;
pub(crate) const FIRST_PERSON_JUMP_SPEED: f32 = 6.0;
pub(crate) const FIRST_PERSON_GROUND_PROBE: f32 = 0.08;
/// Altura del cubo-cuerpo del jugador (placeholder visual).
pub(crate) const FIRST_PERSON_BODY_HEIGHT: f32 = 1.7;
/// Distancia de la cámara orbital en editor (detrás del jugador, fuera de play).
pub(crate) const FIRST_PERSON_EDITOR_ORBIT_DISTANCE: f32 = 3.0;

pub(crate) fn feet_from_player_transform(center: Vec3, scale_y: f32, rotation: glam::Quat) -> Vec3 {
    center + State::feet_offset_local(scale_y, rotation)
}

pub(crate) fn player_center_from_feet(feet: Vec3, scale_y: f32, rotation: glam::Quat) -> Vec3 {
    feet - State::feet_offset_local(scale_y, rotation)
}

pub(crate) fn player_body_center_from_feet(feet: Vec3) -> Vec3 {
    player_center_from_feet(feet, FIRST_PERSON_BODY_HEIGHT, glam::Quat::IDENTITY)
}

impl State {
    /// Posición de los pies del jugador (base de la cápsula de movimiento).
    pub(crate) fn first_person_feet_position(&self) -> Vec3 {
        if let Some((_, _, rot, center)) = self.player_capsule_params() {
            if self.is_first_person_runtime_active() {
                return Self::controller_feet_from_center(center);
            }
            let scale_y = self
                .first_person_player_entity
                .and_then(|id| self.world.get::<Transform>(id))
                .map(|t| t.scale.y)
                .unwrap_or(FIRST_PERSON_BODY_HEIGHT);
            return feet_from_player_transform(center, scale_y, rot);
        }
        self.camera.target
    }

    pub(crate) fn set_first_person_feet_position(&mut self, feet: Vec3) {
        let in_play = self.is_first_person_runtime_active();
        if let Some(id) = self.first_person_player_entity {
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                if in_play {
                    t.position = Self::center_from_controller_feet(feet);
                } else {
                    let scale_y = t.scale.y;
                    let rot = t.rotation;
                    t.position = player_center_from_feet(feet, scale_y, rot);
                }
            }
        }
        self.camera.target = feet;
    }

    /// Alinea el mesh del jugador al yaw de la cámara (editor y play).
    /// En FPS estilo Godot, el cuerpo gira con la cámara para que mirar = orientarse.
    /// Solo aplica yaw (Y world); pitch queda en la cámara, no en el cuerpo.
    pub(crate) fn sync_player_rotation_from_look(&mut self) {
        if let Some(id) = self.first_person_player_entity {
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.rotation = glam::Quat::from_rotation_y(self.camera.yaw);
            }
        }
    }

    /// Inverso de `sync_player_rotation_from_look`: toma el yaw del cuerpo y lo asigna a la
    /// cámara. Se invoca al entrar a Play para que `apply_first_person_mouse_look` no haga
    /// "snap" del cuerpo al primer movimiento de mouse (perdiendo la rotación que el usuario
    /// fijó en el editor por Propiedades o herramientas).
    pub(crate) fn sync_camera_yaw_from_player_body(&mut self) {
        let Some(id) = self.first_person_player_entity else {
            return;
        };
        let Some(t) = self.world.get::<Transform>(id) else {
            return;
        };
        let (yaw, _, _) = t.rotation.to_euler(glam::EulerRot::YXZ);
        self.camera.yaw = yaw;
    }

    /// Pose visual de la cámara FP para dibujar el gizmo de frustum en el editor.
    /// Devuelve `(ojo_mundo, yaw_cuerpo, pitch_inicial)`. El yaw sale del Transform
    /// del cuerpo (no de `camera.yaw`, que en editor es el ángulo del orbit) y el
    /// pitch es 0 — así el gizmo muestra exactamente la orientación que tendrá la
    /// cámara en el primer frame de Play.
    pub(crate) fn first_person_camera_gizmo_pose(&self) -> Option<(Vec3, f32, f32)> {
        let id = self.first_person_player_entity?;
        let t = self.world.get::<Transform>(id)?;
        let (yaw, _, _) = t.rotation.to_euler(glam::EulerRot::YXZ);
        let eye = self.first_person_feet_position() + self.first_person_eye_world_offset();
        Some((eye, yaw, 0.0))
    }

    pub(crate) fn is_first_person_runtime_active(&self) -> bool {
        self.preview_playing && self.camera_2d.is_none()
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
        self.first_person_on_floor = false;
        self.first_person_jump_queued = false;
        self.first_person_jump_request_active = false;
        self.first_person_jump_request_prev = false;
    }

    /// El jugador FP no debe tener cuerpo Rapier (solo cápsula cinemática por queries).
    pub(crate) fn ensure_fp_player_kinematic_only(&mut self) {
        let Some(id) = self.first_person_player_entity else {
            return;
        };
        self.physics.remove_entity_body(id);
    }

    pub(crate) fn has_first_person_player(&self) -> bool {
        self.first_person_player_entity.is_some() && self.camera_2d.is_none()
    }

    /// Solicita salto. NO aplica velocidad aquí (eso lo decide `apply_first_person_keyboard`
    /// con detección de flanco al estilo Godot `is_action_just_pressed`).
    pub(crate) fn queue_first_person_jump(&mut self) {
        if !self.is_first_person_runtime_active() {
            return;
        }
        self.first_person_jump_request_active = true;
    }

    /// Eye offset SIEMPRE en world Y (estilo Godot Camera3D anclado al pivote del jugador).
    /// No rotar por la rotación del mesh: el usuario controla la cámara con yaw/pitch,
    /// y el cuerpo visual puede tener cualquier rotación sin afectar la altura de ojos.
    pub(crate) fn first_person_eye_world_offset(&self) -> Vec3 {
        Vec3::new(0.0, FIRST_PERSON_EYE_OFFSET, 0.0)
    }

    pub(crate) fn sync_first_person_camera_mode(&mut self) {
        if self.is_first_person_runtime_active() {
            self.camera.orbit_pivot_offset = Vec3::ZERO;
            self.camera.eye_height_offset = 0.0;
            self.camera.eye_offset_local = self.first_person_eye_world_offset();
            self.camera.distance = 0.01;
        } else if self.has_first_person_player() {
            self.camera.orbit_pivot_offset =
                Vec3::new(0.0, FIRST_PERSON_EYE_OFFSET, 0.0);
            self.camera.eye_height_offset = 0.0;
            self.camera.eye_offset_local = Vec3::ZERO;
            self.camera.distance = FIRST_PERSON_EDITOR_ORBIT_DISTANCE;
        } else {
            self.camera.orbit_pivot_offset = Vec3::ZERO;
            self.camera.eye_height_offset = 0.0;
            self.camera.eye_offset_local = Vec3::ZERO;
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
        self.first_person_on_floor = false;
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
        // Rotar el cuerpo del jugador con el yaw (estilo Godot CharacterBody3D + Head).
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

        let Some((radius, half_height, _, _)) = self.player_capsule_params() else {
            return;
        };
        let up = Vec3::Y;
        let exclude = self.player_exclude_collider();

        let dt = delta_time.min(0.05);
        let move_speed = self.first_person_move_speed(pressed_inputs);
        let mut feet = self.first_person_feet_position();
        let mut velocity = self.first_person_velocity;

        // Edge detection estilo Godot `is_action_just_pressed`: el script SPACE corre cada
        // frame mientras esté pulsado; solo el flanco de subida cuenta como un nuevo salto.
        let jump_held_now = self.first_person_jump_request_active
            || pressed_inputs.contains("SPACE");
        let jump_just_pressed = jump_held_now && !self.first_person_jump_request_prev;
        self.first_person_jump_request_prev = jump_held_now;
        self.first_person_jump_request_active = false;

        // Godot _physics_process: is_on_floor del frame anterior (tras move_and_slide).
        let mut on_floor = self.first_person_on_floor;
        let gravity = self.physics.gravity_magnitude();

        // 1) Salto solo en flanco y estando en suelo.
        if jump_just_pressed && on_floor {
            velocity.y = self.first_person_jump_speed();
            on_floor = false;
        }

        // 2) Gravedad si no está en suelo (estilo Godot).
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
            FIRST_PERSON_GROUND_PROBE,
            exclude,
        );
        feet = new_feet;
        on_floor = slide_on_floor;

        if on_floor && velocity.y < 0.0 {
            velocity.y = 0.0;
        }

        feet = self.world_bounds_3d.clamp_sphere_center(feet, 0.0);

        self.set_first_person_feet_position(feet);
        self.camera.distance = 0.01;
        self.sync_first_person_camera_mode();
        self.first_person_velocity = velocity;
        self.first_person_on_floor = on_floor;
        self.first_person_jump_queued = false;
    }


    pub(crate) fn clamp_first_person_camera_to_bounds(&mut self) {
        if self.camera_2d.is_some() {
            return;
        }

        let feet = self
            .world_bounds_3d
            .clamp_sphere_center(self.first_person_feet_position(), 0.0);
        self.set_first_person_feet_position(feet);
        if self.is_first_person_runtime_active() {
            self.camera.distance = 0.01;
            self.sync_first_person_camera_mode();
        }
    }
}
