//! Renderizado de cuadros de texto HUD (marco gizmo + glifos + cursor).

use std::sync::Arc;

use ab_glyph::FontArc;

use crate::engine::State;
use crate::gizmo::{self, GizmoVertex};

use super::config::{box_corners, HasUiHudRect, PlayerUiTextBox};
use super::font::{build_hud_label_glyph, text_caret_line_ndc};
use super::ndc_draw::{push_handle_disc, push_line_segment, push_quad};

const HANDLE_RADIUS: f32 = 0.016;

pub(crate) fn append_text_box_gizmo_verts(
    verts: &mut Vec<GizmoVertex>,
    boxes: &[PlayerUiTextBox],
    selected: Option<u32>,
    editing: Option<u32>,
) {
    for b in boxes {
        let hw = b.width * 0.5;
        let hh = b.height * 0.5;
        let is_selected = selected == Some(b.id);
        let is_editing = editing == Some(b.id);
        let fill = if is_editing {
            [0.22_f32, 0.48, 0.95, 0.48]
        } else if is_selected {
            [0.2_f32, 0.46, 0.9, 0.42]
        } else {
            [0.18_f32, 0.42, 0.88, 0.38]
        };
        let border = if is_editing {
            [1.0_f32, 0.92, 0.35, 0.95]
        } else if is_selected {
            [0.98_f32, 0.99, 1.0, 0.95]
        } else {
            [0.92_f32, 0.94, 1.0, 0.88]
        };
        push_quad(
            verts,
            [b.center_x - hw, b.center_y - hh, 0.0],
            [b.center_x + hw, b.center_y - hh, 0.0],
            [b.center_x + hw, b.center_y + hh, 0.0],
            [b.center_x - hw, b.center_y + hh, 0.0],
            fill,
        );
        let (x0, y0, x1, y1) = box_corners(super::config::UiHudRect {
            center_x: b.center_x,
            center_y: b.center_y,
            width: b.width,
            height: b.height,
        });
        push_line_segment(verts, [x0, y0, 0.0], [x1, y0, 0.0], border);
        push_line_segment(verts, [x1, y0, 0.0], [x1, y1, 0.0], border);
        push_line_segment(verts, [x1, y1, 0.0], [x0, y1, 0.0], border);
        push_line_segment(verts, [x0, y1, 0.0], [x0, y0, 0.0], border);

        if is_selected && !is_editing {
            let handle_color = [1.0_f32, 0.85, 0.2, 0.95];
            for (cx, cy) in [(x0, y1), (x1, y1), (x0, y0), (x1, y0)] {
                push_handle_disc(verts, cx, cy, HANDLE_RADIUS, handle_color);
            }
        }
    }
}

pub(crate) fn append_text_glyphs(
    state: &mut State,
    boxes: &[PlayerUiTextBox],
    font_cache: &mut std::collections::HashMap<String, Arc<FontArc>>,
) {
    let vw = state.size.width.max(1) as f32;
    let vh = state.size.height.max(1) as f32;
    for b in boxes {
        let Some(font) = state.player_ui_font_cached_mut(font_cache, &b.font_path) else {
            log::warn!(
                "[player-ui] no se pudo cargar fuente para cuadro {}: {}",
                b.id,
                b.font_path
            );
            continue;
        };
        build_hud_label_glyph(
            &font,
            &b.text,
            super::config::UiHudRect {
                center_x: b.center_x,
                center_y: b.center_y,
                width: b.width,
                height: b.height,
            },
            [1.0, 1.0, 1.0, 1.0],
            vw,
            vh,
            &mut state.player_ui_text_atlas,
            &state.queue,
            &mut state.player_ui_glyph_instances,
        );
    }
}

pub(crate) fn rebuild_caret_buffer(state: &mut State, edit_id: u32) {
    let Some(b) = state.player_ui_boxes_for_id(edit_id).cloned() else {
        state.player_ui_caret_buffer = gizmo::build_from_vertices(&state.device, &[]);
        return;
    };
    let Some(font) = state.player_ui_font_cached(&b.font_path) else {
        state.player_ui_caret_buffer = gizmo::build_from_vertices(&state.device, &[]);
        return;
    };
    let vw = state.size.width.max(1) as f32;
    let vh = state.size.height.max(1) as f32;
    let rect = b.ui_hud_rect();
    let Some(line) =
        text_caret_line_ndc(&font, &b.text, state.player_ui_text_caret, &rect, vw, vh)
    else {
        state.player_ui_caret_buffer = gizmo::build_from_vertices(&state.device, &[]);
        return;
    };
    let mut verts = Vec::new();
    push_line_segment(
        &mut verts,
        [line.x, line.y0, 0.0],
        [line.x, line.y1, 0.0],
        [1.0, 1.0, 1.0, 0.95],
    );
    state.player_ui_caret_buffer = gizmo::build_from_vertices(&state.device, &verts);
}
