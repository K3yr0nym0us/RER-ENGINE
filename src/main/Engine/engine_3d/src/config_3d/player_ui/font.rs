//! Rasterizado TTF/OTF para UI del jugador (un bitmap + quad por cuadro, estilo Label de Godot).

use std::sync::Arc;

use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use glam::Mat4;

use super::config::UiHudRect;
use crate::mesh;
use crate::screen_hud_image::{self, ScreenHudAtlas, ScreenHudPackedImage};

pub(crate) fn load_font_arc(path: &str) -> Option<Arc<FontArc>> {
    let bytes = std::fs::read(path).ok()?;
    FontArc::try_from_vec(bytes).ok().map(Arc::new)
}

/// Segmento vertical del cursor en NDC (z = 0).
pub(crate) struct TextCaretLineNdc {
    pub x:  f32,
    pub y0: f32,
    pub y1: f32,
}

/// Posición horizontal del cursor alineada con el layout de `rasterize_text_block`.
pub(crate) fn text_caret_line_ndc(
    font: &FontArc,
    text: &str,
    caret_char: usize,
    box_rect: &UiHudRect,
    viewport_w: f32,
    viewport_h: f32,
) -> Option<TextCaretLineNdc> {
    if viewport_w <= 0.0 || viewport_h <= 0.0 {
        return None;
    }

    let width_px = (box_rect.width * 0.5 * viewport_w).round().max(8.0) as u32;
    let height_px = (box_rect.height * 0.5 * viewport_h).round().max(8.0) as u32;
    let font_px = (box_rect.height * 0.5 * viewport_h * 0.62).clamp(10.0, 72.0);

    let caret_x_px = layout_caret_x_px(font, text, caret_char, width_px, height_px, font_px);

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

fn layout_caret_x_px(
    font: &FontArc,
    text: &str,
    caret_char: usize,
    width_px: u32,
    height_px: u32,
    font_px: f32,
) -> f32 {
    if text.is_empty() {
        return width_px as f32 * 0.5;
    }

    let Some(boundaries) = text_x_boundaries_in_bitmap(font, text, width_px, height_px, font_px) else {
        return width_px as f32 * 0.5;
    };
    let idx = caret_char.min(boundaries.len().saturating_sub(1));
    boundaries[idx].clamp(0.0, width_px as f32)
}

/// `boundaries[i]` = posición X en el bitmap antes del carácter `i` (y tras el último).
fn text_x_boundaries_in_bitmap(
    font: &FontArc,
    text: &str,
    width_px: u32,
    height_px: u32,
    font_px: f32,
) -> Option<Vec<f32>> {
    let scale = PxScale::from(font_px);
    let scaled = font.as_scaled(scale);
    let baseline_y_up = 0.0_f32;

    let mut pen_x = 0.0_f32;
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut glyphs = Vec::new();
    let mut boundaries = vec![0.0_f32];

    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            continue;
        }
        let mut glyph = scaled.scaled_glyph(ch);
        glyph.position = point(pen_x, baseline_y_up);
        pen_x += scaled.h_advance(glyph.id);
        boundaries.push(pen_x);
        if let Some(outlined) = font.outline_glyph(glyph.clone()) {
            let b = outlined.px_bounds();
            min_x = min_x.min(b.min.x);
            max_x = max_x.max(b.max.x);
            min_y = min_y.min(b.min.y);
            max_y = max_y.max(b.max.y);
            glyphs.push(glyph);
        }
    }

    if glyphs.is_empty() || !min_x.is_finite() {
        return None;
    }

    let ink_w = max_x - min_x;
    let ink_h = max_y - min_y;
    let shift_x = (width_px as f32 - ink_w) * 0.5 - min_x;
    let _shift_y = (height_px as f32 - ink_h) * 0.5 - min_y;

    Some(
        boundaries
            .into_iter()
            .map(|x| x + shift_x)
            .collect(),
    )
}

/// Layout en píxeles → raster → atlas → un quad con UV (como Godot `Label` / Slate font cache).
pub(crate) fn build_hud_label_glyph(
    font: &FontArc,
    text: &str,
    box_rect: UiHudRect,
    tint: [f32; 4],
    viewport_w: f32,
    viewport_h: f32,
    atlas: &mut ScreenHudAtlas,
    queue: &wgpu::Queue,
    out: &mut Vec<mesh::InstanceData>,
) {
    if viewport_w <= 0.0 || viewport_h <= 0.0 || text.is_empty() {
        return;
    }

    let width_px = (box_rect.width * 0.5 * viewport_w).round().max(8.0) as u32;
    let height_px = (box_rect.height * 0.5 * viewport_h).round().max(8.0) as u32;
    let font_px = (box_rect.height * 0.5 * viewport_h * 0.62).clamp(10.0, 72.0);

    let Some(rgba) = rasterize_text_block(font, text, width_px, height_px, font_px, tint) else {
        return;
    };

    let packed = atlas.pack_rgba(queue, &rgba, width_px, height_px);

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
    ) {
        out.push(screen_hud_image::build_screen_hud_instance(packed, model, 1.0));
    }
}

