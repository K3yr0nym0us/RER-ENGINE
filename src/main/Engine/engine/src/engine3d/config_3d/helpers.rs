//! Utilidades de rutas de modelo y geometría de física/transform.

use super::*;

pub(crate) fn is_fbx_model_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("fbx"))
}

pub(crate) fn is_gltf_model_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("glb") || e.eq_ignore_ascii_case("gltf"))
}

/// Posición del cuerpo Rapier: pies en `transform.position` para FBX; centro en el resto.
pub(crate) fn physics_body_position_for_model_path(
    model_path: &str,
    transform_position: [f32; 3],
    half: [f32; 3],
) -> [f32; 3] {
    if is_fbx_model_path(model_path) {
        physics_3d::physics_center_from_feet_position(transform_position, half)
    } else {
        transform_position
    }
}

/// Centro del AABB local de la malla (espacio del mesh antes del transform de entidad).
pub(crate) fn physics_aabb_center_local(bounds: ([f32; 3], [f32; 3])) -> [f32; 3] {
    let (min, max) = bounds;
    [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ]
}

/// Posición del collider en mundo: centro del AABB escalado/rotado (GLB/GLTF);
/// FBX mantiene convención de pies en `transform.position`.
pub(crate) fn physics_body_world_center(
    transform: &Transform,
    local_bounds: Option<([f32; 3], [f32; 3])>,
    model_path: &str,
    half: [f32; 3],
) -> [f32; 3] {
    if is_fbx_model_path(model_path) {
        return physics_body_position_for_model_path(
            model_path,
            transform.position.to_array(),
            half,
        );
    }
    let Some(bounds) = local_bounds else {
        return transform.position.to_array();
    };
    let center_local = glam::Vec3::from_array(physics_aabb_center_local(bounds));
    let offset = transform.rotation * (transform.scale * center_local);
    (transform.position + offset).to_array()
}

/// Posición del `Transform` para que el centro visual del mesh quede en `desired_center`.
pub(crate) fn transform_position_for_visual_center(
    desired_center: glam::Vec3,
    rotation: glam::Quat,
    scale: glam::Vec3,
    model_path: &str,
    local_bounds: Option<([f32; 3], [f32; 3])>,
) -> glam::Vec3 {
    let probe = Transform {
        position: glam::Vec3::ZERO,
        rotation,
        scale,
    };
    let half = physics_half_extents_for_model(scale.abs().to_array(), local_bounds);
    let visual = glam::Vec3::from_array(physics_body_world_center(
        &probe,
        local_bounds,
        model_path,
        half,
    ));
    desired_center - visual
}

/// Aplica rotación alrededor de `group_pivot` manteniendo el centro visual de la malla
/// (mismo criterio que `entity_world_pick_aabb` / gizmo), no `Transform.position`.
pub(crate) fn rotate_entity_transform_about_visual_center(
    start_transform: &Transform,
    start_visual_center: glam::Vec3,
    group_pivot: glam::Vec3,
    delta_quat: glam::Quat,
    model_path: &str,
    local_bounds: Option<([f32; 3], [f32; 3])>,
) -> (glam::Vec3, glam::Quat) {
    let new_visual = group_pivot + delta_quat * (start_visual_center - group_pivot);
    let new_rot = (delta_quat * start_transform.rotation).normalize();
    let new_pos = transform_position_for_visual_center(
        new_visual,
        new_rot,
        start_transform.scale,
        model_path,
        local_bounds,
    );
    (new_pos, new_rot)
}

/// Semieje del collider: AABB local de la malla × escala del transform.
pub(crate) fn physics_half_extents_for_model(
    scale: [f32; 3],
    local_bounds: Option<([f32; 3], [f32; 3])>,
) -> [f32; 3] {
    if let Some((min, max)) = local_bounds {
        let sx = scale[0].abs();
        let sy = scale[1].abs();
        let sz = scale[2].abs();
        return [
            ((max[0] - min[0]).abs() * 0.5 * sx).max(0.01),
            ((max[1] - min[1]).abs() * 0.5 * sy).max(0.01),
            ((max[2] - min[2]).abs() * 0.5 * sz).max(0.01),
        ];
    }
    [
        (scale[0].abs() * 0.5).max(0.01),
        (scale[1].abs() * 0.5).max(0.01),
        (scale[2].abs() * 0.5).max(0.01),
    ]
}

/// Resuelve un cambio de eje individual sobre un vector actual.
/// Si `axis_value` es `Some`, reemplaza solo ese eje; en caso contrario aplica el `vec` completo.
pub(crate) fn resolve_axis_update(
    current: glam::Vec3,
    vec: Option<[f32; 3]>,
    axis_value: Option<AxisValue>,
) -> Option<glam::Vec3> {
    if let Some(av) = axis_value {
        let mut next = current;
        match av.axis {
            0 => next.x = av.value,
            1 => next.y = av.value,
            2 => next.z = av.value,
            _ => {}
        }
        return Some(next);
    }
    vec.map(glam::Vec3::from_array)
}

/// Resuelve rotación de `set_transform`: quaternion explícito, delta Euler o Euler absoluto.
///
/// El front trabaja con índices semánticos: `axis 0 = pitch (X)`, `axis 1 = yaw (Y)`, `axis 2 = roll (Z)`.
///
/// **Deltas (`rotation_euler_delta`)**: se aplican como giro local en quaternion (`current * axis_quat`),
/// idéntico para X/Y/Z y al `quatRotateLocalAxis` del front. NO usar `to_euler` + suma + `from_euler`:
/// en YXZ el eje central (X/pitch) queda acotado a ~±90° al descomponer y el slider “se congela”
/// mientras Y/Z siguen circulares.
///
/// **Euler absoluto (`rotation_euler_degrees`)**: `from_euler(YXZ, yaw, pitch, roll)`.
pub(crate) fn resolve_set_transform_rotation(
    current: glam::Quat,
    rotation: Option<[f32; 4]>,
    euler_delta: Option<RotationEulerDelta>,
    euler_degrees: Option<[f32; 3]>,
) -> Option<glam::Quat> {
    if let Some(d) = euler_delta {
        let delta = d.degrees.to_radians();
        let axis_quat = match d.axis {
            0 => glam::Quat::from_rotation_x(delta),
            1 => glam::Quat::from_rotation_y(delta),
            2 => glam::Quat::from_rotation_z(delta),
            _ => return Some(current),
        };
        return Some((current * axis_quat).normalize());
    }
    if let Some([pitch_deg, yaw_deg, roll_deg]) = euler_degrees {
        return Some(glam::Quat::from_euler(
            glam::EulerRot::YXZ,
            yaw_deg.to_radians(),   // Y
            pitch_deg.to_radians(), // X
            roll_deg.to_radians(),  // Z
        ));
    }
    rotation.map(|r| glam::Quat::from_xyzw(r[0], r[1], r[2], r[3]))
}
