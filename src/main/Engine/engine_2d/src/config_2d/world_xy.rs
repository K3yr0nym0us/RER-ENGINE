//! Contrato espacial 2D del motor (referencia para input, picking y simulación).
//!
//! - **Mundo XY**: ejes de [`crate::ecs::Transform`] (X derecha, Y arriba).
//! - **Body / física Rapier**: `Transform.position` (pivot); no se desplaza con `visual_offsets`.
//! - **Centro visual**: `Transform.position + visual_offsets` — render, picking, hover, triggers y spatial grid.
//! - **Pantalla**: píxeles del viewport winit → [`screen_pixel_to_world_xy`].
//! - **Frame UV**: solo edición de pivot (`handle_pivot_click_2d`).

use glam::Vec3;

use super::Camera2D;

/// Pantalla (píxeles) → mundo XY con la cámara ortográfica 2D activa.
#[inline]
pub(crate) fn screen_pixel_to_world_xy(
    cam: &Camera2D,
    viewport_width: f32,
    viewport_height: f32,
    pixel_x: f32,
    pixel_y: f32,
) -> (f32, f32) {
    let aspect = viewport_width / viewport_height;
    let half_w = cam.half_h * aspect;
    let wx = cam.x + ((pixel_x / viewport_width) * 2.0 - 1.0) * half_w;
    let wy = cam.y + (1.0 - (pixel_y / viewport_height) * 2.0) * cam.half_h;
    (wx, wy)
}

/// Centro XY usado para dibujar el sprite y para hit-tests de editor/gameplay.
#[inline]
pub(crate) fn transform_visual_center(position: Vec3, visual_offset: Option<Vec3>) -> Vec3 {
    position + visual_offset.unwrap_or(Vec3::ZERO)
}

/// AABB alineado a ejes en el plano XY (centro + semitamaños).
#[inline]
pub(crate) fn aabb_contains_point_xy(
    center_x: f32,
    center_y: f32,
    half_ext_x: f32,
    half_ext_y: f32,
    wx: f32,
    wy: f32,
) -> bool {
    wx >= center_x - half_ext_x
        && wx <= center_x + half_ext_x
        && wy >= center_y - half_ext_y
        && wy <= center_y + half_ext_y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_center_applies_offset() {
        let p = Vec3::new(10.0, 5.0, 0.0);
        let vo = Vec3::new(1.0, -2.0, 0.0);
        let c = transform_visual_center(p, Some(vo));
        assert_eq!(c.x, 11.0);
        assert_eq!(c.y, 3.0);
    }

    #[test]
    fn aabb_hit_uses_center() {
        assert!(aabb_contains_point_xy(0.0, 0.0, 2.0, 1.0, 1.5, 0.0));
        assert!(!aabb_contains_point_xy(0.0, 0.0, 1.0, 1.0, 3.0, 0.0));
    }
}
