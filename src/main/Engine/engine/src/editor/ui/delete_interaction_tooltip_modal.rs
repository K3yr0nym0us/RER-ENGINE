//! Confirmación nativa de borrado de un tooltip con nombre.

use egui::RichText;
use egui_phosphor::regular as icons;
use serde_json::Value;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::widgets;

pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let Some(draft) = app.left_sidebar.pending_delete_interaction_tooltip.as_ref() else {
        return;
    };
    let locale = app.cli.locale;
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
                        "¿Seguro que quieres eliminar el tooltip",
                        "Are you sure you want to delete the tooltip",
                    ))
                    .size(13.0)
                    .color(theme::TEXT),
                );
                ui.label(RichText::new(&name).size(13.0).strong().color(theme::TEXT));
            });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);
            if widgets::modal_button(
                ui,
                icons::TRASH,
                tr(locale, "Eliminar", "Delete"),
                widgets::ModalButtonStyle::Danger,
                ui.available_width(),
                true,
            ) {
                confirm = true;
            }
        });

    if confirm {
        app.project_state
            .interaction_tooltips
            .retain(|item| item.get("id").and_then(Value::as_str) != Some(id.as_str()));
        if app.left_sidebar.tooltip_draft.id == id {
            app.left_sidebar.tooltip_draft = Default::default();
        }
        app.send_engine(crate::editor::ui::sidebar_left::sync_command(app));
        app.left_sidebar.pending_delete_interaction_tooltip = None;
        app.push_log("[Tooltip] Eliminado", false);
    }
}
