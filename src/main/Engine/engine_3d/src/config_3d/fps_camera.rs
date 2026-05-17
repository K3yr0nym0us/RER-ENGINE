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
    pub(crate) fn play_character_camera_gizmo_pose(&self) -> Option<(Vec3, f32, f32)> {
        let id = self.play_character_entity?;
        let t = self.world.get::<Transform>(id)?;
        let (mesh_yaw, _, _) = t.rotation.to_euler(glam::EulerRot::YXZ);
        let look = look_xz_from_mesh_yaw(mesh_yaw, self.play_character_mesh_forward_xz);
        let eye = self.play_character_feet_position() + self.play_character_eye_world_offset();
        Some((eye, camera_yaw_from_look_xz(look), 0.0))
    }

    /// Eye offset SIEMPRE en world Y (estilo Godot Camera3D anclado al pivote del jugador).
    pub(crate) fn play_character_eye_world_offset(&self) -> Vec3 {
        Vec3::new(0.0, PLAY_CHARACTER_EYE_OFFSET, 0.0)
    }

    pub(crate) fn sync_fps_camera_mode(&mut self) {
        if self.is_play_controller_active() {
            self.camera.orbit_pivot_offset = Vec3::ZERO;
            self.camera.eye_height_offset = 0.0;
            self.camera.eye_offset_local = self.play_character_eye_world_offset();
            self.camera.distance = 0.01;
        } else if self.has_play_character() {
            self.camera.orbit_pivot_offset =
                Vec3::new(0.0, PLAY_CHARACTER_EYE_OFFSET, 0.0);
            self.camera.eye_height_offset = 0.0;
            self.camera.eye_offset_local = Vec3::ZERO;
            self.camera.distance = PLAY_CHARACTER_EDITOR_ORBIT_DISTANCE;
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
    ) {
        if self.camera_2d.is_some() {
            return;
        }

        self.set_play_character_feet_position(Vec3::from_array(position));
        self.camera.yaw = yaw;
        self.camera.pitch = pitch.clamp(
            -std::f32::consts::FRAC_PI_2 + 0.05,
            std::f32::consts::FRAC_PI_2 - 0.05,
        );
        self.sync_player_rotation_from_look();
        self.sync_fps_camera_mode();

        self.play_controller_velocity = Vec3::ZERO;
        self.play_controller_on_floor = false;
        self.clamp_play_character_camera_to_bounds();
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
        self.apply_play_character_saved_view(position, yaw, pitch);
        if let Some(fov) = fov_y {
            self.camera.fov_y = fov.clamp(0.1, std::f32::consts::FRAC_PI_2 - 0.01);
        }
        if let Some(dist) = frustum_distance {
            self.fps_editor_frustum_distance = dist.clamp(0.5, 50.0);
        }
        self.emit_play_character_view_changed();
    }

    /// Emite la vista actual para que el frontend no derive poses en TypeScript.
    pub(crate) fn emit_play_character_view_changed(&self) {
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
        send_event(&EngineEvent::PlayCharacterViewChanged {
            player_id,
            position: feet.to_array(),
            yaw: self.camera.yaw,
            pitch: self.camera.pitch,
            fov_y: self.camera.fov_y,
            frustum_distance: self.fps_editor_frustum_distance,
            body_center,
            body_rotation,
            body_scale,
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
