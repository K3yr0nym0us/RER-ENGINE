//! Modal nativa (egui) para confirmar la globalización de un modelo.
//!
//! El sidebar guarda un [`GlobalizeModelDraft`] en
//! `SidebarLeftState::globalize_model`; esta modal explica que el modelo pasará
//! a la biblioteca global (visible en todas las escenas) y, al confirmar, envía
//! `globalize_model` al motor.

use egui::{Color32, RichText};
use serde_json::json;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::theme;
use crate::editor::ui::sidebar_left::GlobalizeModelDraft;

use super::widgets;

/// Dibuja la modal si hay una globalización pendiente. Debe llamarse después de
/// los paneles para que el backdrop cubra toda la ventana.
pub(crate) fn show(ctx: &egui::Context, app: &mut EditorApp) {
    let Some(draft) = app.left_sidebar.globalize_model.take() else {
        return;
    };
    let locale = app.cli.locale;
    let mut confirm = false;
    let mut cancel = false;

    let modal = egui::Modal::new(egui::Id::new("editor_globalize_model_modal"))
        .frame(
            egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
                .fill(theme::SIDEBAR_BG)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ctx, |ui| {
            ui.set_width(380.0);

            ui.label(
                RichText::new(tr(locale, "Globalizar modelo", "Globalize model"))
                    .size(15.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(tr(
                    locale,
                    "El modelo pasará a la biblioteca global: dejará de pertenecer a esta escena y aparecerá en la categoría correspondiente de todas las escenas.",
                    "The model moves to the global library: it will no longer belong to this scene and will appear in the matching category in every scene.",
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
                        RichText::new(tr(locale, "Globalizar", "Globalize")).color(Color32::WHITE),
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
        let GlobalizeModelDraft { path, name } = draft;
        app.send_engine(json!({ "cmd": "globalize_model", "path": path }));
        app.push_log(format!("[Recursos] Modelo globalizado: {name}"), false);
    } else if !cancel {
        // Sigue abierta: devolvemos el borrador.
        app.left_sidebar.globalize_model = Some(draft);
    }
    // Cancelar: se descarta y no se envía nada al motor.
}
