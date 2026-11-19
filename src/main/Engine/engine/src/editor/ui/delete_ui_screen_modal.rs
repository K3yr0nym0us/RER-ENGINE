//! Contenido de la ventana nativa de confirmación de borrado de pantalla UI.
//!
//! Sustituye al `ModalConfirmBody` que se abría desde `ui_delete_screen`. El
//! editor nativo es la fuente de verdad del catálogo de pantallas: al confirmar
//! borra la entrada y sincroniza el motor sin pasar por React.

use egui::RichText;
use egui_phosphor::regular as icons;
use serde_json::{Value, json};

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::widgets;

/// Dibuja el cuerpo en la ventana modal externa y aplica la decisión.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let Some(draft) = app.left_sidebar.pending_delete_ui_screen.as_ref() else {
        return;
    };
    let locale = app.cli.locale;
    let scope = draft.scope.clone();
    let id = draft.id.clone();
    let name = draft.name.clone();
    let mut confirm = false;

    egui::Frame::new()
        .fill(theme::PANEL_BG)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(tr(
                        locale,
                        "¿Seguro que quieres eliminar la pantalla de UI",
                        "Are you sure you want to delete the UI screen",
                    ))
                    .size(13.0)
                    .color(theme::TEXT),
                );
                ui.label(RichText::new(&name).size(13.0).strong().color(theme::TEXT));
            });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::modal_button(
                        ui,
                        icons::TRASH,
                        tr(locale, "Eliminar", "Delete"),
                        widgets::ModalButtonStyle::Danger,
                        110.0,
                        true,
                    ) {
                        confirm = true;
                    }
                });
            });
        });

    if confirm {
        // El editor nativo es la fuente de verdad: borra la pantalla y
        // sincroniza el catálogo con el motor directamente.
        {
            let list = if scope == "menu" {
                &mut app.ui_screens.menu
            } else {
                &mut app.ui_screens.player
            };
            list.retain(|screen| screen.id != id);
        }
        if scope == "player" {
            app.sync_player_ui_screens();
            if !app.ui_screens.player.iter().any(|screen| screen.active) {
                app.send_engine(json!({
                    "cmd": "set_active_player_ui_screen",
                    "screen_id": Value::Null,
                }));
            }
        }
        app.left_sidebar.pending_delete_ui_screen = None;
    }
}
