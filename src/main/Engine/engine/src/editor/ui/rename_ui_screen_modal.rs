//! Contenido de la ventana nativa de renombrado de menú UI.
//!
//! Sustituye al `ModalSetNameUi` que se abría desde `ui_edit_screen` con
//! `scope: menu`. El editor nativo posee el catálogo: actualiza el nombre y lo
//! incluye en el `.save`; no intervienen React ni Electron.

use egui::RichText;
use egui_phosphor::regular as icons;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::widgets;

/// Dibuja el cuerpo en la ventana modal externa y aplica el renombrado.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let Some(draft) = app.left_sidebar.pending_rename_ui_screen.as_mut() else {
        return;
    };
    let locale = app.cli.locale;
    let id = draft.id.clone();
    let mut confirm = false;

    egui::Frame::new()
        .fill(theme::PANEL_BG)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.label(
                RichText::new(tr(locale, "Nuevo nombre", "New name"))
                    .size(12.0)
                    .strong()
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::singleline(&mut draft.name)
                    .desired_width(f32::INFINITY)
                    .hint_text(tr(locale, "Nombre del menú", "Menu name")),
            );
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can_rename = !draft.name.trim().is_empty();
                    if widgets::modal_button(
                        ui,
                        icons::CHECK,
                        tr(locale, "Renombrar", "Rename"),
                        widgets::ModalButtonStyle::Primary,
                        130.0,
                        can_rename,
                    ) {
                        confirm = true;
                    }
                });
            });
        });

    let submitted = if confirm {
        let trimmed = draft.name.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    } else {
        None
    };

    if let Some(name) = submitted {
        if let Some(screen) = app
            .ui_screens
            .menu
            .iter_mut()
            .find(|screen| screen.id == id)
        {
            screen.name = name.clone();
            app.push_log(
                format!("[Interfaz de usuario] Menú renombrado a «{name}»"),
                false,
            );
        }
    }
    if confirm {
        app.left_sidebar.pending_rename_ui_screen = None;
    }
}
