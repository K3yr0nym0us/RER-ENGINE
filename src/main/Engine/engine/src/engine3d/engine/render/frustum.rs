//! Culling de AABB contra el frustum de la cámara.

use super::*;

pub(crate) fn is_aabb_visible_3d(view_proj: &glam::Mat4, center: GlamVec3, half: GlamVec3) -> bool {
    let m = view_proj.to_cols_array_2d();
    let r0 = [m[0][0], m[1][0], m[2][0], m[3][0]];
    let r1 = [m[0][1], m[1][1], m[2][1], m[3][1]];
    let r2 = [m[0][2], m[1][2], m[2][2], m[3][2]];
    let r3 = [m[0][3], m[1][3], m[2][3], m[3][3]];

    // Vulkan: z en [0, 1]. Los planos laterales usan r3 ± r0/r1 y el
    // near/far son r2 y r3 - r2 (no r3 ± r2, que es la convención OpenGL).
    let planes: [[f32; 4]; 6] = [
        [r3[0] + r0[0], r3[1] + r0[1], r3[2] + r0[2], r3[3] + r0[3]],
        [r3[0] - r0[0], r3[1] - r0[1], r3[2] - r0[2], r3[3] - r0[3]],
        [r3[0] + r1[0], r3[1] + r1[1], r3[2] + r1[2], r3[3] + r1[3]],
        [r3[0] - r1[0], r3[1] - r1[1], r3[2] - r1[2], r3[3] - r1[3]],
        [r2[0], r2[1], r2[2], r2[3]],
        [r3[0] - r2[0], r3[1] - r2[1], r3[2] - r2[2], r3[3] - r2[3]],
    ];

    for plane in planes {
        let d = plane[0] * center.x + plane[1] * center.y + plane[2] * center.z + plane[3];
        if d < -half.x * plane[0].abs() - half.y * plane[1].abs() - half.z * plane[2].abs() {
            return false;
        }
    }
    true
}

/// Infla el AABB de un caster a lo largo de la dirección de la luz.
///
/// Un objeto fuera del frustum de la cámara puede proyectar sombra dentro de la
/// vista si está «aguas arriba» del sol. Crecer el half-extent por
/// `margin_m * |light_dir|` es un test conservador: equivale a barrer el frustum
/// de cámara hacia la luz, así que cualquier caster cuyo rayo pueda caer dentro
/// pasa el `is_aabb_visible_3d` posterior. Con el sol alto el margen affecta
/// sobre todo al eje Y (las sombras caen cerca del caster); con el sol rasante,
/// a X/Z (sombras largas).
pub(crate) fn shadow_caster_aabb_half(
    half: GlamVec3,
    light_dir: GlamVec3,
    margin_m: f32,
) -> GlamVec3 {
    let dir = light_dir.normalize_or_zero();
    half + margin_m.max(0.0) * dir.abs()
}

/// Predicado de selección de casters: depende de LA CÁMARA (`view_proj`) y del sol.
///
/// Es la única decisión de "qué entra en el shadow map" que cambia con el punto de
/// vista: dos poses de cámara distintas sobre la misma escena producen conjuntos de
/// casters distintos. El forward usa `is_aabb_visible_3d` directo (frustum estricto).
pub(crate) fn is_shadow_caster_visible(
    view_proj: &glam::Mat4,
    center: GlamVec3,
    half: GlamVec3,
    light_dir: GlamVec3,
    margin_m: f32,
) -> bool {
    is_aabb_visible_3d(
        view_proj,
        center,
        shadow_caster_aabb_half(half, light_dir, margin_m),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caster_half_grows_only_along_light_direction() {
        let out = shadow_caster_aabb_half(GlamVec3::splat(1.0), GlamVec3::Y, 4.0);
        assert_eq!(out, GlamVec3::new(1.0, 5.0, 1.0));
    }

    #[test]
    fn caster_half_ignores_zero_and_negative_margin() {
        let half = GlamVec3::splat(1.0);
        assert_eq!(shadow_caster_aabb_half(half, GlamVec3::Y, 0.0), half);
        assert_eq!(shadow_caster_aabb_half(half, GlamVec3::Y, -3.0), half);
    }

    #[test]
    fn low_sun_expands_horizontal_extent() {
        let out =
            shadow_caster_aabb_half(GlamVec3::splat(1.0), GlamVec3::new(1.0, 0.001, 0.0), 10.0);
        assert!(out.x > 10.0, "{out:?}");
        assert!(out.y < 2.0, "{out:?}");
    }

    /// La selección de casters cambia con LA CÁMARA, no solo con el sol o la geometría.
    /// Misma escena y mismo sol; dos view_proj distintos dan conjuntos distintos.
    #[test]
    fn caster_selection_changes_with_the_camera() {
        let proj = glam::Mat4::perspective_rh(60f32.to_radians(), 1.0, 0.1, 100.0);
        let light_dir = GlamVec3::new(0.0, 0.3, 1.0).normalize();
        let center = GlamVec3::new(0.0, 0.0, -20.0);
        let half = GlamVec3::splat(0.5);
        let margin = 30.0;

        // Cámara A: mira al caster; entra en el conjunto de casters.
        let cam_looking_at_it =
            proj * glam::Mat4::look_at_rh(GlamVec3::ZERO, GlamVec3::NEG_Z, GlamVec3::Y);
        assert!(is_shadow_caster_visible(
            &cam_looking_at_it,
            center,
            half,
            light_dir,
            margin
        ));

        // Cámara B: mismo sol y mismo objeto, pero mirando hacia el lado contrario.
        // El caster queda fuera del frustum incluso expandido → sale del shadow map.
        let cam_looking_away = proj
            * glam::Mat4::look_at_rh(
                GlamVec3::new(0.0, 0.0, 200.0),
                GlamVec3::new(0.0, 0.0, 201.0),
                GlamVec3::Y,
            );
        assert!(!is_shadow_caster_visible(
            &cam_looking_away,
            center,
            half,
            light_dir,
            margin
        ));
    }

    /// El caster expandido entra en el frustum aunque su AABB original no.
    #[test]
    fn expanded_caster_becomes_visible_but_unexpanded_does_not() {
        let proj = glam::Mat4::perspective_rh(60f32.to_radians(), 1.0, 0.1, 100.0);
        let view = glam::Mat4::look_at_rh(GlamVec3::ZERO, GlamVec3::NEG_Z, GlamVec3::Y);
        let vp = proj * view;
        // Detrás de la cámara: el AABB estricto se culls.
        let center = GlamVec3::new(0.0, 0.0, 6.0);
        let half = GlamVec3::splat(0.5);
        assert!(!is_aabb_visible_3d(&vp, center, half));
        let expanded = shadow_caster_aabb_half(half, GlamVec3::NEG_Z, 12.0);
        assert!(is_aabb_visible_3d(&vp, center, expanded));
    }
}
