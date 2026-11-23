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

use super::components::{self as ui_kit, ModalButtonStyle};

/// Dibuja el cuerpo en la ventana modal externa y registra la acción elegida.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    if !app.exit_confirm_open {
        return;
    }
    let locale = app.cli.locale;
    let mut action = None;

    ui_kit::modal_panel(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(icons::WARNING)
                    .size(40.0)
                    .color(theme::WARNING),
            );
            ui.add_space(8.0);
            ui_kit::modal_title(ui, tr(locale, "Salir de RER-ENGINE", "Exit RER-ENGINE"));
            ui.add_space(8.0);
            ui_kit::body_text(
                ui,
                tr(
                    locale,
                    "¿Quieres guardar tu progreso antes de salir?",
                    "Do you want to save your progress before exiting?",
                ),
            );
            ui.add_space(4.0);
            ui_kit::note(
                ui,
                tr(
                    locale,
                    "Los cambios sin guardar se perderán.",
                    "Unsaved changes will be lost.",
                ),
            );
        });
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(8.0);

        // Igual que la modal React: tres botones apilados a lo ancho.
        let width = ui.available_width();
        if ui_kit::modal_button(
            ui,
            icons::FLOPPY_DISK,
            tr(locale, "Guardar y salir", "Save and exit"),
            ModalButtonStyle::Primary,
            width,
            true,
        ) {
            action = Some(ExitAction::Save);
        }
        ui.add_space(6.0);
        if ui_kit::modal_button(
            ui,
            icons::SIGN_OUT,
            tr(locale, "Salir sin guardar", "Exit without saving"),
            ModalButtonStyle::Danger,
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
