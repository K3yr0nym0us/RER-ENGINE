use super::*;

pub(super) fn metrics(ui: &mut egui::Ui, app: &EditorApp) {
    let locale = app.cli.locale;

    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(crate::editor::i18n::tr(
                locale,
                "Métricas de uso",
                "Usage metrics",
            ))
            .size(13.0)
            .strong()
            .color(crate::editor::theme::TEXT_MUTED),
        );
    });
    ui.add_space(7.0);

    let rect = ui.available_rect_before_wrap();
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        egui::Stroke::new(1.0, crate::editor::theme::METRICS_BORDER),
    );
    ui.add_space(7.0);

    let Some(metrics) = app.metrics else {
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(crate::editor::i18n::tr(
                    locale,
                    "Esperando métricas…",
                    "Waiting for metrics...",
                ))
                .size(12.0)
                .color(crate::editor::theme::TEXT_MUTED),
            );
        });
        return;
    };

    let width = ui.available_width();
    let col_left = width * 5.0 / 12.0;
    let col_right = (width - col_left - METRIC_GAP).max(0.0);

    egui::Grid::new("editor_right_metrics_grid")
        .num_columns(2)
        .spacing([METRIC_GAP, 0.0])
        .show(ui, |ui| {
            metric_cell(
                ui,
                col_left,
                "FPS",
                &format!("{:.1}", metrics.fps),
                Some(fps_color(metrics.fps)),
            );
            metric_cell(
                ui,
                col_right,
                "Frame Time",
                &format!("{:.1} ms", metrics.frame_time_ms),
                None,
            );
            ui.end_row();

            metric_cell(
                ui,
                col_left,
                "Draw Calls",
                &metrics.draw_calls.to_string(),
                None,
            );
            metric_cell(
                ui,
                col_right,
                "Physics Bodies",
                &metrics.physics_bodies.to_string(),
                None,
            );
            ui.end_row();

            metric_cell(
                ui,
                col_left,
                "CPU",
                &format_percent(metrics.cpu_percent),
                None,
            );
            metric_cell(
                ui,
                col_right,
                "GPU",
                &format_percent(metrics.gpu_percent),
                None,
            );
            ui.end_row();
        });
}

pub(super) fn metric_cell(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    value: &str,
    value_color: Option<egui::Color32>,
) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, METRIC_LINE_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.x = METRIC_GAP;
            ui.label(
                RichText::new(label)
                    .monospace()
                    .size(METRIC_FONT_SIZE)
                    .color(crate::editor::theme::TEXT_MUTED),
            );
            ui.label(
                RichText::new(value)
                    .monospace()
                    .size(METRIC_FONT_SIZE)
                    .color(value_color.unwrap_or(crate::editor::theme::TEXT)),
            );
        },
    );
}

pub(super) fn format_percent(value: Option<f32>) -> String {
    match value {
        Some(value) if value.is_finite() => format!("{value:.1}%"),
        _ => "—".to_string(),
    }
}

pub(super) fn fps_color(fps: f32) -> egui::Color32 {
    if fps >= 55.0 {
        crate::editor::theme::METRIC_FPS_GOOD
    } else if fps >= 30.0 {
        crate::editor::theme::METRIC_FPS_WARN
    } else {
        crate::editor::theme::METRIC_FPS_BAD
    }
}
