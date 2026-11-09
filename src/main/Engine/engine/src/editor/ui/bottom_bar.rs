//! Consola inferior del editor.
//!
//! Espejo de `LogConsole` en React: fondo `#03050d`, borde superior `#202545`,
//! monospace pequeño, verde para eventos y rojo para errores, con auto-scroll.

use egui::RichText;

use crate::editor::app::EditorApp;

/// Bootstrap `small` sobre base 14 px (0.875em).
const LOG_FONT_SIZE: f32 = 12.5;

pub fn show(ui: &mut egui::Ui, app: &EditorApp) {
    let text = app.text();

    // `border-top: 1px solid #202545` del `.console-panel`.
    let rect = ui.max_rect();
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        egui::Stroke::new(1.0, crate::editor::theme::METRICS_BORDER),
    );

    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(14, 7))
        .show(ui, |ui| {
            // Sin separación extra: React apila los <div> con el line-height.
            ui.spacing_mut().item_spacing.y = 0.0;

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if app.log.is_empty() {
                        ui.label(
                            RichText::new(text.empty_log)
                                .monospace()
                                .size(LOG_FONT_SIZE)
                                .color(crate::editor::theme::TEXT_MUTED),
                        );
                        return;
                    }

                    for entry in &app.log {
                        let color = if entry.is_error {
                            crate::editor::theme::DANGER
                        } else {
                            crate::editor::theme::SUCCESS
                        };
                        ui.label(
                            RichText::new(&entry.text)
                                .monospace()
                                .size(LOG_FONT_SIZE)
                                .color(color),
                        );
                    }
                });
        });
}
