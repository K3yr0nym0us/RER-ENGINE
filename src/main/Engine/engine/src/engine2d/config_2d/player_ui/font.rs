//! Rasterizado de texto HUD mediante el `TextSystem` compartido con i18n.

use rer_engine_core::player_ui::viewport::ndc_transform_top_left;

use super::config::UiHudRect;
use crate::engine2d::mesh;
use crate::engine2d::screen_hud_image::{self, ScreenHudAtlas, ScreenHudPackedImage};
use crate::text::{TextStyle, TextSystem};

/// Texto rasterizado: el bitmap ya compensa Y; el quad invierte V en el modelo.
const HUD_TEXT_FLIP_BITMAP_Y: bool = true;
/// PNG/JPEG del accordion: en 2D el pass HUD no requiere invertir V (el texto sí).
const HUD_FILE_TEXTURE_FLIP_BITMAP_Y: bool = false;

/// Segmento vertical del cursor en NDC (z = 0).
pub(crate) struct TextCaretLineNdc {
    pub x: f32,
    pub y0: f32,
    pub y1: f32,
}

pub(crate) fn hud_pixel_size(box_rect: UiHudRect, viewport_w: f32, viewport_h: f32) -> (u32, u32) {
    (
        (box_rect.width * 0.5 * viewport_w).round().max(8.0) as u32,
        (box_rect.height * 0.5 * viewport_h).round().max(8.0) as u32,
    )
}

/// Posición horizontal del cursor derivada del mismo layout que el rasterizado.
pub(crate) fn text_caret_line_ndc(
    text_system: &mut TextSystem,
    text: &str,
    caret_char: usize,
    style: &TextStyle,
    box_rect: &UiHudRect,
    viewport_w: f32,
    viewport_h: f32,
) -> Option<TextCaretLineNdc> {
    if viewport_w <= 0.0 || viewport_h <= 0.0 {
        return None;
    }

    let (width_px, _) = hud_pixel_size(*box_rect, viewport_w, viewport_h);
    let caret_x_px = text_system
        .caret_x(text, caret_char, style)
        .unwrap_or(width_px as f32 * 0.5);

    let hw = box_rect.width * 0.5;
    let hh = box_rect.height * 0.5;
    let x0_ndc = box_rect.center_x - hw;
    let x0_px = (x0_ndc + 1.0) * 0.5 * viewport_w;
    let caret_px = x0_px + caret_x_px;
    let caret_ndc_x = (caret_px / viewport_w) * 2.0 - 1.0;

    let pad_y = box_rect.height * 0.1;
    Some(TextCaretLineNdc {
        x: caret_ndc_x,
        y0: box_rect.center_y - hh + pad_y,
        y1: box_rect.center_y + hh - pad_y,
    })
}

/// Layout en píxeles → raster → atlas → un quad con UV.
pub(crate) fn build_hud_label_glyph(
    text_system: &mut TextSystem,
    text: &str,
    style: &TextStyle,
    box_rect: UiHudRect,
    viewport_w: f32,
    viewport_h: f32,
    atlas: &mut ScreenHudAtlas,
    out: &mut Vec<mesh::InstanceData>,
) {
    if viewport_w <= 0.0 || viewport_h <= 0.0 || text.is_empty() {
        return;
    }

    let (width_px, height_px) = hud_pixel_size(box_rect, viewport_w, viewport_h);
    let raster = text_system.rasterize(text, style);
    let packed = atlas.pack_rgba(&raster.rgba, raster.width, raster.height);

    let hw = box_rect.width * 0.5;
    let hh = box_rect.height * 0.5;
    let x0_ndc = box_rect.center_x - hw;
    let y1_ndc = box_rect.center_y + hh;
    let x0_px = (x0_ndc + 1.0) * 0.5 * viewport_w;
    let y0_px = (1.0 - y1_ndc) * 0.5 * viewport_h;

    if let Some(model) = ndc_transform_top_left(
        x0_px,
        y0_px,
        width_px as f32,
        height_px as f32,
        viewport_w,
        viewport_h,
        HUD_TEXT_FLIP_BITMAP_Y,
    ) {
        out.push(screen_hud_image::build_screen_hud_instance(
            packed, model, 1.0,
        ));
    }
}

/// Empaqueta la imagen en el atlas (o devuelve la entrada de caché).
pub(crate) fn pack_hud_texture_cached(
    path: &str,
    cache: &mut std::collections::HashMap<String, ScreenHudPackedImage>,
    atlas: &mut ScreenHudAtlas,
) -> Option<ScreenHudPackedImage> {
    if let Some(packed) = cache.get(path) {
        return Some(*packed);
    }
    let bytes = std::fs::read(path).ok()?;
    use image::ImageReader;
    let img = ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?
        .to_rgba8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let packed = atlas.pack_rgba(img.as_raw(), w, h);
    cache.insert(path.to_string(), packed);
    Some(packed)
}

/// Instancia de quad HUD con UV ya empaquetados (solo transforma al rect NDC).
pub(crate) fn push_hud_texture_quad_instance(
    packed: ScreenHudPackedImage,
    box_rect: UiHudRect,
    viewport_w: f32,
    viewport_h: f32,
    opacity: f32,
    out: &mut Vec<mesh::InstanceData>,
) {
    if viewport_w <= 0.0 || viewport_h <= 0.0 {
        return;
    }
    let hw = box_rect.width * 0.5;
    let hh = box_rect.height * 0.5;
    let x0_ndc = box_rect.center_x - hw;
    let y1_ndc = box_rect.center_y + hh;
    let x0_px = (x0_ndc + 1.0) * 0.5 * viewport_w;
    let y0_px = (1.0 - y1_ndc) * 0.5 * viewport_h;
    let w_px = box_rect.width * 0.5 * viewport_w;
    let h_px = box_rect.height * 0.5 * viewport_h;

    if let Some(model) = ndc_transform_top_left(
        x0_px,
        y0_px,
        w_px,
        h_px,
        viewport_w,
        viewport_h,
        HUD_FILE_TEXTURE_FLIP_BITMAP_Y,
    ) {
        out.push(screen_hud_image::build_screen_hud_instance(
            packed,
            model,
            opacity.clamp(0.0, 1.0),
        ));
    }
}

/// Textura HUD con caché de UV (rebuild completo o preview en vivo).
pub(crate) fn build_hud_texture_quad_cached(
    path: &str,
    box_rect: UiHudRect,
    viewport_w: f32,
    viewport_h: f32,
    cache: &mut std::collections::HashMap<String, ScreenHudPackedImage>,
    atlas: &mut ScreenHudAtlas,
    out: &mut Vec<mesh::InstanceData>,
    opacity: f32,
) {
    let Some(packed) = pack_hud_texture_cached(path, cache, atlas) else {
        log::warn!("[player-ui] textura no legible: {path}");
        return;
    };
    push_hud_texture_quad_instance(packed, box_rect, viewport_w, viewport_h, opacity, out);
}
