//! Overlay de métricas dentro del viewport del motor.
//!
//! Se dibuja sobre la escena (mismo swapchain) en la esquina superior derecha,
//! como el contador de FPS de un juego. Los valores los lee el host directamente
//! del motor en cada frame, no del evento `debug_metrics` (1 Hz).

use egui::Color32;

use crate::editor::app::DebugMetrics;
use crate::editor::cli::Locale;
use crate::editor::i18n::tr;
use crate::editor::theme;

const WIDTH: f32 = 196.0;
const PAD: f32 = 9.0;
const ROW_H: f32 = 17.0;
const HEADER_H: f32 = 18.0;
const FONT_SIZE: f32 = 11.5;

/// Dibuja el panel si hay métricas. `viewport` es el rect de la escena en puntos egui.
pub(crate) fn show(
    ui: &mut egui::Ui,
    viewport: egui::Rect,
    metrics: Option<DebugMetrics>,
    locale: Locale,
) {
    let Some(m) = metrics else {
        return;
    };

    let height = PAD * 2.0 + HEADER_H + ROW_H * 3.0;
    let rect = egui::Rect::from_min_size(
        egui::pos2(viewport.right() - WIDTH - 12.0, viewport.top() + 12.0),
        egui::vec2(WIDTH, height),
    );
    if rect.width() <= 0.0 || rect.right() > viewport.right() || rect.bottom() > viewport.bottom() {
        return;
    }

    // Fondo translúcido + borde, como un HUD de juego.
    let bg = Color32::from_rgba_premultiplied(
        theme::METRICS_BG.r(),
        theme::METRICS_BG.g(),
        theme::METRICS_BG.b(),
        214,
    );
    ui.painter().rect_filled(rect, 6.0, bg);
    ui.painter().rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(1.0, theme::METRICS_BORDER),
        egui::StrokeKind::Inside,
    );

    let header_font = egui::FontId::new(11.0, egui::FontFamily::Name("rer-bold".into()));
    ui.painter().text(
        egui::pos2(rect.center().x, rect.top() + PAD + HEADER_H * 0.5),
        egui::Align2::CENTER_CENTER,
        tr(locale, "Métricas de uso", "Usage metrics"),
        header_font,
        theme::TEXT_MUTED,
    );
    let sep_y = rect.top() + PAD + HEADER_H;
    ui.painter().hline(
        egui::Rangef::new(rect.left() + PAD, rect.right() - PAD),
        sep_y,
        egui::Stroke::new(1.0, theme::METRICS_BORDER),
    );

    let col_w = (WIDTH - PAD * 2.0) * 0.5 - 4.0;
    let row_y = |index: u32| sep_y + ROW_H * (index as f32 + 0.5);

    pair(
        ui,
        rect.left() + PAD,
        row_y(0),
        col_w,
        "FPS",
        &format!("{:.1}", m.fps),
        fps_color(m.fps),
    );
    pair(
        ui,
        rect.left() + PAD + col_w + 8.0,
        row_y(0),
        col_w,
        "Frame",
        &format!("{:.1} ms", m.frame_time_ms),
        theme::TEXT,
    );
    pair(
        ui,
        rect.left() + PAD,
        row_y(1),
        col_w,
        "Draws",
        &m.draw_calls.to_string(),
        theme::TEXT,
    );
    pair(
        ui,
        rect.left() + PAD + col_w + 8.0,
        row_y(1),
        col_w,
        "Bodies",
        &m.physics_bodies.to_string(),
        theme::TEXT,
    );
    pair(
        ui,
        rect.left() + PAD,
        row_y(2),
        col_w,
        "CPU",
        &format_percent(m.cpu_percent),
        theme::TEXT,
    );
    pair(
        ui,
        rect.left() + PAD + col_w + 8.0,
        row_y(2),
        col_w,
        "GPU",
        &format_percent(m.gpu_percent),
        theme::TEXT,
    );
}

fn pair(ui: &egui::Ui, x: f32, y: f32, width: f32, label: &str, value: &str, color: Color32) {
    let font = egui::FontId::monospace(FONT_SIZE);
    ui.painter().text(
        egui::pos2(x, y),
        egui::Align2::LEFT_CENTER,
        label,
        font.clone(),
        theme::TEXT_MUTED,
    );
    ui.painter().text(
        egui::pos2(x + width, y),
        egui::Align2::RIGHT_CENTER,
        value,
        font,
        color,
    );
}

fn format_percent(value: Option<f32>) -> String {
    match value {
        Some(value) if value.is_finite() => format!("{value:.1}%"),
        _ => "—".to_string(),
    }
}

fn fps_color(fps: f32) -> Color32 {
    if fps >= 55.0 {
        theme::METRIC_FPS_GOOD
    } else if fps >= 30.0 {
        theme::METRIC_FPS_WARN
    } else {
        theme::METRIC_FPS_BAD
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_formats_and_handles_missing() {
        assert_eq!(format_percent(Some(12.34)), "12.3%");
        assert_eq!(format_percent(Some(0.0)), "0.0%");
        assert_eq!(format_percent(None), "—");
        assert_eq!(format_percent(Some(f32::NAN)), "—");
    }

    #[test]
    fn fps_thresholds_match_the_sidebar_palette() {
        assert_eq!(fps_color(60.0), theme::METRIC_FPS_GOOD);
        assert_eq!(fps_color(55.0), theme::METRIC_FPS_GOOD);
        assert_eq!(fps_color(45.0), theme::METRIC_FPS_WARN);
        assert_eq!(fps_color(30.0), theme::METRIC_FPS_WARN);
        assert_eq!(fps_color(20.0), theme::METRIC_FPS_BAD);
    }
}
