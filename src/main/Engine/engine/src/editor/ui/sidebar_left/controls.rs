use super::*;

pub(super) fn controls_body(
    ui: &mut egui::Ui,
    locale: Locale,
    hint: &str,
    controls: Option<&ControlSummary>,
    actions: &mut Vec<LeftAction>,
) {
    widgets::caption(
        ui,
        crate::editor::i18n::tr(locale, "Controles del jugador", "Player controls"),
    );

    let enabled = controls.is_some();
    if let Some(controls) = controls {
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
    } else {
        widgets::note(
            ui,
            crate::editor::i18n::tr(
                locale,
                "Añade un jugador a la escena para configurar sus controles.",
                "Add a player to the scene to configure its controls.",
            ),
        );
    }

    let width = ui.available_width();
    let keyboard_count = controls.map_or(0, |controls| controls.keyboard.len());
    let gamepad_count = controls.map_or(0, |controls| controls.gamepad.len());
    let keyboard_label = format!(
        "{}  ({keyboard_count})",
        crate::editor::i18n::tr(
            locale,
            "Configurar teclado + ratón",
            "Configure keyboard + mouse"
        )
    );
    let gamepad_label = format!(
        "{}  ({gamepad_count})",
        crate::editor::i18n::tr(locale, "Configurar mandos", "Configure gamepads")
    );

    ui.add_enabled_ui(enabled, |ui| {
        if widgets::wide_button_click_sized(
            ui,
            width,
            icons::KEYBOARD,
            &keyboard_label,
            hint,
            crate::editor::theme::INFO,
        ) {
            actions.push(LeftAction::OpenControlBindings("keyboard_mouse"));
        }
        ui.add_space(2.0);
        if widgets::wide_button_click_sized(
            ui,
            width,
            icons::GAME_CONTROLLER,
            &gamepad_label,
            hint,
            crate::editor::theme::WARNING,
        ) {
            actions.push(LeftAction::OpenControlBindings("gamepad"));
        }
    });

    widgets::note(
        ui,
        crate::editor::i18n::tr(
            locale,
            "Los cambios se guardan en el proyecto y se aplican en modo juego.",
            "Changes are saved with the project and apply in game mode.",
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
