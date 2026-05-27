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

/// AABB local de la malla del jugador (tras normalizar); alimenta cápsula de movimiento.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PlayCharacterMeshExtents {
    pub local_min_y: f32,
    pub local_max_y: f32,
    pub radius_xz: f32,
}

impl PlayCharacterMeshExtents {
    pub(crate) fn from_local_bounds(min: [f32; 3], max: [f32; 3]) -> Self {
        let radius_xz = (max[0] - min[0]).max(max[2] - min[2]) * 0.5;
        Self {
            local_min_y: min[1],
            local_max_y: max[1],
            radius_xz: radius_xz.max(0.15),
        }
    }

    pub(crate) fn height(&self) -> f32 {
        (self.local_max_y - self.local_min_y).max(0.01)
    }
}
/// Distancia de la cámara orbital en editor (detrás del jugador, fuera de play).
pub(crate) const PLAY_CHARACTER_EDITOR_ORBIT_DISTANCE: f32 = 5.0;
/// Pitch orbital del editor 3D (rad). Eleva la cámara; no confundir con mirar al suelo (eso depende del pivote).
pub(crate) const PLAY_CHARACTER_EDITOR_ORBIT_PITCH: f32 = 0.48;
/// Yaw orbital del editor 3D (rad) en escenas FP.
pub(crate) const PLAY_CHARACTER_EDITOR_ORBIT_YAW: f32 = -std::f32::consts::FRAC_PI_2;
/// Pitch por defecto al arrancar el motor (antes de cargar escena FP).
pub(crate) const EDITOR_DEFAULT_ORBIT_PITCH: f32 = 0.48;

impl State {
    pub(crate) fn play_character_body_height_world(&self, scale_y: f32) -> f32 {
        if self.play_character_mesh_extents.is_some() {
            self.play_character_mesh_extents
                .as_ref()
                .map(|e| e.height())
                .unwrap_or(PLAY_CHARACTER_BODY_HEIGHT)
        } else {
            PLAY_CHARACTER_BODY_HEIGHT * scale_y
        }
    }

    pub(crate) fn play_character_capsule_radius_world(&self, _scale: glam::Vec3) -> f32 {
        if let Some(e) = &self.play_character_mesh_extents {
            e.radius_xz.max(PLAY_CHARACTER_COLLIDER_RADIUS)
        } else {
            PLAY_CHARACTER_COLLIDER_RADIUS
        }
    }

    /// Pies del controller en play: eje mundo Y (independiente de rotación del mesh).
    pub(crate) fn controller_feet_from_center(&self, center: Vec3) -> Vec3 {
        let half_h = self.play_character_body_height_world(1.0) * 0.5;
        Vec3::new(center.x, center.y - half_h, center.z)
    }

    pub(crate) fn center_from_controller_feet(&self, feet: Vec3) -> Vec3 {
        let half_h = self.play_character_body_height_world(1.0) * 0.5;
        Vec3::new(feet.x, feet.y + half_h, feet.z)
    }
}

pub(crate) fn feet_offset_local(
    extents: Option<&PlayCharacterMeshExtents>,
    scale_y: f32,
    rotation: glam::Quat,
) -> Vec3 {
    if let Some(e) = extents {
        rotation * Vec3::new(0.0, e.local_min_y * scale_y, 0.0)
    } else {
        rotation * Vec3::new(0.0, -PLAY_CHARACTER_BODY_HEIGHT * 0.5, 0.0)
    }
}

pub(crate) fn feet_from_transform(
    center: Vec3,
    scale_y: f32,
    rotation: glam::Quat,
    extents: Option<&PlayCharacterMeshExtents>,
) -> Vec3 {
    center + feet_offset_local(extents, scale_y, rotation)
}

pub(crate) fn center_from_feet(
    feet: Vec3,
    scale_y: f32,
    rotation: glam::Quat,
    extents: Option<&PlayCharacterMeshExtents>,
) -> Vec3 {
    feet - feet_offset_local(extents, scale_y, rotation)
}

pub(crate) fn body_center_from_feet(feet: Vec3) -> Vec3 {
    center_from_feet(feet, PLAY_CHARACTER_BODY_HEIGHT, glam::Quat::IDENTITY, None)
}
