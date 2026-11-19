//! Contenido de la ventana nativa de confirmación de borrado de escena.
//!
//! Sustituye a `DeleteConfirmBody` / `DeleteBlockedBody` del renderer. El sidebar
//! guarda un `DeleteSceneDraft`; si es la única escena del proyecto se muestra
//! solo el aviso, y en caso contrario se pide confirmación antes de enviar
//! `delete_editor_scene` al motor.

use egui::RichText;
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::widgets;

/// Dibuja el cuerpo en la ventana modal externa y aplica la decisión.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let Some(draft) = app.left_sidebar.pending_delete_scene.as_ref() else {
        return;
    };
    let locale = app.cli.locale;
    let id = draft.id;
    let name = draft.name.clone();
    let blocked = draft.blocked;
    let mut confirm = false;

    egui::Frame::new()
        .fill(theme::PANEL_BG)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            if blocked {
                ui.label(
                    RichText::new(tr(
                        locale,
                        "No puedes eliminar esta escena porque es la única del proyecto.",
                        "You cannot delete this scene because it is the only one in the project.",
                    ))
                    .size(13.0)
                    .color(theme::TEXT),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(tr(
                        locale,
                        "Debe existir al menos una escena para mantener el editor en un estado válido.",
                        "There must be at least one scene to keep the editor in a valid state.",
                    ))
                    .size(12.0)
                    .color(theme::TEXT_MUTED),
                );
                return;
            }

            ui.label(
                RichText::new(tr(
                    locale,
                    "Esta acción eliminará la escena seleccionada.",
                    "This action will delete the selected scene.",
                ))
                .size(13.0)
                .color(theme::TEXT),
            );
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
        app.send_engine(json!({ "cmd": "delete_editor_scene", "scene_id": id }));
        app.push_log(format!("[Escenas] Eliminando «{name}»…"), false);
    }
    if confirm {
        app.left_sidebar.pending_delete_scene = None;
    }
}
