//! Contrato espacial 2D del motor (referencia para input, picking y simulación).
//!
//! - **Mundo XY**: ejes de [`crate::ecs::Transform`] (X derecha, Y arriba). Rapier, triggers,
//!   colliders dibujados y picking del editor usan AABB en este espacio.
//! - **Pantalla**: píxeles del viewport winit. Convertir con [`screen_pixel_to_world_xy`].
//! - **Frame UV**: solo edición de pivot (`handle_pivot_click_2d`); no es espacio de juego.
//! - **`visual_offsets`**: desplazamiento solo en render; picking/triggers siguen `Transform`
//!   hasta una migración explícita de comportamiento.

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
