use glam::Vec3;

use crate::config_3d::character_anchor::{
    body_center_from_feet, PLAY_CHARACTER_EDITOR_ORBIT_DISTANCE, PLAY_CHARACTER_EYE_OFFSET,
    PLAY_CHARACTER_MOUSE_SPEED,
};
use crate::ecs::Transform;
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};

/// Forward de la cámara proyectado al plano XZ. Coincide con `Camera::view_forward` con pitch=0.
fn look_xz_from_camera_yaw(camera_yaw: f32) -> glam::Vec2 {
    let (sy, cy) = camera_yaw.sin_cos();
    glam::Vec2::new(-cy, -sy)
}

/// Yaw del mesh para que su forward local apunte hacia donde mira la cámara (offset 0).
pub(crate) fn mesh_yaw_from_camera_and_forward(
    camera_yaw: f32,
    mesh_forward_xz: glam::Vec2,
) -> f32 {
    let look = look_xz_from_camera_yaw(camera_yaw);
    mesh_forward_xz.y.atan2(mesh_forward_xz.x) - look.y.atan2(look.x)
}

impl State {
    /// Editor 3D (sin play activo): el viewport usa `editor_orbit_target` + `editor_viewport_*`.
    pub(crate) fn uses_editor_viewport_camera(&self) -> bool {
        self.camera_2d.is_none() && !self.is_play_controller_active()
    }

    /// Jugador FP en editor: el transform del mesh no mueve el viewport.
    pub(crate) fn editor_orbit_decoupled_from_player(&self) -> bool {
        self.uses_editor_viewport_camera() && self.has_play_character()
    }

    pub(crate) fn viewport_orbit_angles(&self) -> (f32, f32, f32) {
        if self.uses_editor_viewport_camera() {
            (
                self.editor_viewport_yaw,
                self.editor_viewport_pitch,
                self.editor_viewport_distance,
            )
        } else {
            (self.camera.yaw, self.camera.pitch, self.camera.distance)
        }
    }

    /// Copia la vista orbital actual a los campos del editor (al cargar / salir de play).
    /// Fija el blanco orbital del editor (una vez al crear el jugador o al orbitar manualmente).
    pub(crate) fn init_editor_viewport_for_player(&mut self, body_center: Vec3) {
        if !self.uses_editor_viewport_camera() {
            return;
        }
        self.editor_orbit_target = body_center;
        self.editor_viewport_yaw = self.camera.yaw;
        self.editor_viewport_pitch = self.camera.pitch;
        if self.editor_viewport_distance < 0.5 {
            self.editor_viewport_distance = PLAY_CHARACTER_EDITOR_ORBIT_DISTANCE;
        }
        // Posición inicial del ojo de la cámara FPS: en el ojo del Player. Una vez aquí,
        // el ojo es independiente: mover al Player en editor no la altera.
        self.play_camera_eye_position =
            self.play_character_feet_position() + self.play_character_eye_world_offset();
        self.capture_play_camera_follow_offset();
        self.ensure_editor_camera_entity();
        self.sync_editor_camera_entity_from_viewport();
    }

    /// Punto al que mira la órbita del editor (pivote de `look_at`, no la posición del ojo).
    pub(crate) fn editor_orbit_look_at_pivot(&self) -> Vec3 {
        let pivot = if let Some(id) = self.editor_camera_entity {
            if let Some(t) = self.world.get::<Transform>(id) {
                t.position
            } else {
                self.editor_orbit_target
            }
        } else {
            self.editor_orbit_target
        };
        pivot
    }

    /// Punto al que mira la cámara orbital del viewport.
    pub(crate) fn orbit_view_anchor(&self) -> Vec3 {
        if self.uses_editor_viewport_camera() {
            return self.editor_orbit_look_at_pivot();
        }
        self.camera.target
    }

    pub(crate) fn pan_editor_viewport(&mut self, dx: f32, dy: f32) {
        if self.camera_2d.is_some() {
            return;
        }
        let offset = self.camera.pan_offset_with_distance(
            self.editor_orbit_target,
            dx,
            dy,
            self.editor_viewport_distance,
            self.editor_viewport_yaw,
            self.editor_viewport_pitch,
        );
        self.editor_orbit_target += offset;
        self.sync_editor_camera_entity_from_viewport();
    }

