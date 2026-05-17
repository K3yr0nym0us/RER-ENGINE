use glam::Vec3;

use crate::engine::State;

pub(crate) const PLAY_CHARACTER_KEYBOARD_SPEED: f32 = 4.0;
pub(crate) const PLAY_CHARACTER_SPRINT_MULTIPLIER: f32 = 3.0;
pub(crate) const PLAY_CHARACTER_MOUSE_SPEED: f32 = 0.0020;
pub(crate) const PLAY_CHARACTER_COLLIDER_RADIUS: f32 = 0.40;
pub(crate) const PLAY_CHARACTER_EYE_OFFSET: f32 = 1.35;
pub(crate) const PLAY_CHARACTER_JUMP_SPEED: f32 = 6.0;
pub(crate) const PLAY_CHARACTER_GROUND_PROBE: f32 = 0.08;
/// Altura del cubo-cuerpo del jugador (placeholder visual).
pub(crate) const PLAY_CHARACTER_BODY_HEIGHT: f32 = 1.7;
/// Distancia de la cámara orbital en editor (detrás del jugador, fuera de play).
pub(crate) const PLAY_CHARACTER_EDITOR_ORBIT_DISTANCE: f32 = 3.0;

impl State {
    /// Offset pivot→pies. La malla del jugador SIEMPRE mide `PLAY_CHARACTER_BODY_HEIGHT` (1.7m):
    /// el cubo placeholder lo logra vía `scale.y = 1.7`, los modelos importados se normalizan
    /// a 1.7m con `scale.y = 1.0`. Por eso el offset es constante (no depende de `scale_y`).
    pub(crate) fn feet_offset_local(_scale_y: f32, rotation: glam::Quat) -> Vec3 {
        rotation * Vec3::new(0.0, -PLAY_CHARACTER_BODY_HEIGHT * 0.5, 0.0)
    }

    /// Pies del controller en play: eje mundo Y (independiente de rotación del mesh).
    pub(crate) fn controller_feet_from_center(center: Vec3) -> Vec3 {
        Vec3::new(
            center.x,
            center.y - PLAY_CHARACTER_BODY_HEIGHT * 0.5,
            center.z,
        )
    }

    pub(crate) fn center_from_controller_feet(feet: Vec3) -> Vec3 {
        Vec3::new(
            feet.x,
            feet.y + PLAY_CHARACTER_BODY_HEIGHT * 0.5,
            feet.z,
        )
    }
}

pub(crate) fn feet_from_transform(center: Vec3, scale_y: f32, rotation: glam::Quat) -> Vec3 {
    center + State::feet_offset_local(scale_y, rotation)
}

pub(crate) fn center_from_feet(feet: Vec3, scale_y: f32, rotation: glam::Quat) -> Vec3 {
    feet - State::feet_offset_local(scale_y, rotation)
}

pub(crate) fn body_center_from_feet(feet: Vec3) -> Vec3 {
    center_from_feet(feet, PLAY_CHARACTER_BODY_HEIGHT, glam::Quat::IDENTITY)
}
