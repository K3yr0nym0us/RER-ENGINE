//! Confirmación nativa de quitar el script/nodo asignado a un tooltip.

use egui::{RichText, TextStyle};
use egui_phosphor::regular as icons;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::widgets;

/// Dibuja la ventana nativa de confirmación y aplica la decisión.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    let name = app.left_sidebar.tooltip_draft.action_ref_name.clone();
    let mut confirm = false;

    egui::Frame::new()
        .fill(theme::PANEL_BG)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(tr(
                        locale,
                        "¿Seguro que quieres quitar el script",
                        "Are you sure you want to remove the script",
                    ))
                    .color(theme::TEXT),
                );
                ui.label(
                    RichText::new(&name)
                        .color(theme::TEXT)
                        .text_style(TextStyle::Body),
                );
                ui.label(RichText::new("?").color(theme::TEXT));
            });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);
            if widgets::modal_button(
                ui,
                icons::TRASH,
                tr(locale, "Quitar", "Remove"),
                widgets::ModalButtonStyle::Danger,
                ui.available_width(),
                true,
            ) {
                confirm = true;
            }
        });

    if confirm {
        let draft = &mut app.left_sidebar.tooltip_draft;
        draft.action_ref_name.clear();
        draft.action_ref_source.clear();
        draft.action_ref_graph = serde_json::Value::Null;
        app.left_sidebar.pending_remove_tooltip_script = false;
    }
}
