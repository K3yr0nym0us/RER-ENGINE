//! Contenido de la ventana nativa de confirmación de borrado de un recurso.
//!
//! El icono de papelera de Recursos marca `SidebarLeftState::pending_remove_asset`;
//! [`super::super::modal_window::ModalWindow`] abre una ventana winit aparte y
//! dibuja este cuerpo. Confirmar envía el comando `remove_*` al motor;
//! cancelar (botón o X de la ventana) no envía nada.

use egui::RichText;
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::editor::app::EditorApp;
use crate::editor::assets::AssetKind;
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::widgets;

/// Dibuja el cuerpo en la ventana modal externa y aplica la decisión.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let Some(draft) = app.left_sidebar.pending_remove_asset.as_ref() else {
        return;
    };
    let locale = app.cli.locale;
    let kind = draft.kind;
    let path = draft.path.clone();
    let name = draft.name.clone();
    let mut confirm = false;

    egui::Frame::new()
        .fill(theme::PANEL_BG)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.label(
                RichText::new(tr(locale, "Eliminar recurso", "Delete asset"))
                    .size(15.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(tr(
                        locale,
                        "¿Seguro que quieres eliminar el recurso",
                        "Are you sure you want to delete the asset",
                    ))
                    .size(13.0)
                    .color(theme::TEXT),
                );
                ui.label(RichText::new(&name).size(13.0).strong().color(theme::TEXT));
                ui.label(RichText::new("?").size(13.0).color(theme::TEXT));
            });
            ui.add_space(6.0);
            ui.label(
                RichText::new(tr(
                    locale,
                    "Esta acción no se puede deshacer.",
                    "This action cannot be undone.",
                ))
                .size(12.0)
                .color(theme::DANGER),
            );
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::modal_button(
                        ui,
                        icons::TRASH,
                        tr(locale, "Sí, eliminar", "Yes, Delete"),
                        widgets::ModalButtonStyle::Danger,
                        140.0,
                        true,
                    ) {
                        confirm = true;
                    }
                });
            });
        });

    if confirm {
        app.send_engine(json!({ "cmd": remove_command(kind), "path": path }));
        app.push_log(format!("[Recursos] Eliminando «{name}»…"), false);
    }
    if confirm {
        app.left_sidebar.pending_remove_asset = None;
    }
}

/// Comando de borrado según el tipo de recurso (antes lo elegía el modal React).
fn remove_command(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Model => "remove_model_asset",
        AssetKind::Sprite => "remove_sprite",
        AssetKind::Sound => "remove_sound",
        AssetKind::Font => "remove_font",
        AssetKind::HudImage => "remove_hud_image",
        AssetKind::Background => "remove_background_asset",
    }
}
