//! Contenido de la ventana nativa de confirmación de borrado de escena.
//!
//! Sustituye a `DeleteConfirmBody` / `DeleteBlockedBody` del renderer. El sidebar
//! guarda un `DeleteSceneDraft`; si es la única escena del proyecto se muestra
//! solo el aviso, y en caso contrario se pide confirmación antes de enviar
//! `delete_editor_scene` al motor.

use egui_phosphor::regular as icons;
use serde_json::json;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;

use super::components::{self as ui_kit, ModalButtonStyle};

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

    ui_kit::modal_panel(ui, |ui| {
        if blocked {
            ui_kit::body_text(
                ui,
                tr(
                    locale,
                    "No puedes eliminar esta escena porque es la única del proyecto.",
                    "You cannot delete this scene because it is the only one in the project.",
                ),
            );
            ui.add_space(6.0);
            ui_kit::note(
                ui,
                tr(
                    locale,
                    "Debe existir al menos una escena para mantener el editor en un estado válido.",
                    "There must be at least one scene to keep the editor in a valid state.",
                ),
            );
            return;
        }

        ui_kit::body_text(
            ui,
            tr(
                locale,
                "Esta acción eliminará la escena seleccionada.",
                "This action will delete the selected scene.",
            ),
        );
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui_kit::modal_button(
                    ui,
                    icons::TRASH,
                    tr(locale, "Eliminar", "Delete"),
                    ModalButtonStyle::Danger,
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
