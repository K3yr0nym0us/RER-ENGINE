//! Contrato espacial 2D del motor (referencia para input, picking y simulación).
//!
//! - **Mundo XY**: ejes de [`crate::ecs::Transform`] (X derecha, Y arriba).
//! - **Body / física Rapier**: `Transform.position` (pivot); no se desplaza con `visual_offsets`.
//! - **Centro visual**: `Transform.position + visual_offsets` — render, picking, hover, triggers y spatial grid.
//! - **Pantalla**: píxeles del viewport winit → [`screen_pixel_to_world_xy`].
//! - **Frame UV**: solo edición de pivot (`handle_pivot_click_2d`).

use glam::{Vec2, Vec3};

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

/// Rayo en XY contra AABB alineado a ejes. `dir` debe estar normalizado.
/// Devuelve `(toi, normal)` con la normal de la cara impactada.
pub(crate) fn ray_aabb_intersect_xy(
    origin: Vec2,
    dir: Vec2,
    max_dist: f32,
    center: Vec2,
    half: Vec2,
) -> Option<(f32, Vec2)> {
    if max_dist <= 1e-6 {
        return None;
    }
    let min = center - half;
    let max = center + half;
    let mut tmin = 0.0_f32;
    let mut tmax = max_dist;
    let mut normal = Vec2::Y;

    for axis in 0..2 {
        let (o, d, mn, mx) = match axis {
            0 => (origin.x, dir.x, min.x, max.x),
            _ => (origin.y, dir.y, min.y, max.y),
        };
        if d.abs() < 1e-8 {
            if o < mn || o > mx {
                return None;
            }
            continue;
        }
        let inv = 1.0 / d;
        let mut t1 = (mn - o) * inv;
        let mut t2 = (mx - o) * inv;
        let n = if axis == 0 {
            Vec2::new(if inv < 0.0 { 1.0 } else { -1.0 }, 0.0)
        } else {
            Vec2::new(0.0, if inv < 0.0 { 1.0 } else { -1.0 })
        };
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }
        if t1 > tmin {
            tmin = t1;
            normal = n;
        }
        tmax = tmax.min(t2);
        if tmax < tmin {
            return None;
        }
    }

    if tmax < 0.0 {
        return None;
    }
    let toi = if tmin >= 0.0 { tmin } else { 0.0 };
    if toi > max_dist {
        return None;
    }
    Some((toi, normal))
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

    #[test]
    fn ray_hits_aabb_from_left() {
        let hit = ray_aabb_intersect_xy(
            Vec2::new(-5.0, 0.0),
            Vec2::X,
            10.0,
            Vec2::ZERO,
            Vec2::new(1.0, 1.0),
        );
        let (toi, normal) = hit.expect("rayo debe impactar caja");
        assert!((toi - 4.0).abs() < 1e-4);
        assert!((normal.x + 1.0).abs() < 1e-4);
    }

    #[test]
    fn ray_misses_aabb_behind() {
        assert!(
            ray_aabb_intersect_xy(
                Vec2::new(5.0, 0.0),
                Vec2::X,
                10.0,
                Vec2::ZERO,
                Vec2::new(1.0, 1.0),
            )
            .is_none()
        );
    }
}