/// Empaqueta la imagen en el atlas (o devuelve la entrada de caché).
pub(crate) fn pack_hud_texture_cached(
    path: &str,
    cache: &mut std::collections::HashMap<String, ScreenHudPackedImage>,
    atlas: &mut ScreenHudAtlas,
    queue: &wgpu::Queue,
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
    let packed = atlas.pack_rgba(queue, img.as_raw(), w, h);
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

    if let Some(model) = ndc_transform_top_left(x0_px, y0_px, w_px, h_px, viewport_w, viewport_h) {
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
    queue: &wgpu::Queue,
    out: &mut Vec<mesh::InstanceData>,
    opacity: f32,
) {
    let Some(packed) = pack_hud_texture_cached(path, cache, atlas, queue) else {
        log::warn!("[player-ui] textura no legible: {path}");
        return;
    };
    push_hud_texture_quad_instance(
        packed,
        box_rect,
        viewport_w,
        viewport_h,
        opacity,
        out,
    );
}

/// Buffer RGBA: origen arriba-izquierda, filas hacia abajo (igual que textura GPU).
fn rasterize_text_block(
    font: &FontArc,
    text: &str,
    width_px: u32,
    height_px: u32,
    font_px: f32,
    tint: [f32; 4],
) -> Option<Vec<u8>> {
    let mut rgba = vec![0u8; (width_px * height_px * 4) as usize];
    let scale = PxScale::from(font_px);
    let scaled = font.as_scaled(scale);

    // Paso 1: layout provisional y caja de tinta real (mejor para fuentes script/decorativas).
    let mut pen_x = 0.0_f32;
    let baseline_y_up = 0.0_f32;
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut glyphs = Vec::new();

    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            continue;
        }
        let mut glyph = scaled.scaled_glyph(ch);
        glyph.position = point(pen_x, baseline_y_up);
        pen_x += scaled.h_advance(glyph.id);
        if let Some(outlined) = font.outline_glyph(glyph.clone()) {
            let b = outlined.px_bounds();
            min_x = min_x.min(b.min.x);
            max_x = max_x.max(b.max.x);
            min_y = min_y.min(b.min.y);
            max_y = max_y.max(b.max.y);
            glyphs.push(glyph);
        }
    }

    if glyphs.is_empty() || !min_x.is_finite() {
        return Some(rgba);
    }

    let ink_w = max_x - min_x;
    let ink_h = max_y - min_y;
    let shift_x = (width_px as f32 - ink_w) * 0.5 - min_x;
    let shift_y_up = (height_px as f32 - ink_h) * 0.5 - min_y;

    for glyph in &mut glyphs {
        glyph.position.x += shift_x;
        glyph.position.y += shift_y_up;
    }

    for glyph in glyphs {
        let Some(outlined) = font.outline_glyph(glyph) else {
            continue;
        };

        let bounds = outlined.px_bounds();
        outlined.draw(|x, y, coverage| {
            // x,y: offset dentro del rect del glifo (ver ejemplos oficiales ab_glyph).
            let ax = bounds.min.x + x as f32;
            let ay_up = bounds.min.y + y as f32;
            let col = ax.round() as i32;
            let row = (height_px as f32 - ay_up - 1.0).round() as i32;
            if col < 0 || row < 0 {
                return;
            }
            let col = col as u32;
            let row = row as u32;
            if col >= width_px || row >= height_px {
                return;
            }
            let a = (coverage * tint[3] * 255.0).round() as u8;
            if a == 0 {
                return;
            }
            let i = ((row * width_px + col) * 4) as usize;
            rgba[i] = (tint[0] * 255.0).round() as u8;
            rgba[i + 1] = (tint[1] * 255.0).round() as u8;
            rgba[i + 2] = (tint[2] * 255.0).round() as u8;
            rgba[i + 3] = a;
        });
    }

    Some(rgba)
}

fn ndc_transform_top_left(
    px: f32,
    py: f32,
    w_px: f32,
    h_px: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> Option<Mat4> {
    if w_px <= 0.0 || h_px <= 0.0 {
        return None;
    }
    let ndc_w = 2.0 * w_px / viewport_w;
    let ndc_h = 2.0 * h_px / viewport_h;
    let cx = -1.0 + (px / viewport_w) * 2.0 + ndc_w * 0.5;
    let cy = 1.0 - (py / viewport_h) * 2.0 - ndc_h * 0.5;
    // Bitmap en y-up; el quad HUD invierte solo V (Y), no U.
    Some(
        Mat4::from_translation(glam::vec3(cx, cy, 0.0))
            * Mat4::from_scale(glam::vec3(ndc_w, -ndc_h, 1.0)),
    )
}
