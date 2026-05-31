use glam::{Quat, Vec3};

use crate::ecs::Transform;
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
/// Radio de la píldora ≈ mitad del ancho del mesh × factor (margen animaciones).
pub(crate) const PLAY_CHARACTER_COLLISION_RADIUS_FACTOR: f32 = 0.54;
pub(crate) const PLAY_CHARACTER_COLLISION_RADIUS_MAX: f32 = 0.50;
pub(crate) const PLAY_CHARACTER_COLLISION_RADIUS_MIN: f32 = 0.30;
/// Altura de la píldora de colisión respecto al cuerpo normalizado (1.7 m).
pub(crate) const PLAY_CHARACTER_CAPSULE_HEIGHT_SCALE: f32 = 0.9;

/// Cápsula de movimiento del jugador (primitiva fija; no usa la malla visual).
#[derive(Clone, Copy, Debug)]
pub(crate) struct PlayCharacterCollisionCapsule {
    pub radius: f32,
    pub height: f32,
}

impl PlayCharacterCollisionCapsule {
    pub(crate) fn standard() -> Self {
        Self {
            radius: PLAY_CHARACTER_COLLIDER_RADIUS,
            height: PLAY_CHARACTER_BODY_HEIGHT,
        }
    }
}

/// AABB local de la malla del jugador (pies = `local_min[1]`, techo = `local_max[1]`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct PlayCharacterMeshExtents {
    pub local_min: [f32; 3],
    pub local_max: [f32; 3],
}

/// Tolerancia para considerar que el origen local de la malla está en los pies (caché `::play_character`).
pub(crate) const PLAY_CHARACTER_MESH_FEET_ORIGIN_EPS: f32 = 0.05;

impl PlayCharacterMeshExtents {
    pub(crate) fn from_local_bounds(min: [f32; 3], max: [f32; 3]) -> Self {
        Self {
            local_min: min,
            local_max: max,
        }
    }

    /// Punto más bajo de la malla en espacio local de entidad (suelo del barril).
    pub(crate) fn local_feet(&self) -> Vec3 {
        Vec3::new(
            (self.local_min[0] + self.local_max[0]) * 0.5,
            self.local_min[1],
            (self.local_min[2] + self.local_max[2]) * 0.5,
        )
    }

    pub(crate) fn feet_world_offset(&self, scale: Vec3, rotation: Quat) -> Vec3 {
        let f = self.local_feet();
        rotation * Vec3::new(f.x * scale.x, f.y * scale.y, f.z * scale.z)
    }

    /// `true` si `Transform.position` es directamente los pies (FBX / preview con pies en Y≈0).
    pub(crate) fn origin_at_feet(&self) -> bool {
        self.local_min[1].abs() < PLAY_CHARACTER_MESH_FEET_ORIGIN_EPS
    }

    /// Altura del barril (máximo − mínimo en Y local).
    pub(crate) fn height(&self) -> f32 {
        (self.local_max[1] - self.local_min[1]).max(0.01)
    }

    /// Anchura en planta (eje X o Z, el mayor).
    pub(crate) fn horizontal_extent(&self) -> f32 {
        let dx = (self.local_max[0] - self.local_min[0]).abs();
        let dz = (self.local_max[2] - self.local_min[2]).abs();
        dx.max(dz)
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
    /// Altura en mundo del mesh del jugador (AABB local × scale.y); placeholder = 1.7 m.
    pub(crate) fn play_character_visual_world_height(&self) -> f32 {
        let scale_y = self
            .play_character_entity
            .and_then(|id| self.world.get::<Transform>(id))
            .map(|t| t.scale.y.abs())
            .unwrap_or(1.0);
        self.play_character_mesh_extents
            .map(|e| e.height() * scale_y)
            .unwrap_or(PLAY_CHARACTER_BODY_HEIGHT)
    }

    /// Cápsula del jugador: altura normalizada 1.7 m; radio desde ancho del mesh (píldora uniforme).
    pub(crate) fn play_character_collision_capsule(&self) -> PlayCharacterCollisionCapsule {
        let mut cap = PlayCharacterCollisionCapsule::standard();
        if self.play_character_uses_mesh_driven_capsule() {
            let ext = self.play_character_mesh_extents.unwrap();
            let t = self
                .play_character_entity
                .and_then(|id| self.world.get::<Transform>(id));
            let scale_xz = t
                .map(|t| t.scale.x.abs().max(t.scale.z.abs()))
                .unwrap_or(1.0);
            cap.height = self.play_character_visual_world_height();
            let max_r = (cap.height * 0.5 - 0.05).max(PLAY_CHARACTER_COLLISION_RADIUS_MIN);
            cap.radius = (ext.horizontal_extent() * scale_xz * 0.5
                * PLAY_CHARACTER_COLLISION_RADIUS_FACTOR)
                .clamp(
                    PLAY_CHARACTER_COLLISION_RADIUS_MIN,
                    max_r.min(PLAY_CHARACTER_COLLISION_RADIUS_MAX),
                );
        }
        cap.height *= PLAY_CHARACTER_CAPSULE_HEIGHT_SCALE;
        cap
    }

    pub(crate) fn play_character_body_height_world(&self, scale_y: f32) -> f32 {
        if self.play_character_entity.is_some() {
            self.play_character_collision_capsule().height
        } else {
            PLAY_CHARACTER_BODY_HEIGHT * scale_y
        }
    }

    pub(crate) fn play_character_capsule_radius_world(&self, _scale: glam::Vec3) -> f32 {
        if self.play_character_entity.is_some() {
            self.play_character_collision_capsule().radius
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

pub(crate) fn feet_from_transform(
    center: Vec3,
    scale: Vec3,
    rotation: Quat,
    extents: Option<&PlayCharacterMeshExtents>,
) -> Vec3 {
    center
        + extents
            .map(|e| e.feet_world_offset(scale, rotation))
            .unwrap_or_else(|| play_character_placeholder_feet_offset(rotation))
}

pub(crate) fn center_from_feet(
    feet: Vec3,
    scale: Vec3,
    rotation: Quat,
    extents: Option<&PlayCharacterMeshExtents>,
) -> Vec3 {
    feet
        - extents
            .map(|e| e.feet_world_offset(scale, rotation))
            .unwrap_or_else(|| play_character_placeholder_feet_offset(rotation))
}

/// Cubo `[Player]` antes de asignar un `.glb`: centro del cuerpo a mitad de altura fija (1.7 m).
pub(crate) fn play_character_placeholder_feet_offset(rotation: Quat) -> Vec3 {
    rotation * Vec3::new(0.0, -PLAY_CHARACTER_BODY_HEIGHT * 0.5, 0.0)
}

pub(crate) fn body_center_from_feet(feet: Vec3) -> Vec3 {
    center_from_feet(feet, Vec3::ONE, Quat::IDENTITY, None)
}
