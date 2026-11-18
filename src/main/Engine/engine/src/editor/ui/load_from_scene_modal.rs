//! Modal nativa (egui) «Cargar desde otra escena».
//!
//! Lista SOLO los modelos que pertenecen a otras escenas. Al elegir uno se abre
//! la confirmación de globalización (`globalize_model_modal`): el recurso deja
//! de duplicarse por escena y pasa a la biblioteca global, disponible en todas.

use egui::RichText;
use egui_phosphor::regular as icons;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::theme;
use crate::editor::ui::sidebar_left::GlobalizeModelDraft;

use super::widgets;

/// Dibuja la modal si está abierta. Debe llamarse después de los paneles.
pub(crate) fn show(ctx: &egui::Context, app: &mut EditorApp) {
    if !app.left_sidebar.load_from_scene_open {
        return;
    }
    let locale = app.cli.locale;
    let active_scene_id = app.scenes.active_scene_id;
    let mut close = false;
    let mut picked: Option<GlobalizeModelDraft> = None;

    // Modelos por escena que NO son la activa (ya están cargados) y están listos.
    let mut scene_groups: Vec<(u32, Vec<(&str, &str)>)> = Vec::new();
    let mut has_scene_models = false;
    for model in &app.assets.models {
        let Some(scene_id) = model.scene_id else {
            continue;
        };
        if model.state.as_deref() == Some("importing") || model.state.as_deref() == Some("failed") {
            continue;
        }
        has_scene_models = true;
        if u64::from(scene_id) == active_scene_id || model.model_id.is_none() {
            continue;
        }
        let Some(model_id) = model.model_id.as_deref() else {
            continue;
        };
        let entry = (model.name.as_str(), model_id);
        match scene_groups.iter_mut().find(|(id, _)| *id == scene_id) {
            Some((_, list)) => list.push(entry),
            None => scene_groups.push((scene_id, vec![entry])),
        }
    }
    scene_groups.sort_by_key(|(scene_id, _)| *scene_id);

    let modal = egui::Modal::new(egui::Id::new("editor_load_from_scene_modal"))
        .frame(
            egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
                .fill(theme::SIDEBAR_BG)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ctx, |ui| {
            ui.set_width(380.0);

            ui.label(
                RichText::new(tr(locale, "Cargar desde otra escena", "Load from another scene"))
                    .size(15.0)
                    .strong()
                    .color(theme::TEXT),
            );
            widgets::hint(
                ui,
                tr(
                    locale,
                    "Elige un modelo de otra escena: pasará a ser global y podrá usarse en todas.",
                    "Pick a model from another scene: it becomes global and usable in every scene.",
                ),
            );
            ui.add_space(10.0);

            if scene_groups.is_empty() {
                widgets::note(
                    ui,
                    if has_scene_models {
                        tr(
                            locale,
                            "Solo la escena actual tiene modelos por escena.",
                            "Only the current scene has per-scene models.",
                        )
                    } else {
                        tr(
                            locale,
                            "Aún no hay recursos por escena. Guarda el proyecto para escribir los recursos de escena en el .save.",
                            "No per-scene resources found yet. Save the project to write scene resources into the .save.",
                        )
                    },
                );
            } else {
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for (scene_id, models) in &scene_groups {
                            let scene_name = app
                                .scenes
                                .scenes
                                .iter()
                                .find(|scene| scene.id == u64::from(*scene_id))
                                .map(|scene| scene.name.clone())
                                .unwrap_or_else(|| format!("Scene {scene_id}"));
                            ui.add_space(4.0);
                            widgets::caption(ui, &scene_name);
                            for (name, model_id) in models {
                                if ui
                                    .selectable_label(
                                        false,
                                        RichText::new(*name).size(13.0).color(theme::TEXT),
                                    )
                                    .clicked()
                                {
                                    picked = Some(GlobalizeModelDraft {
                                        path: (*model_id).to_string(),
                                        name: (*name).to_string(),
                                    });
                                }
                            }
                        }
                    });
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::modal_button(
                        ui,
                        icons::X,
                        tr(locale, "Cerrar", "Close"),
                        widgets::ModalButtonStyle::Secondary,
                        100.0,
                        true,
                    ) {
                        close = true;
                    }
                });
            });
        });

    if modal.should_close() {
        close = true;
    }

    if let Some(draft) = picked {
        app.left_sidebar.load_from_scene_open = false;
        // Unificación con «Globalizar modelo»: misma confirmación y comando.
        app.left_sidebar.globalize_model = Some(draft);
    } else if close {
        app.left_sidebar.load_from_scene_open = false;
    }
}
