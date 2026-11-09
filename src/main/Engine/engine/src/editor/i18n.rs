//! Textos del editor egui (es/en).

use crate::editor::cli::Locale;

pub struct Text {
    pub window_title: &'static str,
    pub play: &'static str,
    pub stop: &'static str,
    pub debug: &'static str,
    pub save: &'static str,
    pub auto_on: &'static str,
    pub auto_off: &'static str,
    pub saving_project: &'static str,
    pub save_timeout: &'static str,
    pub save_unavailable: &'static str,
    pub save_first: &'static str,
    pub starting: &'static str,
    pub error: &'static str,
    pub empty_log: &'static str,
    pub phase_note: &'static str,
}

pub fn tr(locale: Locale, es: &'static str, en: &'static str) -> &'static str {
    match locale {
        Locale::Es => es,
        Locale::En => en,
    }
}

pub fn text(locale: Locale) -> Text {
    match locale {
        Locale::Es => Text {
            window_title: "RER-ENGINE — Editor (egui)",
            play: "Reproducir",
            stop: "Detener",
            debug: "Depurar",
            save: "Guardar",
            auto_on: "Auto (on)",
            auto_off: "Auto (off)",
            saving_project: "Guardando proyecto…",
            save_timeout: "Guardado cancelado: el motor no entregó el snapshot",
            save_unavailable: "El motor aún no está listo",
            save_first: "Guarda el proyecto primero",
            starting: "Iniciando…",
            error: "Error",
            empty_log: "Sin eventos aún…",
            phase_note: "Sin funcionalidad todavía (Fase 5)",
        },
        Locale::En => Text {
            window_title: "RER-ENGINE — Editor (egui)",
            play: "Play",
            stop: "Stop",
            debug: "Debug",
            save: "Save",
            auto_on: "Auto (on)",
            auto_off: "Auto (off)",
            saving_project: "Saving project…",
            save_timeout: "Save cancelled: the engine did not deliver the snapshot",
            save_unavailable: "The engine is not ready yet",
            save_first: "Save the project first",
            starting: "Starting…",
            error: "Error",
            empty_log: "No events yet…",
            phase_note: "Not wired yet (Phase 5)",
        },
    }
}
