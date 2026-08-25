//! Hit-test y helpers de resize/snap para edición HUD (sin dependencia de `State`).

use crate::player_ui::config::{
    self, HasUiHudRect, PlayerUiButton, PlayerUiImage, PlayerUiObject, PlayerUiResizeHandle,
    PlayerUiTextBox, UiHudRect, box_corners,
};
use crate::player_ui::geometry::point_in_polygon;
use crate::player_ui::grid::{player_ui_grid_steps, snap_ui_hud_rect_to_grid};
use crate::player_ui::hud_layers::{self, HudLayerKind};
use crate::player_ui::text_input::types::{HANDLE_RADIUS, MIN_BOX_H, MIN_BOX_W, PlayerUiHitTarget};

pub fn hit_test_box(boxes: &[PlayerUiTextBox], id: u32, ndc: [f32; 2]) -> bool {
    let Some(b) = boxes.iter().find(|b| b.id == id) else {
        return false;
    };
    let (x0, y0, x1, y1) = box_corners(b.ui_hud_rect());
    ndc_in_rect(ndc, x0, y0, x1, y1)
}

pub fn hit_test_handle_at_rect(rect: UiHudRect, ndc: [f32; 2]) -> Option<PlayerUiResizeHandle> {
    let (x0, y0, x1, y1) = config::box_corners(rect);
    let corners = [
        (PlayerUiResizeHandle::TopLeft, x0, y1),
        (PlayerUiResizeHandle::TopRight, x1, y1),
        (PlayerUiResizeHandle::BottomLeft, x0, y0),
        (PlayerUiResizeHandle::BottomRight, x1, y0),
    ];
    for (handle, cx, cy) in corners {
        let dx = ndc[0] - cx;
        let dy = ndc[1] - cy;
        if dx * dx + dy * dy <= HANDLE_RADIUS * HANDLE_RADIUS {
            return Some(handle);
        }
    }
    None
}

pub fn hit_test_text_handle(
    boxes: &[PlayerUiTextBox],
    selected_id: u32,
    ndc: [f32; 2],
) -> Option<PlayerUiResizeHandle> {
    let b = boxes.iter().find(|b| b.id == selected_id)?;
    hit_test_handle_at_rect(b.ui_hud_rect(), ndc)
}

pub fn hit_test_image_handle(
    images: &[PlayerUiImage],
    selected_id: u32,
    ndc: [f32; 2],
) -> Option<PlayerUiResizeHandle> {
    let img = images.iter().find(|i| i.id == selected_id)?;
    hit_test_handle_at_rect(img.ui_hud_rect(), ndc)
}

pub fn hit_test_button_handle(
    buttons: &[PlayerUiButton],
    selected_id: u32,
    ndc: [f32; 2],
) -> Option<PlayerUiResizeHandle> {
    let btn = buttons.iter().find(|b| b.id == selected_id)?;
    hit_test_handle_at_rect(btn.ui_hud_rect(), ndc)
}

pub fn hit_test_top_hud(
    texts: &[PlayerUiTextBox],
    buttons: &[PlayerUiButton],
    images: &[PlayerUiImage],
    objects: &[PlayerUiObject],
    ndc: [f32; 2],
) -> Option<PlayerUiHitTarget> {
    let order = hud_layers::hud_hit_test_order(texts, buttons, images, objects);
    for layer in order {
        match layer.kind {
            HudLayerKind::Object => {
                let obj = &objects[layer.index];
                if obj.locked {
                    continue;
                }
                if point_in_polygon(ndc, &obj.vertices) {
                    return Some(PlayerUiHitTarget::Object(obj.id));
                }
            }
            HudLayerKind::Image => {
                let img = &images[layer.index];
                if img.locked {
                    continue;
                }
                if hit_test_hud_rect(img.ui_hud_rect(), ndc) {
                    return Some(PlayerUiHitTarget::Image(img.id));
                }
            }
            HudLayerKind::Button => {
                let btn = &buttons[layer.index];
                if btn.locked {
                    continue;
                }
                if hit_test_hud_rect(btn.ui_hud_rect(), ndc) {
                    return Some(PlayerUiHitTarget::Button(btn.id));
                }
            }
            HudLayerKind::Text => {
                let b = &texts[layer.index];
                if b.locked {
                    continue;
                }
                if hit_test_hud_rect(b.ui_hud_rect(), ndc) {
                    return Some(PlayerUiHitTarget::Text(b.id));
                }
            }
        }
    }
    None
}

pub fn snap_player_ui_element_move(
    center_x: f32,
    center_y: f32,
    width: f32,
    height: f32,
    vw: f32,
    vh: f32,
) -> (f32, f32) {
    let (step_x, step_y) = player_ui_grid_steps(vw, vh);
    let mut rect = UiHudRect {
        center_x,
        center_y,
        width,
        height,
    };
    snap_ui_hud_rect_to_grid(&mut rect, step_x, step_y);
    (rect.center_x, rect.center_y)
}

pub fn apply_resize(
    b: &mut PlayerUiTextBox,
    start: &PlayerUiTextBox,
    handle: PlayerUiResizeHandle,
    start_mouse: [f32; 2],
    cur_mouse: [f32; 2],
) {
    let (sx0, sy0, sx1, sy1) = box_corners(start.ui_hud_rect());
    let dx = cur_mouse[0] - start_mouse[0];
    let dy = cur_mouse[1] - start_mouse[1];
    let (mut x0, mut y0, mut x1, mut y1) = (sx0, sy0, sx1, sy1);
    match handle {
        PlayerUiResizeHandle::TopLeft => {
            x0 = (sx0 + dx).min(sx1 - MIN_BOX_W);
            y1 = (sy1 + dy).max(sy0 + MIN_BOX_H);
        }
        PlayerUiResizeHandle::TopRight => {
            x1 = (sx1 + dx).max(sx0 + MIN_BOX_W);
            y1 = (sy1 + dy).max(sy0 + MIN_BOX_H);
        }
        PlayerUiResizeHandle::BottomLeft => {
            x0 = (sx0 + dx).min(sx1 - MIN_BOX_W);
            y0 = (sy0 + dy).min(sy1 - MIN_BOX_H);
        }
        PlayerUiResizeHandle::BottomRight => {
            x1 = (sx1 + dx).max(sx0 + MIN_BOX_W);
            y0 = (sy0 + dy).min(sy1 - MIN_BOX_H);
        }
    }
    b.center_x = (x0 + x1) * 0.5;
    b.center_y = (y0 + y1) * 0.5;
    b.width = (x1 - x0).max(MIN_BOX_W);
    b.height = (y1 - y0).max(MIN_BOX_H);
}

pub fn char_index_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

fn ndc_in_rect(ndc: [f32; 2], x0: f32, y0: f32, x1: f32, y1: f32) -> bool {
    ndc[0] >= x0 && ndc[0] <= x1 && ndc[1] >= y0 && ndc[1] <= y1
}

fn hit_test_hud_rect(rect: UiHudRect, ndc: [f32; 2]) -> bool {
    let (x0, y0, x1, y1) = config::box_corners(rect);
    ndc_in_rect(ndc, x0, y0, x1, y1)
}
