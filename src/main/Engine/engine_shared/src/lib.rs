pub mod assets;
pub mod bundled_models;
pub mod editor_defaults;
pub mod scripting;
pub mod gpu;
pub mod wgpu_surface;
pub mod logging;
pub mod overlay;
pub mod player_ui;
pub mod process_metrics;
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub mod platform;

/// Magnitud de gravedad por defecto del mundo (m/s², positiva = hacia abajo).
pub const DEFAULT_GRAVITY_MAGNITUDE: f32 = 15.0;

pub fn point_to_segment_2d(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-6 {
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }
    let t = ((px - ax) * dx + (py - ay) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let cx = ax + t * dx;
    let cy = ay + t * dy;
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}
