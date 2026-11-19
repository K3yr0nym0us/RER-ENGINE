//! Contenido de la ventana nativa del script Rhai de escena.
//!
//! Sustituye al `SceneScriptEditorModalBody` del renderer. Usa `egui_code_editor`
//! con resaltado de sintaxis y números de línea. El código se lee del último
//! `project_state` recibido; al guardar el editor lo persiste en `scene_scripts`
//! y envía `load_scene_visual_script` al motor directamente.

use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::editor::app::EditorApp;
use crate::editor::cli::Locale;
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::widgets;

/// Tema del editor alineado con la paleta del editor nativo.
pub(crate) const RER_THEME: ColorTheme = ColorTheme {
    name: "RER",
    dark: true,
    bg: "1f2237",
    cursor: "38bdf8",
    selection: "2c3152",
    comments: "6c757d",
    functions: "4ade80",
    keywords: "38bdf8",
    literals: "facc15",
    numerics: "c792ea",
    punctuation: "c5c9e8",
    strs: "4ade80",
    types: "0dcaf0",
    special: "ffc107",
};

/// Palabras clave de Rhai, las mismas que el tokenizer Monaco del renderer.
pub(crate) fn rhai_syntax() -> Syntax {
    Syntax::new("rhai")
        .with_comment("//")
        .with_comment_multiline(["/*", "*/"])
        .with_keywords([
            "fn", "let", "const", "if", "else", "switch", "case", "default", "loop", "while", "do",
            "for", "in", "break", "continue", "return", "throw", "try", "catch", "true", "false",
            "pub", "private", "use", "import", "as", "go_to", "print", "debug", "eval", "is",
            "match",
        ])
        .with_special([
            "engine",
            "on_scene_start",
            "on_scene_tick",
            "log",
            "entity",
            "scene",
            "vector",
            "rand",
            "sin",
            "cos",
            "time",
        ])
}

/// Script inicial con comentarios y callbacks, espejo de `getDefaultSceneScript`.
fn default_script(locale: Locale) -> &'static str {
    match locale {
        Locale::Es => {
            "// Script Rhai de escena (Level Blueprint)\n\
// Callbacks disponibles:\n\
//   on_scene_start()  → al iniciar play en esta escena\n\
//   on_scene_tick(dt) → cada frame mientras la escena está en play\n\
//\n\
// API: engine.log(msg)\n\
// ─────────────────────────────────────────────────────────────────────────────\n\
\n\
fn on_scene_start() {\n\
    engine.log(\"Escena iniciada\");\n\
}\n\
\n\
fn on_scene_tick(dt) {\n\
}\n"
        }
        Locale::En => {
            "// Scene Rhai script (Level Blueprint)\n\
// Available callbacks:\n\
//   on_scene_start()  → when play starts in this scene\n\
//   on_scene_tick(dt) → each frame while the scene is in play\n\
//\n\
// API: engine.log(msg)\n\
// ─────────────────────────────────────────────────────────────────────────────\n\
\n\
fn on_scene_start() {\n\
    engine.log(\"Scene started\");\n\
}\n\
\n\
fn on_scene_tick(dt) {\n\
}\n"
        }
    }
}

/// Dibuja el editor en la ventana modal externa.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let Some(scene_id) = app.left_sidebar.scene_script_open else {
        return;
    };

    let locale = app.cli.locale;

    // El estado de proyecto puede llegar después de abrir la ventana. Si la
    // escena no tiene script, se usa el mismo plantillo con comentarios que
    // tenía la modal Monaco (getDefaultSceneScript).
    if app.left_sidebar.scene_script_source.is_none() {
        let stored = app
            .project_state
            .scenes
            .iter()
            .find(|scene| scene.get("id").and_then(serde_json::Value::as_u64) == Some(scene_id))
            .and_then(|scene| scene.get("sceneScriptRhai"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let source = if stored.trim().is_empty() {
            default_script(locale).to_string()
        } else {
            stored.to_string()
        };
        app.left_sidebar.scene_script_source = Some(source);
    }

    let mut source = app
        .left_sidebar
        .scene_script_source
        .clone()
        .unwrap_or_default();
    let mut save = false;
    let syntax = rhai_syntax();

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL_BG)
                .inner_margin(egui::Margin::same(12)),
        )
        .show(ui, |ui| {
            // El original no tiene cabecera dentro del cuerpo: el titulo va en
            // la barra de la ventana. El editor ocupa todo el espacio menos el
            // boton, con scroll interno en vez de crecer.
            let editor_width = ui.available_width();
            let editor_height = (ui.available_height() - 46.0).max(240.0);
            egui::ScrollArea::vertical()
                .id_salt("scene_script_scroll")
                .max_height(editor_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    CodeEditor::default()
                        .id_source("scene_script_editor")
                        .with_rows(20)
                        .with_fontsize(13.0)
                        .with_theme(RER_THEME)
                        .with_numlines(true)
                        .vscroll(false)
                        .desired_width(editor_width)
                        .show(ui, &mut source, &syntax);
                });

            ui.add_space(8.0);
            if widgets::modal_button(
                ui,
                icons::FLOPPY_DISK,
                tr(locale, "Guardar script", "Save script"),
                widgets::ModalButtonStyle::Primary,
                120.0,
                true,
            ) {
                save = true;
            }
        });

    app.left_sidebar.scene_script_source = Some(source.clone());
    if save {
        // El editor guarda el script y lo aplica en el motor sin pasar por React.
        app.scene_scripts.insert(scene_id, source.clone());
        app.send_engine(json!({
            "cmd": "load_scene_visual_script",
            "scene_id": scene_id,
            "source": source,
        }));
        app.left_sidebar.scene_script_open = None;
        app.left_sidebar.scene_script_source = None;
    }
}
