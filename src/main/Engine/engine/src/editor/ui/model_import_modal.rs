//! Modal nativa (egui) para categorizar un modelo 3D al importarlo.
//!
//! El sidebar abre el diálogo de archivos y guarda un [`ModelImportDraft`] en
//! `SidebarLeftState::model_import`; esta modal pide nombre y categoría y, al
//! confirmar, envía `load_model_asset` con `category`. Sin categoría el motor
//! registra el modelo con `category: None` y no aparece en los selectores
//! filtrados por categoría (p. ej. «cambiar modelo» de un personaje).

use egui::{Color32, RichText};
use serde_json::json;

use crate::editor::app::EditorApp;
use crate::editor::cli::Locale;
use crate::editor::i18n::tr;
use crate::editor::theme;
use crate::editor::ui::sidebar_left::ModelImportDraft;

use super::widgets;

/// Categorías válidas de la biblioteca de modelos. Deben coincidir con
/// `normalize_model_library_category` (`engine3d::ipc::types`).
pub(crate) const MODEL_CATEGORIES: [&str; 5] =
    ["character", "environment", "object", "weapon", "projectile"];

fn category_label(locale: Locale, category: &str) -> &'static str {
    match category {
        "character" => tr(locale, "Personaje", "Character"),
        "environment" => tr(locale, "Entorno", "Environment"),
        "weapon" => tr(locale, "Arma", "Weapon"),
        "projectile" => tr(locale, "Proyectil", "Projectile"),
        _ => tr(locale, "Objeto", "Object"),
    }
}

/// Dibuja la modal si hay una importación pendiente. Debe llamarse después de
/// los paneles para que el backdrop cubra toda la ventana.
pub(crate) fn show(ctx: &egui::Context, app: &mut EditorApp) {
    let Some(mut draft) = app.left_sidebar.model_import.take() else {
        return;
    };
    let locale = app.cli.locale;
    let mut confirm = false;
    let mut cancel = false;

    let modal = egui::Modal::new(egui::Id::new("editor_model_import_modal"))
        .frame(
            egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
                .fill(theme::SIDEBAR_BG)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ctx, |ui| {
            ui.set_width(360.0);

            ui.label(
                RichText::new(tr(locale, "Importar modelo 3D", "Import 3D model"))
                    .size(15.0)
                    .strong()
                    .color(theme::TEXT),
            );
            widgets::hint(
                ui,
                tr(
                    locale,
                    "Elige la categoría para que el modelo aparezca en los selectores correctos.",
                    "Choose a category so the model shows up in the right pickers.",
                ),
            );
            ui.add_space(10.0);

            widgets::caption(ui, tr(locale, "Nombre", "Name"));
            ui.add(
                egui::TextEdit::singleline(&mut draft.name)
                    .desired_width(f32::INFINITY)
                    .hint_text(tr(locale, "Nombre del modelo", "Model name")),
            );
            ui.add_space(10.0);

            widgets::caption(ui, tr(locale, "Categoría", "Category"));
            for category in MODEL_CATEGORIES {
                ui.radio_value(
                    &mut draft.category,
                    category,
                    category_label(locale, category),
                );
            }

            ui.add_space(8.0);
            widgets::tiny(ui, &draft.path);
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let import = egui::Button::new(
                        RichText::new(tr(locale, "Importar", "Import")).color(Color32::WHITE),
                    )
                    .fill(theme::ACCENT)
                    .min_size(egui::vec2(100.0, 28.0));
                    let can_import = !draft.name.trim().is_empty();
                    if ui.add_enabled(can_import, import).clicked() {
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
        let name = draft.name.trim().to_string();
        let ModelImportDraft { path, category, .. } = draft;
        app.send_engine(json!({
            "cmd": "load_model_asset",
            "path": path,
            "name": name,
            "category": category,
        }));
        app.push_log(
            format!("[Recursos] Importando «{name}» ({category})…"),
            false,
        );
    } else if !cancel {
        // Sigue abierta: devolvemos el borrador con los cambios del frame.
        app.left_sidebar.model_import = Some(draft);
    }
    // Cancelar: el borrador se descarta y no se envía nada al motor.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine3d::ipc::normalize_model_library_category;

    #[test]
    fn categories_are_accepted_by_the_engine() {
        for category in MODEL_CATEGORIES {
            assert_eq!(
                normalize_model_library_category(Some(category)).as_deref(),
                Some(category),
                "la categoría «{category}» no la normaliza el motor"
            );
        }
    }
}
