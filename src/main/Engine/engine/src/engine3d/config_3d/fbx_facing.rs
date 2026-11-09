//! Orientación del jugador FP desde metadata FBX (`SceneSettings.axes.front`).

use glam::{Vec2, Vec3};

/// Forward en plano XZ desde `settings.axes.front` (ufbx, espacio Y-up del motor).
pub fn forward_xz_from_ufbx_front(front: ufbx::CoordinateAxis) -> Vec2 {
    let dir = match front {
        ufbx::CoordinateAxis::PositiveX => Vec3::X,
        ufbx::CoordinateAxis::NegativeX => -Vec3::X,
        ufbx::CoordinateAxis::PositiveY => Vec3::Y,
        ufbx::CoordinateAxis::NegativeY => -Vec3::Y,
        ufbx::CoordinateAxis::PositiveZ => Vec3::Z,
        ufbx::CoordinateAxis::NegativeZ => -Vec3::Z,
        ufbx::CoordinateAxis::Unknown => Vec3::Z,
    };
    let xz = Vec2::new(dir.x, dir.z);
    if xz.length_squared() < 1e-8 {
        Vec2::new(0.0, 1.0)
    } else {
        xz.normalize()
    }
}

/// Combina metadata FBX con estimación geométrica (solo `.fbx`).
pub fn resolve_fbx_forward_xz(meta: Vec2, geometry_est: Vec2) -> Vec2 {
    let dot = meta.dot(geometry_est);
    if dot < -0.5 {
        return geometry_est;
    }
    if dot < 0.35 {
        return geometry_est;
    }
    meta
}
