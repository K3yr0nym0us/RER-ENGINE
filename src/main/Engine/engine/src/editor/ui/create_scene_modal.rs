//! Contenido de la ventana nativa de creación de escena.
//!
//! Sustituye a `CreateSceneModalBody` del renderer. El sidebar prepara el nombre
//! por defecto (`Scene-NN`); esta ventana deja editarlo y envía
//! `create_editor_scene` con el nombre recortado (o el default si se vacía).

use egui_phosphor::regular as icons;
use serde_json::json;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;

use super::components::{self as ui_kit, ModalButtonStyle};

/// Dibuja el cuerpo en la ventana modal externa y aplica la decisión.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let Some(draft) = app.left_sidebar.pending_create_scene.as_mut() else {
        return;
    };
    let locale = app.cli.locale;
    let mut confirm = false;

    ui_kit::modal_panel(ui, |ui| {
        ui_kit::caption(ui, tr(locale, "Nombre de la escena", "Scene name"));
        ui.add_space(4.0);
        ui_kit::text_field(
            ui,
            &mut draft.name,
            tr(locale, "Nombre de la escena", "Scene name"),
        );
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_create = !draft.name.trim().is_empty();
                if ui_kit::modal_button(
                    ui,
                    icons::PLUS,
                    tr(locale, "Crear escena", "Create scene"),
                    ModalButtonStyle::Primary,
                    130.0,
                    can_create,
                ) {
                    confirm = true;
                }
            });
        });
    });

    let submitted = if confirm {
        let trimmed = draft.name.trim();
        Some(if trimmed.is_empty() {
            draft.default_name.clone()
        } else {
            trimmed.to_string()
        })
    } else {
        None
    };

    if let Some(name) = submitted {
        app.send_engine(json!({ "cmd": "create_editor_scene", "name": name }));
        app.push_log(format!("[Escenas] Creando «{name}»…"), false);
    }
    if confirm {
        app.left_sidebar.pending_create_scene = None;
    }
}
