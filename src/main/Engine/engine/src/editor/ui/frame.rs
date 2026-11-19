//! Marco dividido del editor (Fase 1).
//!
//! Reproduce la estructura de `EngineView` en React:
//! - Sidebars de 268 px a toda la altura.
//! - Columna central con barra superior, viewport del motor y consola.
//!
//! En esta fase solo se dibujan las cajas y sus etiquetas: los componentes
//! reales llegan en las fases 2–4.

use crate::editor::app::EditorApp;

use super::{bottom_bar, sidebar_left, sidebar_right, top_bar, widgets};

pub fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    let text = app.text();
    let full_height = ui.max_rect().height();
    let console_height = console_height(full_height);

    // Las sidebars ocupan toda la altura, como en React.
    egui::Panel::left("editor_sidebar_left")
        .exact_size(crate::editor::theme::SIDEBAR_WIDTH)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(crate::editor::theme::PANEL_BG))
        .show(ui, |ui| {
            sidebar_left::show(ui, app);
        });

    egui::Panel::right("editor_sidebar_right")
        .exact_size(crate::editor::theme::SIDEBAR_WIDTH)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(crate::editor::theme::PANEL_BG))
        .show(ui, |ui| {
            sidebar_right::show(ui, app);
        });

    // Barra superior y consola solo en la columna central.
    egui::Panel::top("editor_top_bar")
        .exact_size(crate::editor::theme::TOP_BAR_HEIGHT)
        .show_separator_line(false)
        .frame(panel_frame(crate::editor::theme::PANEL_BG))
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
            if app.engine_loading {
                // Spinner + texto centrados solo en el área del viewport.
                let center = rect.center();
                let spinner = egui::Rect::from_center_size(
                    center - egui::vec2(0.0, 22.0),
                    egui::vec2(28.0, 28.0),
                );
                ui.put(spinner, egui::Spinner::new().size(26.0));
                ui.painter().text(
                    center + egui::vec2(0.0, 16.0),
                    egui::Align2::CENTER_CENTER,
                    text.loading_engine,
                    egui::FontId::proportional(16.0),
                    crate::editor::theme::TEXT_MUTED,
                );
            } else if let Some(error) = app.engine_error.as_deref() {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{}: {error}", text.engine_error),
                    egui::FontId::proportional(16.0),
                    crate::editor::theme::DANGER,
                );
            } else if let Some(hint) = app.placement_hint.as_deref() {
                // Aviso flotante mientras una herramienta de colocación está
                // activa: el motor espera un click en el viewport, no en el sidebar.
                widgets::viewport_notification(
                    ui,
                    egui::Align2::CENTER_CENTER,
                    rect.center() - egui::vec2(0.0, rect.height() * 0.28),
                    rect.width() - 16.0,
                    hint,
                    egui::FontId::proportional(15.0),
                    crate::editor::theme::INFO,
                );
            }

            // Modo play: sustituye al tooltip PNG «Esc para salir / Esc to exit».
            if !app.engine_loading && app.engine_error.is_none() && app.preview_playing {
                let esc_hint = crate::editor::i18n::tr(
                    app.cli.locale,
                    "[ESC] para salir del modo juego",
                    "[ESC] to exit play mode",
                );
                widgets::viewport_notification(
                    ui,
                    egui::Align2::LEFT_CENTER,
                    egui::pos2(rect.min.x + 12.0, rect.max.y - 20.0),
                    rect.width() - 16.0,
                    esc_hint,
                    egui::FontId::new(13.0, egui::FontFamily::Name("rer-bold".into())),
                    crate::editor::theme::TEXT,
                );
            }

            // Ayuda permanente de navegación en modo edición (no en play, donde
            // los controles los define el script). Se pinta abajo para no
            // solaparse con el hint de colocación de entidades.
            if !app.engine_loading && app.engine_error.is_none() && !app.preview_playing {
                let nav_hint = crate::editor::i18n::tr(
                    app.cli.locale,
                    "Clic en la ventana del motor · [WASD] mover · [Espacio] subir · [C] bajar · Botón derecho mirar",
                    "Click the engine window · [WASD] move · [Space] up · [C] down · Right mouse look",
                );
                widgets::viewport_notification(
                    ui,
                    egui::Align2::CENTER_CENTER,
                    egui::pos2(
                        rect.center().x,
                        (rect.max.y - 20.0).min(rect.center().y + rect.height() * 0.38),
                    ),
                    rect.width() - 16.0,
                    nav_hint,
                    // Familia bold registrada en `theme::install_fonts`.
                    egui::FontId::new(13.0, egui::FontFamily::Name("rer-bold".into())),
                    crate::editor::theme::TEXT,
                );
            }

            // Métricas en la esquina superior derecha del viewport, como un HUD
            // de juego. El check de la consola controla su visibilidad.
            if app.show_metrics && !app.engine_loading && app.engine_error.is_none() {
                super::metrics_overlay::show(
                    ui,
                    rect,
                    app.metrics,
                    app.cli.locale,
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
