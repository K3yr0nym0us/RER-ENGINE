use super::*;

pub(super) fn controls_body(
    ui: &mut egui::Ui,
    locale: Locale,
    _hint: &str,
    controls: Option<&ControlSummary>,
) {
    let Some(controls) = controls else {
        widgets::note(
            ui,
            crate::editor::i18n::tr(
                locale,
                "Aún no hay jugador en la escena.",
                "No player in the scene yet.",
            ),
        );
        return;
    };

    widgets::caption(
        ui,
        crate::editor::i18n::tr(locale, "Controles del jugador", "Player controls"),
    );
    if let Some(name) = &controls.player_name {
        widgets::tiny(ui, name);
    }

    widgets::label_value(
        ui,
        crate::editor::i18n::tr(locale, "Teclado + ratón", "Keyboard + mouse"),
        &format_controls(&controls.keyboard, locale),
    );
    widgets::label_value(
        ui,
        crate::editor::i18n::tr(locale, "Mandos", "Gamepads"),
        &format_controls(&controls.gamepad, locale),
    );
    widgets::note(
        ui,
        crate::editor::i18n::tr(
            locale,
            "Editar los bindings requiere el editor de scripts; de momento se hace desde la UI React.",
            "Editing bindings requires the script editor; for now it is done from the React UI.",
        ),
    );
}

pub(super) fn format_controls(keys: &[String], locale: Locale) -> String {
    if keys.is_empty() {
        crate::editor::i18n::tr(locale, "Sin bindings", "No bindings").to_string()
    } else {
        keys.join(", ")
    }
}
