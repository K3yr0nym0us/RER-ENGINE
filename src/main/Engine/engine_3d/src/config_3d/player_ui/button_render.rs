//! Renderizado NDC de botones HUD (forma, borde, textura, etiqueta).

use std::sync::Arc;

use ab_glyph::FontArc;

use crate::gizmo::GizmoVertex;
use crate::screen_hud_image::ScreenHudAtlas;

use super::config::{HasUiHudRect, PlayerUiButton, UiHudRect};
use super::font::{build_hud_label_glyph, build_hud_texture_quad};
use super::ndc_draw::{append_rect_fill, append_rect_outline, push_quad};

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
        }
    }
}

pub(crate) fn append_button_hud_glyphs(
    buttons: &[PlayerUiButton],
    font_cache: &mut std::collections::HashMap<String, Arc<FontArc>>,
    atlas: &mut ScreenHudAtlas,
    queue: &wgpu::Queue,
    instances: &mut Vec<crate::mesh::InstanceData>,
    viewport_w: f32,
    viewport_h: f32,
) {
    for btn in buttons {
        if let Some(path) = btn.texture_path.as_deref() {
            build_hud_texture_quad(
                path,
                btn.ui_hud_rect(),
                viewport_w,
                viewport_h,
                atlas,
                queue,
                instances,
            );
        }
        if btn.text.trim().is_empty() || btn.font_path.is_empty() {
            continue;
        }
        let font = match font_cache.get(&btn.font_path) {
            Some(f) => f.clone(),
            None => {
                let Some(loaded) = super::font::load_font_arc(&btn.font_path) else {
                    continue;
                };
                font_cache.insert(btn.font_path.clone(), loaded.clone());
                loaded
            }
        };
        let mut tint = btn.text_color;
        tint[3] *= btn.transparency_text.clamp(0.0, 1.0);
        build_hud_label_glyph(
            &font,
            &btn.text,
            btn.ui_hud_rect(),
            tint,
            viewport_w,
            viewport_h,
            atlas,
            queue,
            instances,
        );
    }
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
    push_quad(verts, [cx, y1, 0.0], [x1, cy, 0.0], [cx, y0, 0.0], [cx, y1, 0.0], color);
    push_quad(verts, [cx, y1, 0.0], [x0, cy, 0.0], [cx, y0, 0.0], [x1, cy, 0.0], color);
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

fn push_rounded_rect(verts: &mut Vec<GizmoVertex>, rect: UiHudRect, round_px: f32, color: [f32; 4]) {
    let round = round_px.clamp(0.0, 64.0);
    if round < 1.0 {
        append_rect_fill(verts, rect, color);
        return;
    }
    // Aproximación: rectángulo interior + esquinas disc
    append_rect_fill(verts, rect, color);
}