    pub(crate) fn orbit_editor_viewport(&mut self, dx: f32, dy: f32) {
        const SENSITIVITY: f32 = 0.005;
        self.editor_viewport_yaw += dx * SENSITIVITY;
        self.editor_viewport_pitch = (self.editor_viewport_pitch - dy * SENSITIVITY).clamp(
            -std::f32::consts::FRAC_PI_2 + 0.05,
            std::f32::consts::FRAC_PI_2 - 0.05,
        );
        self.sync_editor_camera_entity_from_viewport();
    }

    pub(crate) fn zoom_editor_viewport(&mut self, delta: f32) {
        self.editor_viewport_distance =
            (self.editor_viewport_distance - delta * 0.3).clamp(0.5, 500.0);
        self.sync_editor_camera_entity_from_viewport();
    }

    /// Play: única cámara de juego = acordeón Cámara (`play_camera_eye_position` + yaw/pitch).
    pub(crate) fn uses_play_accordion_camera(&self) -> bool {
        self.is_play_controller_active() && self.has_play_character()
    }

    pub(crate) fn camera_view_matrix(&self) -> glam::Mat4 {
        if self.uses_play_accordion_camera() {
            return self.camera.view_matrix_from_eye(
                self.play_camera_eye_position,
                self.camera.yaw,
                self.camera.pitch,
            );
        }
        let anchor = self.orbit_view_anchor();
        let (yaw, pitch, dist) = self.viewport_orbit_angles();
        self.camera.view_matrix_at_angles(anchor, yaw, pitch, dist)
    }

    pub(crate) fn camera_world_position(&self) -> Vec3 {
        if self.uses_play_accordion_camera() {
            return self.play_camera_eye_position;
        }
        let anchor = self.orbit_view_anchor();
        let (yaw, pitch, dist) = self.viewport_orbit_angles();
        self.camera.position_at_angles(anchor, yaw, pitch, dist)
    }

    pub(crate) fn camera_to_uniform_at_anchor(&self, anchor: Vec3, aspect: f32) -> crate::config_3d::camera_3d::CameraUniform {
        let view = if self.uses_play_accordion_camera() {
            self.camera.view_matrix_from_eye(
                self.play_camera_eye_position,
                self.camera.yaw,
                self.camera.pitch,
            )
        } else {
            let (yaw, pitch, dist) = self.viewport_orbit_angles();
            self.camera.view_matrix_at_angles(anchor, yaw, pitch, dist)
        };
        crate::config_3d::camera_3d::CameraUniform {
            view_proj: (self.camera.proj_matrix(aspect) * view).to_cols_array_2d(),
        }
    }

    /// Pose visual de la cámara FPS para dibujar el gizmo de frustum en el editor.
    ///
    /// Posición/yaw/pitch vienen del estado independiente de la cámara FPS
    /// (`self.play_camera_eye_position`, `self.camera.yaw/pitch`).
    /// Mover o rotar al Player en el panel Transform NO afecta este gizmo.
    pub(crate) fn play_character_camera_gizmo_pose(&self) -> Option<(Vec3, f32, f32)> {
        let _ = self.play_character_entity?;
        Some((self.play_camera_eye_position, self.camera.yaw, self.camera.pitch))
    }

    /// Eye offset SIEMPRE en world Y (estilo Godot Camera3D anclado al pivote del jugador).
    pub(crate) fn play_character_eye_world_offset(&self) -> Vec3 {
        Vec3::new(0.0, PLAY_CHARACTER_EYE_OFFSET, 0.0)
    }

