//! Conversión píxeles ↔ NDC y transform de quads HUD anclados arriba-izquierda.

use glam::Mat4;

/// Convierte coordenadas de píxel (origen arriba-izquierda) a NDC.
pub fn pixel_to_ndc(viewport_w: f32, viewport_h: f32, px: f32, py: f32) -> [f32; 2] {
    let w = viewport_w.max(1.0);
    let h = viewport_h.max(1.0);
    [(px / w) * 2.0 - 1.0, 1.0 - (py / h) * 2.0]
}

/// Matriz NDC para un quad con esquina superior-izquierda en píxeles de viewport.
///
/// `flip_bitmap_y`: `true` para bitmaps rasterizados (texto HUD, motor 3D y archivos PNG);
/// `false` para texturas PNG/JPEG en el motor 2D (sin flip V extra en el modelo).
pub fn ndc_transform_top_left(
    px: f32,
    py: f32,
    w_px: f32,
    h_px: f32,
    viewport_w: f32,
    viewport_h: f32,
    flip_bitmap_y: bool,
) -> Option<Mat4> {
    if w_px <= 0.0 || h_px <= 0.0 || viewport_w <= 0.0 || viewport_h <= 0.0 {
        return None;
    }
    let ndc_w = 2.0 * w_px / viewport_w;
    let ndc_h = 2.0 * h_px / viewport_h;
    let cx = -1.0 + (px / viewport_w) * 2.0 + ndc_w * 0.5;
    let cy = 1.0 - (py / viewport_h) * 2.0 - ndc_h * 0.5;
    let scale_y = if flip_bitmap_y { -ndc_h } else { ndc_h };
    Some(
        Mat4::from_translation(glam::vec3(cx, cy, 0.0))
            * Mat4::from_scale(glam::vec3(ndc_w, scale_y, 1.0)),
    )
}
