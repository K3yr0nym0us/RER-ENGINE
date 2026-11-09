//! Empaquetado y colocación de overlays HUD en espacio de pantalla.
//!
//! Todo lo de este módulo es puro: el empaquetado shelf y la creación de la textura GPU viven en
//! cada motor (el 2D usa su atlas Vulkan; el 3D tiene el suyo).

use glam::Mat4;

/// Subrectángulo UV y tamaño en píxeles del PNG original (sin estirar al atlas).
#[derive(Debug, Clone, Copy)]
pub struct ScreenHudPackedImage {
    pub uv_rect: [f32; 4],
    pub pixel_width: f32,
    pub pixel_height: f32,
}

/// Posición del overlay en la esquina inferior izquierda (NDC).
#[derive(Debug, Clone, Copy)]
pub struct ScreenHudBottomLeftLayout {
    pub margin_px: f32,
    pub max_width_fraction: f32,
    pub max_width_px_min: f32,
    pub max_width_px_max: f32,
    pub display_scale: f32,
}

impl Default for ScreenHudBottomLeftLayout {
    fn default() -> Self {
        Self {
            margin_px: 18.0,
            max_width_fraction: 0.28,
            max_width_px_min: 120.0,
            max_width_px_max: 360.0,
            display_scale: 0.9,
        }
    }
}

/// Matriz NDC para un quad anclado abajo-izquierda con animación de entrada suave.
pub fn ndc_transform_bottom_left(
    viewport_w: f32,
    viewport_h: f32,
    packed: ScreenHudPackedImage,
    layout: ScreenHudBottomLeftLayout,
    eased_alpha: f32,
) -> Option<Mat4> {
    if viewport_w <= 0.0 || viewport_h <= 0.0 {
        return None;
    }
    let img_w = packed.pixel_width;
    let img_h = packed.pixel_height;
    if img_w <= 0.0 || img_h <= 0.0 {
        return None;
    }

    let max_width_px = (viewport_w * layout.max_width_fraction)
        .clamp(layout.max_width_px_min, layout.max_width_px_max);
    let scale_px = (max_width_px / img_w).min(1.0);
    let draw_w_px = img_w * scale_px * layout.display_scale;
    let draw_h_px = img_h * scale_px * layout.display_scale;

    let scale_in = 0.92 + 0.08 * eased_alpha;
    let slide_px = (1.0 - eased_alpha) * 14.0;

    let ndc_w = 2.0 * (draw_w_px / viewport_w) * scale_in;
    let ndc_h = 2.0 * (draw_h_px / viewport_h) * scale_in;
    let margin_x_ndc = 2.0 * layout.margin_px / viewport_w;
    let margin_y_ndc = 2.0 * layout.margin_px / viewport_h;
    let slide_ndc = 2.0 * slide_px / viewport_h;
    let cx = -1.0 + margin_x_ndc + ndc_w * 0.5;
    let cy = -1.0 + margin_y_ndc + ndc_h * 0.5 + slide_ndc;

    Some(
        Mat4::from_translation(glam::vec3(cx, cy, 0.0))
            * Mat4::from_scale(glam::vec3(ndc_w, ndc_h, 1.0)),
    )
}

/// Elige imagen localizada (es/en) para empaquetados en pares.
pub fn pick_localized_screen_hud(
    locale: &str,
    primary: Option<ScreenHudPackedImage>,
    fallback: Option<ScreenHudPackedImage>,
) -> Option<ScreenHudPackedImage> {
    if locale == "en" {
        primary.or(fallback)
    } else {
        fallback.or(primary)
    }
}