    /// Centra la órbita del editor en la selección (jugador incluido).
    pub(crate) fn sync_editor_camera_focus(&mut self) {
        if self.camera_2d.is_some() || self.is_play_controller_active() {
            return;
        }

        if let Some(center) = self.selection_center() {
            if self.uses_editor_viewport_camera() {
                self.editor_orbit_target = center;
                self.ensure_editor_camera_entity();
                self.sync_editor_camera_entity_from_viewport();
                self.camera.orbit_pivot_offset = Vec3::ZERO;
            } else {
                let focus_player = self
                    .selected_entity
                    .and_then(|id| self.play_character_entity.map(|p| p == id))
                    .unwrap_or(false);
                if focus_player {
                    self.camera.target = self.play_character_feet_position();
                    self.camera.orbit_pivot_offset = Vec3::new(0.0, PLAY_CHARACTER_EYE_OFFSET, 0.0);
                } else {
                    self.camera.target = center;
                    self.camera.orbit_pivot_offset = Vec3::ZERO;
                }
            }
        } else if self.has_play_character() && !self.editor_orbit_decoupled_from_player() {
            self.camera.target = self.play_character_feet_position();
            self.camera.orbit_pivot_offset = Vec3::new(0.0, PLAY_CHARACTER_EYE_OFFSET, 0.0);
        } else {
            self.camera.orbit_pivot_offset = Vec3::ZERO;
        }
    }

    pub(crate) fn sync_fps_camera_mode(&mut self) {
        if self.is_play_controller_active() {
            // Play: sin cámara de ojos del player; render desde acordeón Cámara.
            self.camera.orbit_pivot_offset = Vec3::ZERO;
            self.camera.eye_height_offset = 0.0;
            self.camera.eye_offset_local = Vec3::ZERO;
            return;
        } else if self.has_play_character() {
            self.camera.eye_height_offset = 0.0;
            self.camera.eye_offset_local = Vec3::ZERO;
            if self.editor_viewport_distance < 0.5 {
                self.editor_viewport_distance = PLAY_CHARACTER_EDITOR_ORBIT_DISTANCE;
                self.camera.distance = self.editor_viewport_distance;
            }
            self.sync_editor_camera_focus();
        } else {
            self.camera.orbit_pivot_offset = Vec3::ZERO;
            self.camera.eye_height_offset = 0.0;
            self.camera.eye_offset_local = Vec3::ZERO;
        }
    }

    /// Aplica vista guardada (carga de proyecto). Funciona en editor y en preview.
    pub(crate) fn apply_play_character_saved_view(
        &mut self,
        position: [f32; 3],
        yaw: f32,
        pitch: f32,
        sync_editor_viewport: bool,
    ) {
        if self.camera_2d.is_some() {
            return;
        }

        self.set_play_character_feet_position(Vec3::from_array(position));
        let pitch_clamped = pitch.clamp(
            -std::f32::consts::FRAC_PI_2 + 0.05,
            std::f32::consts::FRAC_PI_2 - 0.05,
        );
        self.camera.yaw = yaw;
        self.camera.pitch = pitch_clamped;
        if sync_editor_viewport && self.uses_editor_viewport_camera() {
            self.editor_orbit_target = body_center_from_feet(Vec3::from_array(position));
            self.editor_viewport_yaw = yaw;
            self.editor_viewport_pitch = pitch_clamped;
            // Reset del ojo de la cámara FPS al ojo actual del Player al cargar/aplicar vista.
            self.play_camera_eye_position =
                self.play_character_feet_position() + self.play_character_eye_world_offset();
            self.ensure_editor_camera_entity();
            self.sync_editor_camera_entity_from_viewport();
        }
        // En editor: NO girar el mesh del jugador al editar la cámara (entidades desacopladas).
        // En play: el mouse-look usa `apply_fps_mouse_look` que sí alinea cuerpo ↔ cámara.
        if self.is_play_controller_active() {
            self.sync_player_rotation_from_look();
        }
        self.sync_fps_camera_mode();

        self.play_controller_velocity = Vec3::ZERO;
        self.play_controller_on_floor = false;
        self.clamp_play_character_camera_to_bounds();
    }

    pub(crate) fn play_character_head_world(&self) -> Vec3 {
        self.play_character_feet_position() + self.play_character_eye_world_offset()
    }

    pub(crate) fn play_character_body_yaw(&self) -> f32 {
        let Some(id) = self.play_character_entity else {
            return 0.0;
        };
        let Some(t) = self.world.get::<Transform>(id) else {
            return 0.0;
        };
        t.rotation.to_euler(glam::EulerRot::YXZ).0
    }

    fn follow_offset_world_from_local(local: Vec3, body_yaw: f32) -> Vec3 {
        let (sy, cy) = body_yaw.sin_cos();
        Vec3::new(
            local.x * cy - local.z * sy,
            local.y,
            local.x * sy + local.z * cy,
        )
    }

