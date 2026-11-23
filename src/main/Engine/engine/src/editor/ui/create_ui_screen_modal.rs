//! Contenido de la ventana nativa de creación de pantalla UI (HUD o menú).
//!
//! Sustituye al `ui_create_screen` del renderer. El sidebar prepara el nombre
//! por defecto (`HUD-N` / `UI Menu-N`); esta ventana lo deja editar. El editor
//! nativo genera el id, registra la pantalla y sincroniza el motor sin React.

use egui_phosphor::regular as icons;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::ui_screens::UiScreenEntry;

use super::components::{self as ui_kit, ModalButtonStyle};

/// Dibuja el cuerpo en la ventana modal externa y aplica la creación.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let Some(draft) = app.left_sidebar.pending_create_ui_screen.as_mut() else {
        return;
    };
    let locale = app.cli.locale;
    let scope = draft.scope.clone();
    let default_name = draft.default_name.clone();
    let is_player = scope == "player";
    let mut confirm = false;

    ui_kit::modal_panel(ui, |ui| {
        ui_kit::caption(ui, tr(locale, "Nombre de la pantalla", "Screen name"));
        ui.add_space(4.0);
        ui_kit::text_field(
            ui,
            &mut draft.name,
            tr(locale, "Nombre de la pantalla", "Screen name"),
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
                    tr(locale, "Crear pantalla", "Create screen"),
                    ModalButtonStyle::Primary,
                    140.0,
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
            default_name
        } else {
            trimmed.to_string()
        })
    } else {
        None
    };

    if let Some(name) = submitted {
        let id = app.ui_screens.next_id();
        let list = if is_player {
            &mut app.ui_screens.player
        } else {
            &mut app.ui_screens.menu
        };
        list.push(UiScreenEntry {
            id: id.clone(),
            name: name.clone(),
            active: false,
        });

        if is_player {
            app.sync_player_ui_screens();
            // Abre directamente el editor HUD nativo para la pantalla nueva.
            crate::editor::hud_editor::begin_edit(app, &id);
        }
        app.push_log(
            format!("[Interfaz de usuario] Pantalla «{name}» creada"),
            false,
        );
    }
    if confirm {
        app.left_sidebar.pending_create_ui_screen = None;
    }
}
