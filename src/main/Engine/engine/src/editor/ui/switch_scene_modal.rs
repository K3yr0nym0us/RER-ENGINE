//! Modal nativa (egui) para confirmar el cambio de escena.
//!
//! El sidebar guarda un [`SceneSwitchDraft`] en
//! `SidebarLeftState::pending_scene_switch` al pulsar el icono de cargar escena.
//! Confirmar conserva el estado de la escena actual y carga la destino; cancelar
//! no envía nada al motor.

use egui::{Color32, RichText};
use serde_json::json;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::theme;
use crate::editor::ui::sidebar_left::SceneSwitchDraft;

use super::widgets;

/// Dibuja la modal si hay un cambio de escena pendiente. Debe llamarse después
/// de los paneles para que el backdrop cubra toda la ventana.
pub(crate) fn show(ctx: &egui::Context, app: &mut EditorApp) {
    let Some(draft) = app.left_sidebar.pending_scene_switch.take() else {
        return;
    };
    let locale = app.cli.locale;
    let mut confirm = false;
    let mut cancel = false;

    let modal = egui::Modal::new(egui::Id::new("editor_switch_scene_modal"))
        .frame(
            egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
                .fill(theme::SIDEBAR_BG)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ctx, |ui| {
            ui.set_width(380.0);

            ui.label(
                RichText::new(tr(locale, "Cambiar de escena", "Switch scene"))
                    .size(15.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(tr(
                    locale,
                    "Se conservará el estado de la escena actual y se cargará la escena destino.",
                    "The current scene state is kept and the target scene is loaded.",
                ))
                .size(13.0)
                .color(theme::TEXT),
            );
            ui.add_space(8.0);
            widgets::tiny(ui, &draft.name);
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let confirm_button = egui::Button::new(
                        RichText::new(tr(locale, "Cambiar", "Switch")).color(Color32::WHITE),
                    )
                    .fill(theme::ACCENT)
                    .min_size(egui::vec2(110.0, 28.0));
                    if ui.add(confirm_button).clicked() {
                        confirm = true;
                    }

                    let cancel_button = egui::Button::new(
                        RichText::new(tr(locale, "Cancelar", "Cancel")).color(theme::TEXT_MUTED),
                    )
                    .min_size(egui::vec2(100.0, 28.0));
                    if ui.add(cancel_button).clicked() {
                        cancel = true;
                    }
                });
            });
        });

    // Clic fuera de la modal o Esc.
    if modal.should_close() {
        cancel = true;
    }

    if confirm {
        let SceneSwitchDraft { id, name } = draft;
        app.scenes.begin_switch(id);
        app.send_engine(json!({ "cmd": "switch_editor_scene", "scene_id": id }));
        app.push_log(format!("[Escenas] Cargando «{name}»…"), false);
    } else if !cancel {
        // Sigue abierta: devolvemos el borrador.
        app.left_sidebar.pending_scene_switch = Some(draft);
    }
    // Cancelar: no se envía nada al motor.
}
