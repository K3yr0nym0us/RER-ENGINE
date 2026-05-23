use glam::Vec3;

use crate::config_3d::character_anchor::{
    PLAY_CHARACTER_EDITOR_ORBIT_DISTANCE, PLAY_CHARACTER_EYE_OFFSET, PLAY_CHARACTER_MOUSE_SPEED,
};
use crate::ecs::Transform;
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};

/// Forward de la cámara proyectado al plano XZ. Coincide con `Camera::view_forward` con pitch=0.
fn look_xz_from_camera_yaw(camera_yaw: f32) -> glam::Vec2 {
    let (sy, cy) = camera_yaw.sin_cos();
    glam::Vec2::new(-cy, -sy)
}

fn camera_yaw_from_look_xz(look: glam::Vec2) -> f32 {
    (-look.y).atan2(-look.x)
}

/// Forward world del mesh tras rotar `mesh_yaw` alrededor de Y.
fn look_xz_from_mesh_yaw(mesh_yaw: f32, mesh_forward_xz: glam::Vec2) -> glam::Vec2 {
    let (s, c) = mesh_yaw.sin_cos();
    let fx = mesh_forward_xz.x;
    let fz = mesh_forward_xz.y;
    glam::Vec2::new(fx * c + fz * s, -fx * s + fz * c)
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
        self.ensure_editor_camera_entity();
        self.sync_editor_camera_entity_from_viewport();
    }

    /// Punto al que mira la cámara orbital del viewport.
    pub(crate) fn orbit_view_anchor(&self) -> Vec3 {
        if self.uses_editor_viewport_camera() {
            if let Some(id) = self.editor_camera_entity {
                if let Some(t) = self.world.get::<Transform>(id) {
                    return t.position;
                }
            }
            self.editor_orbit_target
        } else {
            self.camera.target
        }
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

    pub(crate) fn camera_view_matrix(&self) -> glam::Mat4 {
        let anchor = self.orbit_view_anchor();
        let (yaw, pitch, dist) = self.viewport_orbit_angles();
        self.camera.view_matrix_at_angles(anchor, yaw, pitch, dist)
    }

    pub(crate) fn camera_world_position(&self) -> Vec3 {
        let anchor = self.orbit_view_anchor();
        let (yaw, pitch, dist) = self.viewport_orbit_angles();
        self.camera.position_at_angles(anchor, yaw, pitch, dist)
    }

    pub(crate) fn camera_to_uniform_at_anchor(&self, anchor: Vec3, aspect: f32) -> crate::config_3d::camera_3d::CameraUniform {
        let (yaw, pitch, dist) = self.viewport_orbit_angles();
        let view = self.camera.view_matrix_at_angles(anchor, yaw, pitch, dist);
        crate::config_3d::camera_3d::CameraUniform {
            view_proj: (self.camera.proj_matrix(aspect) * view).to_cols_array_2d(),
        }
    }

    /// Inverso de `sync_player_rotation_from_look`: toma el yaw del cuerpo y lo asigna a la
    /// cámara. Se invoca al entrar a Play para que `apply_fps_mouse_look` no haga
    /// "snap" del cuerpo al primer movimiento de mouse.
    pub(crate) fn sync_camera_yaw_from_player_body(&mut self) {
        let Some(id) = self.play_character_entity else {
            return;
        };
        let Some(t) = self.world.get::<Transform>(id) else {
            return;
        };
        let (mesh_yaw, _, _) = t.rotation.to_euler(glam::EulerRot::YXZ);
        let look = look_xz_from_mesh_yaw(mesh_yaw, self.play_character_mesh_forward_xz);
        self.camera.yaw = camera_yaw_from_look_xz(look);
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

    /// Centra la órbita del editor en la selección o en el jugador FP.
    pub(crate) fn sync_editor_camera_focus(&mut self) {
        if self.camera_2d.is_some() || self.is_play_controller_active() {
            return;
        }

        if self.editor_orbit_decoupled_from_player() {
            self.camera.orbit_pivot_offset = Vec3::ZERO;
            return;
        }

        if let Some(center) = self.selection_center() {
            let focus_player = self
                .selected_entity
                .and_then(|id| self.play_character_entity.map(|p| p == id))
                .unwrap_or(false);

            if !focus_player {
                self.editor_orbit_target = center;
            }
            self.camera.orbit_pivot_offset = Vec3::ZERO;
        } else {
            self.camera.orbit_pivot_offset = Vec3::ZERO;
        }
    }

    pub(crate) fn sync_fps_camera_mode(&mut self) {
        if self.is_play_controller_active() {
            self.camera.orbit_pivot_offset = Vec3::ZERO;
            self.camera.eye_height_offset = 0.0;
            self.camera.eye_offset_local = self.play_character_eye_world_offset();
            self.camera.distance = 0.01;
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
            self.editor_orbit_target = Vec3::from_array(position);
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

    /// Aplica cambios parciales de la cámara FPS en editor (sin tocar al Player).
    pub(crate) fn apply_play_camera_view_patch(
        &mut self,
        position_axis: Option<crate::ipc::AxisValue>,
        yaw: Option<f32>,
        pitch: Option<f32>,
        fov_y: Option<f32>,
        frustum_distance: Option<f32>,
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

        self.sync_fps_camera_mode();
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
        if self.is_play_controller_active() {
            self.camera.distance = 0.01;
            self.sync_fps_camera_mode();
        }
    }
}
