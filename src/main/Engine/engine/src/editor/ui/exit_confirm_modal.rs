//! Contenido de la ventana nativa de confirmación de salida del editor.
//!
//! Sustituye a la ventana Electron `ExitConfirmBody`: al cerrar la ventana winit
//! se marca `EditorApp::exit_confirm_open`, se abre una ventana aparte con este
//! cuerpo y la acción elegida se deja en `pending_exit_action` para que el host
//! la aplique (guardar, salir sin guardar o cancelar).

use egui::RichText;
use egui_phosphor::regular as icons;

use crate::editor::app::{EditorApp, ExitAction};
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::widgets;

/// Dibuja el cuerpo en la ventana modal externa y registra la acción elegida.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    if !app.exit_confirm_open {
        return;
    }
    let locale = app.cli.locale;
    let mut action = None;

    egui::Frame::new()
        .fill(theme::PANEL_BG)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(icons::WARNING)
                        .size(40.0)
                        .color(theme::WARNING),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(tr(locale, "Salir de RER-ENGINE", "Exit RER-ENGINE"))
                        .size(15.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(tr(
                        locale,
                        "¿Quieres guardar tu progreso antes de salir?",
                        "Do you want to save your progress before exiting?",
                    ))
                    .size(13.0)
                    .color(theme::TEXT),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(tr(
                        locale,
                        "Los cambios sin guardar se perderán.",
                        "Unsaved changes will be lost.",
                    ))
                    .size(12.0)
                    .color(theme::TEXT_MUTED),
                );
            });
            ui.add_space(14.0);
            ui.separator();
            ui.add_space(8.0);

            // Igual que la modal React: tres botones apilados a lo ancho.
            let width = ui.available_width();
            if widgets::modal_button(
                ui,
                icons::FLOPPY_DISK,
                tr(locale, "Guardar y salir", "Save and exit"),
                widgets::ModalButtonStyle::Primary,
                width,
                true,
            ) {
                action = Some(ExitAction::Save);
            }
            ui.add_space(6.0);
            if widgets::modal_button(
                ui,
                icons::SIGN_OUT,
                tr(locale, "Salir sin guardar", "Exit without saving"),
                widgets::ModalButtonStyle::Danger,
                width,
                true,
            ) {
                action = Some(ExitAction::Discard);
            }
        });

    if let Some(action) = action {
        app.exit_confirm_open = false;
        app.pending_exit_action = Some(action);
    }
}
