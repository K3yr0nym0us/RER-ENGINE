//! Marco dividido del editor (Fase 1).
//!
//! Reproduce la estructura de `EngineView` en React:
//! - Sidebars de 268 px a toda la altura.
//! - Columna central con barra superior, viewport del motor y consola.
//!
//! En esta fase solo se dibujan las cajas y sus etiquetas: los componentes
//! reales llegan en las fases 2–4.

use crate::editor::app::EditorApp;

use super::{bottom_bar, sidebar_left, sidebar_right, top_bar};

pub fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    let text = app.text();
    let full_height = ui.max_rect().height();
    let console_height = console_height(full_height);

    // Las sidebars ocupan toda la altura, como en React.
    egui::Panel::left("editor_sidebar_left")
        .exact_size(crate::editor::theme::SIDEBAR_WIDTH)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(crate::editor::theme::SIDEBAR_BG))
        .show(ui, |ui| {
            sidebar_left::show(ui, app);
        });

    egui::Panel::right("editor_sidebar_right")
        .exact_size(crate::editor::theme::SIDEBAR_WIDTH)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(crate::editor::theme::SIDEBAR_BG))
        .show(ui, |ui| {
            sidebar_right::show(ui, app);
        });

    // Barra superior y consola solo en la columna central.
    egui::Panel::top("editor_top_bar")
        .exact_size(crate::editor::theme::TOP_BAR_HEIGHT)
        .show_separator_line(false)
        .frame(panel_frame(crate::editor::theme::SIDEBAR_BG))
        .show(ui, |ui| {
            top_bar::show(ui, app);
        });

    egui::Panel::bottom("editor_log_console")
        .exact_size(console_height)
        .show_separator_line(false)
        .frame(panel_frame(crate::editor::theme::LOG_BG))
        .show(ui, |ui| {
            bottom_bar::show(ui, app);
        });

    // El centro queda limpio: aquí se dibuja la escena del motor en el mismo swapchain.
    // Mientras arranca, solo se muestra un texto en esta área; el resto de la UI ya está vivo.
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            let rect = ui.max_rect();
            app.viewport_rect = Some(rect);
            let (message, color) = if app.engine_loading {
                (
                    Some(text.loading_engine.to_string()),
                    crate::editor::theme::TEXT_MUTED,
                )
            } else if let Some(error) = app.engine_error.as_deref() {
                (
                    Some(format!("{}: {error}", text.engine_error)),
                    crate::editor::theme::DANGER,
                )
            } else {
                (None, crate::editor::theme::TEXT_MUTED)
            };
            if let Some(message) = message {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    message,
                    egui::FontId::proportional(16.0),
                    color,
                );
            }
        });
}

fn panel_frame(fill: egui::Color32) -> egui::Frame {
    egui::Frame::new().fill(fill)
}

/// Altura de la consola: 25 % del alto disponible, con un mínimo de 120 px.
fn console_height(full_height: f32) -> f32 {
    (full_height * 0.25).max(120.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_is_a_quarter_of_the_height() {
        assert_eq!(console_height(800.0), 200.0);
        assert_eq!(console_height(1080.0), 270.0);
    }

    #[test]
    fn console_never_goes_below_the_minimum() {
        assert_eq!(console_height(400.0), 120.0);
        assert_eq!(console_height(0.0), 120.0);
    }
}