    fn follow_offset_local_from_world(world: Vec3, body_yaw: f32) -> Vec3 {
        let (sy, cy) = body_yaw.sin_cos();
        Vec3::new(
            world.x * cy + world.z * sy,
            world.y,
            -world.x * sy + world.z * cy,
        )
    }

    pub(crate) fn capture_play_camera_follow_offset(&mut self) {
        if !self.has_play_character() {
            return;
        }
        let head = self.play_character_head_world();
        let world = self.play_camera_eye_position - head;
        self.play_camera_follow_offset = world;
        self.play_camera_follow_offset_local =
            Self::follow_offset_local_from_world(world, self.play_character_body_yaw());
    }

    fn resolve_play_camera_eye_line_of_sight(&mut self, focus: Vec3, desired_eye: Vec3) -> Vec3 {
        let offset = desired_eye - focus;
        let dist = offset.length();
        if dist < 1e-4 {
            return desired_eye;
        }
        let exclude = self.play_character_exclude_collider();
        const MARGIN: f32 = 0.12;
        if let Some(hit_dist) = self.physics.raycast_first_hit_distance(
            focus,
            offset,
            dist,
            exclude,
        ) {
            let t = (hit_dist - MARGIN).max(0.0);
            return focus + offset.normalize() * t;
        }
        desired_eye
    }

    pub(crate) fn apply_follow_character_camera_snap(&mut self) {
        if self.camera_2d.is_some() {
            return;
        }
        let head = self.play_character_head_world();
        let world_offset = Self::follow_offset_world_from_local(
            self.play_camera_follow_offset_local,
            self.play_character_body_yaw(),
        );
        let desired = head + world_offset;
        self.play_camera_eye_position =
            self.resolve_play_camera_eye_line_of_sight(head, desired);
        self.play_camera_follow_offset = self.play_camera_eye_position - head;
    }

    pub(crate) fn sync_play_camera_on_player_feet_moved(&mut self, old_feet: Vec3, new_feet: Vec3) {
        if self.camera_2d.is_some() {
            return;
        }
        let delta = new_feet - old_feet;
        if delta.length_squared() < 1e-10 {
            return;
        }
        match self.play_camera_follow_mode {
            crate::ipc::PlayCameraFollowMode::MoveWithCharacter => {
                self.play_camera_eye_position += delta;
            }
            crate::ipc::PlayCameraFollowMode::FollowCharacter => {
                self.apply_follow_character_camera_snap();
            }
        }
    }

    pub(crate) fn set_play_camera_follow_mode(&mut self, mode: crate::ipc::PlayCameraFollowMode) {
        if self.play_camera_follow_mode == mode {
            return;
        }
        self.play_camera_follow_mode = mode;
        self.capture_play_camera_follow_offset();
        if mode == crate::ipc::PlayCameraFollowMode::FollowCharacter {
            self.apply_follow_character_camera_snap();
        }
        log::info!(
            "[cámara] modo de seguimiento: {}",
            match mode {
                crate::ipc::PlayCameraFollowMode::FollowCharacter => "seguir personaje",
                crate::ipc::PlayCameraFollowMode::MoveWithCharacter => "moverse junto al personaje",
            }
        );
    }

    /// Aplica cambios parciales de la cámara FPS en editor (sin tocar al Player).
    pub(crate) fn apply_play_camera_view_patch(
        &mut self,
        position_axis: Option<crate::ipc::AxisValue>,
        yaw: Option<f32>,
        pitch: Option<f32>,
        fov_y: Option<f32>,
        frustum_distance: Option<f32>,
        camera_follow_mode: Option<crate::ipc::PlayCameraFollowMode>,
    ) {
        if self.camera_2d.is_some() {
            return;
        }
        if let Some(av) = position_axis {
            let mut eye = self.play_camera_eye_position;
            match av.axis {
                0 => eye.x = av.value,
                1 => eye.y = av.value,
                2 => eye.z = av.value,
                _ => {}
            }
            self.play_camera_eye_position = eye;
            self.capture_play_camera_follow_offset();
        }
        if let Some(mode) = camera_follow_mode {
            self.set_play_camera_follow_mode(mode);
        }
        if let Some(y) = yaw {
            self.camera.yaw = y;
        }
        if let Some(p) = pitch {
            self.camera.pitch = p.clamp(
                -std::f32::consts::FRAC_PI_2 + 0.05,
                std::f32::consts::FRAC_PI_2 - 0.05,
            );
        }
        if let Some(fov) = fov_y {
            self.camera.fov_y = fov.clamp(0.1, std::f32::consts::FRAC_PI_2 - 0.01);
        }
        if let Some(dist) = frustum_distance {
            self.fps_editor_frustum_distance = dist.clamp(0.5, 50.0);
        }
        self.emit_play_character_view_changed(false);
    }

