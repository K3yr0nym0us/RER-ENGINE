//! Puente de mensajes con Electron (solo para la UI React antigua durante la
//! migración). El motor se controla dentro de Rust.

use std::io::Write;

use serde_json::json;

/// Escribe una línea JSON en stdout y la vacía inmediatamente.
fn write_line(line: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}

/// Reenvía una línea del motor tal cual para que Electron la entregue a React.
pub fn forward_line(line: &str) {
    if line.trim().is_empty() {
        return;
    }
    write_line(line);
}

/// Avisa a Electron de que la ventana Rust ya está lista.
///
/// Electron oculta su ventana React y la deja viva como host de las ventanas
/// legacy (editor RHAI, editor de nodos).
pub fn notify_editor_ready() {
    write_line(&json!({ "type": "editor_ready" }).to_string());
}

/// Avisa a Electron el idioma del editor para que las modales usen el mismo.
pub fn notify_locale(locale: crate::editor::cli::Locale) {
    write_line(
        &json!({
            "type": "editor_locale",
            "locale": locale.as_str(),
        })
        .to_string(),
    );
}

/// Notifica que el `.save` se escribió correctamente.
pub fn notify_project_saved(path: &std::path::Path) {
    write_line(
        &json!({
            "type": "project_saved",
            "path": path.to_string_lossy(),
        })
        .to_string(),
    );
}

/// Notifica un fallo de guardado con el motivo.
pub fn notify_save_failed(message: &str) {
    write_line(
        &json!({
            "type": "project_save_failed",
            "message": message,
        })
        .to_string(),
    );
}
