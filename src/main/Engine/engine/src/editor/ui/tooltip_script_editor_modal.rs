//! Editor nativo del script Rhai asignado a un tooltip de interacción.

use egui_code_editor::CodeEditor;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::scene_script_modal::{RER_THEME, rhai_syntax};
use super::widgets;

/// Dibuja la ventana nativa del editor de script del tooltip.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let Some((mut name, mut source)) = app.left_sidebar.tooltip_script_editor.clone() else {
        return;
    };
    let locale = app.cli.locale;
    let syntax = rhai_syntax();
    let mut save = false;

    egui::Frame::new()
        .fill(theme::PANEL_BG)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(520.0);
            ui.label(
                egui::RichText::new(tr(
                    locale,
                    "Editar script del tooltip",
                    "Edit tooltip script",
                ))
                .size(13.0)
                .strong()
                .color(theme::TEXT),
            );
            ui.add_space(8.0);
            ui.add(
                egui::TextEdit::singleline(&mut name)
                    .desired_width(f32::INFINITY)
                    .hint_text(tr(locale, "Nombre del script", "Script name")),
            );
            ui.add_space(8.0);
            CodeEditor::default()
                .id_source("tooltip_script_native_editor")
                .with_rows(22)
                .with_fontsize(13.0)
                .with_theme(RER_THEME)
                .with_numlines(true)
                .desired_width(ui.available_width())
                .show(ui, &mut source, &syntax);
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            save = widgets::modal_button_bold(
                ui,
                egui_phosphor::regular::FLOPPY_DISK,
                &tr(locale, "Guardar script", "Save script"),
                widgets::ModalButtonStyle::Primary,
                ui.available_width(),
                !name.trim().is_empty(),
            );
        });

    if save {
        let draft = &mut app.left_sidebar.tooltip_draft;
        draft.action_ref_name = name.trim().to_string();
        draft.action_ref_source = source;
        app.left_sidebar.tooltip_script_editor = None;
    }
}
