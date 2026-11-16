//! Modal nativa (egui) para confirmar «Limpiar escena».
//!
//! El botón de Escenas (visible solo mientras queden elementos DEMO) marca
//! `SidebarLeftState::pending_clear_scene`. Confirmar envía `clear_scene_content`:
//! elimina los elementos DEMO (`[EditorBox]`, `[Ball]`, `[ReflectionProbe]`,
//! `[MatVal]`, `[MatComparison]`…) y conserva suelo, sol, jugador y cámaras.

use egui::{Color32, RichText};
use serde_json::json;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::widgets;

/// Dibuja la modal si hay una limpieza pendiente. Debe llamarse después de los
/// paneles para que el backdrop cubra toda la ventana.
pub(crate) fn show(ctx: &egui::Context, app: &mut EditorApp) {
    if !app.left_sidebar.pending_clear_scene {
        return;
    }
    let locale = app.cli.locale;
    let mut confirm = false;
    let mut cancel = false;

    let modal = egui::Modal::new(egui::Id::new("editor_clear_scene_modal"))
        .frame(
            egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
                .fill(theme::SIDEBAR_BG)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ctx, |ui| {
            ui.set_width(380.0);

            ui.label(
                RichText::new(tr(locale, "Limpiar escena", "Clear scene"))
                    .size(15.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(tr(
                    locale,
                    "Se eliminarán todos los elementos DEMO de la escena (muros, cubos, esfera de física, sondas y las pruebas de materiales: esferas, alfombras y letreros). El suelo, el sol, el jugador y las cámaras se conservan.",
                    "All DEMO elements in the scene will be removed (walls, cubes, physics ball, probes and the material tests: spheres, rugs and labels). The ground, sun, player and cameras are kept.",
                ))
                .size(13.0)
                .color(theme::TEXT),
            );
            ui.add_space(10.0);
            widgets::tiny(
                ui,
                tr(
                    locale,
                    "Se puede deshacer con Ctrl+Z entidad por entidad.",
                    "Undo with Ctrl+Z restores the entities one by one.",
                ),
            );
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let confirm_button = egui::Button::new(
                        RichText::new(tr(locale, "Limpiar", "Clear")).color(Color32::WHITE),
                    )
                    .fill(theme::DANGER)
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
        app.left_sidebar.pending_clear_scene = false;
        app.send_engine(json!({ "cmd": "clear_scene_content" }));
        app.push_log(
            "[Escenas] Limpiando elementos DEMO de la escena…".to_string(),
            false,
        );
    } else if cancel {
        app.left_sidebar.pending_clear_scene = false;
    }
}