    /// Aplica vista (pies + cámara + opcional FOV/frustum) y notifica al frontend.
    pub(crate) fn apply_play_character_view(
        &mut self,
        position: [f32; 3],
        yaw: f32,
        pitch: f32,
        fov_y: Option<f32>,
        frustum_distance: Option<f32>,
        camera_follow_mode: Option<crate::ipc::PlayCameraFollowMode>,
    ) {
        if self.camera_2d.is_some() {
            return;
        }
        self.apply_play_character_saved_view(position, yaw, pitch, true);
        if let Some(fov) = fov_y {
            self.camera.fov_y = fov.clamp(0.1, std::f32::consts::FRAC_PI_2 - 0.01);
        }
        if let Some(dist) = frustum_distance {
            self.fps_editor_frustum_distance = dist.clamp(0.5, 50.0);
        }
        if let Some(mode) = camera_follow_mode {
            self.set_play_camera_follow_mode(mode);
        } else {
            self.capture_play_camera_follow_offset();
        }
        self.emit_play_character_view_changed(true);
    }

    /// Emite la vista actual para que el frontend no derive poses en TypeScript.
    pub(crate) fn emit_play_character_view_changed(&self, sync_editor_viewport: bool) {
        if self.camera_2d.is_some() || !self.has_play_character() {
            return;
        }
        let player_id = self.play_character_entity;
        let Some(id) = player_id else {
            return;
        };
        let feet = self.play_character_feet_position();
        let (body_center, body_rotation, body_scale) =
            if let Some(t) = self.world.get::<Transform>(id) {
                (
                    t.position.to_array(),
                    [
                        t.rotation.x,
                        t.rotation.y,
                        t.rotation.z,
                        t.rotation.w,
                    ],
                    t.scale.to_array(),
                )
            } else {
                return;
            };
        let (yaw, pitch) = if self.uses_editor_viewport_camera() {
            (self.editor_viewport_yaw, self.editor_viewport_pitch)
        } else {
            (self.camera.yaw, self.camera.pitch)
        };
        let editor_orbit_target = self.editor_camera_entity.map(|_| self.editor_orbit_target.to_array());
        send_event(&EngineEvent::PlayCharacterViewChanged {
            player_id,
            editor_camera_id: self.editor_camera_entity,
            editor_orbit_target,
            position: feet.to_array(),
            camera_eye_position: self.play_camera_eye_position.to_array(),
            fps_camera_yaw: self.camera.yaw,
            fps_camera_pitch: self.camera.pitch,
            yaw,
            pitch,
            fov_y: self.camera.fov_y,
            frustum_distance: self.fps_editor_frustum_distance,
            camera_follow_mode: self.play_camera_follow_mode,
            body_center,
            body_rotation,
            body_scale,
            sync_editor_viewport,
        });
    }

    pub(crate) fn apply_fps_mouse_look(&mut self, dx: f32, dy: f32) {
        if !self.is_play_controller_active() {
            return;
        }

        self.camera.yaw += dx * PLAY_CHARACTER_MOUSE_SPEED;
        self.camera.pitch = (self.camera.pitch + dy * PLAY_CHARACTER_MOUSE_SPEED).clamp(
            -std::f32::consts::FRAC_PI_2 + 0.05,
            std::f32::consts::FRAC_PI_2 - 0.05,
        );
        self.sync_player_rotation_from_look();
    }

    pub(crate) fn clamp_play_character_camera_to_bounds(&mut self) {
        if self.camera_2d.is_some() {
            return;
        }

        let feet = self
            .world_bounds_3d
            .clamp_sphere_center(self.play_character_feet_position(), 0.0);
        self.set_play_character_feet_position(feet);
    }
}
