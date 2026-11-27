//! Renderizado NDC de botones HUD (forma, borde, textura, etiqueta).

use crate::engine2d::gizmo::GizmoVertex;
use crate::engine2d::screen_hud_image::ScreenHudAtlas;
use crate::text::hud_style;

use super::config::{HasUiHudRect, PlayerUiButton, UiHudRect, box_corners};
use super::font::build_hud_label_glyph;
use super::ndc_draw::{append_rect_fill, append_rect_outline, push_handle_disc, push_quad};
use super::text_render::HANDLE_RADIUS;

pub(crate) fn append_button_gizmo_verts(
    verts: &mut Vec<GizmoVertex>,
    buttons: &[PlayerUiButton],
    selected: Option<u32>,
) {
    for btn in buttons {
        let rect = btn.ui_hud_rect();
        let is_selected = selected == Some(btn.id);
        let mut fill = btn.background_color;
        fill[3] *= btn.transparency_background.clamp(0.0, 1.0);
        if btn.texture_path.is_none() {
            append_button_shape_fill(verts, btn, rect, fill);
        }
        if btn.border_weight > 0.0 {
            append_rect_outline(verts, rect, btn.border_color);
        }
        if is_selected {
            append_rect_outline(verts, rect, [1.0_f32, 0.85, 0.2, 0.95]);
            let (x0, y0, x1, y1) = box_corners(rect);
            let handle_color = [1.0_f32, 0.85, 0.2, 0.95];
            for (cx, cy) in [(x0, y1), (x1, y1), (x0, y0), (x1, y0)] {
                push_handle_disc(verts, cx, cy, HANDLE_RADIUS, handle_color);
            }
        }
    }
}

pub(crate) fn append_button_hud_glyphs(
    i18n: &mut crate::i18n::I18nRuntime,
    buttons: &[PlayerUiButton],
    atlas: &mut ScreenHudAtlas,
    instances: &mut Vec<crate::engine2d::mesh::InstanceData>,
    viewport_w: f32,
    viewport_h: f32,
    texture_cache: &mut std::collections::HashMap<
        String,
        crate::engine2d::screen_hud_image::ScreenHudPackedImage,
    >,
    text_cache: &mut std::collections::HashMap<
        String,
        crate::engine2d::screen_hud_image::ScreenHudPackedImage,
    >,
) -> bool {
    for btn in buttons {
        if let Some(path) = btn.texture_path.as_deref() {
            super::font::build_hud_texture_quad_cached(
                path,
                btn.ui_hud_rect(),
                viewport_w,
                viewport_h,
                texture_cache,
                atlas,
                instances,
                1.0,
            );
        }
        let Ok(resolved) = i18n.resolve(&btn.text) else {
            log::warn!(
                "[player-ui] no se pudo resolver texto para botón {}",
                btn.id
            );
            continue;
        };
        let text = resolved.display;
        if text.trim().is_empty() {
            continue;
        }
        let family = match i18n.ensure_font(&btn.font_path, &btn.font_name) {
            Ok(family) => family,
            Err(error) => {
                log::warn!(
                    "[player-ui] no se pudo cargar fuente para botón {}: {error}",
                    btn.id
                );
                continue;
            }
        };
        let mut tint = btn.text_color;
        tint[3] *= btn.transparency_text.clamp(0.0, 1.0);
        let rect = btn.ui_hud_rect();
        let (width_px, height_px) = super::font::hud_pixel_size(rect, viewport_w, viewport_h);
        let language = i18n.text_shaping_language(&btn.text);
        let style = hud_style(family, width_px, height_px, tint, Some(&language));
        let cache_key = super::font::hud_text_cache_key(&text, &style);
        if !build_hud_label_glyph(
            i18n.text_system_mut(),
            &text,
            &style,
            &cache_key,
            rect,
            viewport_w,
            viewport_h,
            atlas,
            text_cache,
            texture_cache,
            instances,
        ) {
            return false;
        }
    }
    true
}

fn append_button_shape_fill(
    verts: &mut Vec<GizmoVertex>,
    btn: &PlayerUiButton,
    rect: UiHudRect,
    color: [f32; 4],
) {
    match btn.shape_type.as_str() {
        "diamond" => push_diamond(verts, rect, color),
        "triangle" => push_triangle(verts, rect, color),
        "circle" => push_circle(verts, rect, color),
        _ => push_rounded_rect(verts, rect, btn.round, color),
    }
}

fn push_diamond(verts: &mut Vec<GizmoVertex>, rect: UiHudRect, color: [f32; 4]) {
    let (x0, y0, x1, y1) = super::config::box_corners(rect);
    let cx = rect.center_x;
    let cy = rect.center_y;
    push_quad(
        verts,
        [cx, y1, 0.0],
        [x1, cy, 0.0],
        [cx, y0, 0.0],
        [cx, y1, 0.0],
        color,
    );
    push_quad(
        verts,
        [cx, y1, 0.0],
        [x0, cy, 0.0],
        [cx, y0, 0.0],
        [x1, cy, 0.0],
        color,
    );
}

fn push_triangle(verts: &mut Vec<GizmoVertex>, rect: UiHudRect, color: [f32; 4]) {
    let (x0, y0, x1, y1) = super::config::box_corners(rect);
    let apex = [rect.center_x, y1, 0.0];
    let bl = [x0, y0, 0.0];
    let br = [x1, y0, 0.0];
    push_quad(verts, apex, bl, br, apex, color);
}

fn push_circle(verts: &mut Vec<GizmoVertex>, rect: UiHudRect, color: [f32; 4]) {
    let segments = 24;
    let rx = rect.width * 0.5;
    let ry = rect.height * 0.5;
    for i in 0..segments {
        let a0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
        let p0 = [
            rect.center_x + a0.cos() * rx,
            rect.center_y + a0.sin() * ry,
            0.0,
        ];
        let p1 = [
            rect.center_x + a1.cos() * rx,
            rect.center_y + a1.sin() * ry,
            0.0,
        ];
        push_quad(
            verts,
            [rect.center_x, rect.center_y, 0.0],
            p0,
            p1,
            [rect.center_x, rect.center_y, 0.0],
            color,
        );
    }
}

fn push_rounded_rect(
    verts: &mut Vec<GizmoVertex>,
    rect: UiHudRect,
    round_px: f32,
    color: [f32; 4],
) {
    let round = round_px.clamp(0.0, 64.0);
    if round < 1.0 {
        append_rect_fill(verts, rect, color);
        return;
    }
    // Aproximación: rectángulo interior + esquinas disc
    append_rect_fill(verts, rect, color);
}
