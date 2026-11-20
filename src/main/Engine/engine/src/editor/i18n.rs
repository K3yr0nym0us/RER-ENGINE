//! Textos del editor egui (es/en).

use crate::editor::cli::Locale;

pub struct Text {
    pub window_title: &'static str,
    pub loading_engine: &'static str,
    pub engine_error: &'static str,
    pub play_game: &'static str,
    pub debug: &'static str,
    pub debug_hint: &'static str,
    pub save: &'static str,
    pub save_project: &'static str,
    pub auto_on: &'static str,
    pub auto_off: &'static str,
    pub saving_project: &'static str,
    pub save_timeout: &'static str,
    pub save_unavailable: &'static str,
    pub save_first: &'static str,
    pub empty_log: &'static str,
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
            loading_engine: "Cargando motor…",
            engine_error: "No se pudo iniciar el motor",
            play_game: "Jugar partida",
            debug: "Depurar",
            debug_hint: "Muestra colisiones y ayudas de edición, también en juego",
            save: "Guardar",
            save_project: "Guardar proyecto",
            auto_on: "Auto (on)",
            auto_off: "Auto (off)",
            saving_project: "Guardando proyecto…",
            save_timeout: "Guardado cancelado: el motor no entregó el snapshot",
            save_unavailable: "El motor aún no está listo",
            save_first: "Guarda primero para activar el auto guardado",
            empty_log: "Sin eventos aún…",
        },
        Locale::En => Text {
            window_title: "RER-ENGINE — Editor (egui)",
            loading_engine: "Loading engine…",
            engine_error: "The engine could not start",
            play_game: "Play game",
            debug: "Debug",
            debug_hint: "Show colliders and editor helpers, also during play",
            save: "Save",
            save_project: "Save project",
            auto_on: "Auto (on)",
            auto_off: "Auto (off)",
            saving_project: "Saving project…",
            save_timeout: "Save cancelled: the engine did not deliver the snapshot",
            save_unavailable: "The engine is not ready yet",
            save_first: "Save first to enable autosave",
            empty_log: "No events yet…",
        },
    }
}
