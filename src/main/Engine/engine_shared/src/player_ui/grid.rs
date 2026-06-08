//! Cuadrícula NDC y snap para edición de Player UI.

use super::config::UiHudRect;

/// Divisiones fijas de la regla en pantalla (NDC): máximo = celdas más pequeñas.
pub const PLAYER_UI_GRID_DIVISIONS: u32 = 48;

/// Uniformes NDC (view_proj + model identidad), mismo layout que crosshair / grid 2D.
pub const NDC_SCREEN_UNIFORM: [[f32; 4]; 9] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
    [-1.0, -1.0, 0.0, 0.0],
];

/// Pasos NDC de la cuadrícula UI con celdas cuadradas en píxeles de pantalla.
pub fn player_ui_grid_steps(viewport_w: f32, viewport_h: f32) -> (f32, f32) {
    let aspect = (viewport_w.max(1.0)) / viewport_h.max(1.0);
    let divisions = PLAYER_UI_GRID_DIVISIONS as f32;
    let step_y = 2.0 / divisions;
    let step_x = step_y / aspect;
    (step_x, step_y)
}

/// Alinea un punto NDC al cruce de cuadrícula más cercano.
pub fn snap_ndc_point_to_grid(x: f32, y: f32, step_x: f32, step_y: f32) -> [f32; 2] {
    if step_x <= 1e-6 || step_y <= 1e-6 {
        return [x, y];
    }
    [(x / step_x).round() * step_x, (y / step_y).round() * step_y]
}

/// Alinea el borde más cercano del rectángulo HUD a la línea de cuadrícula más próxima.
pub fn snap_ui_hud_rect_to_grid(rect: &mut UiHudRect, step_x: f32, step_y: f32) {
    if step_x <= 1e-6 || step_y <= 1e-6 {
        return;
    }
    let hw = rect.width * 0.5;
    let left = rect.center_x - hw;
    let right = rect.center_x + hw;
    let left_snap = (left / step_x).round() * step_x;
    let right_snap = (right / step_x).round() * step_x;
    if (left - left_snap).abs() <= (right - right_snap).abs() {
        rect.center_x = left_snap + hw;
    } else {
        rect.center_x = right_snap - hw;
    }

    let hh = rect.height * 0.5;
    let bottom = rect.center_y - hh;
    let top = rect.center_y + hh;
    let bottom_snap = (bottom / step_y).round() * step_y;
    let top_snap = (top / step_y).round() * step_y;
    if (bottom - bottom_snap).abs() <= (top - top_snap).abs() {
        rect.center_y = bottom_snap + hh;
    } else {
        rect.center_y = top_snap - hh;
    }
}
