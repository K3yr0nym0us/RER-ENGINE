use super::*;

pub(super) fn lighting_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::hint(
        ui,
        crate::editor::i18n::tr(
            locale,
            "Mueve el gizmo del sol en el viewport. Estos valores ajustan el brillo.",
            "Move the sun gizmo in the viewport. These values tune brightness.",
        ),
    );

    widgets::label_value(
        ui,
        crate::editor::i18n::tr(locale, "Luz ambiental", "Ambient light"),
        &format!("{:.2}", world.light_ambient),
    );
    let mut ambient = world.light_ambient;
    if widgets::range_slider(
        ui,
        &mut ambient,
        0.0,
        0.45,
        0.01,
        crate::editor::theme::CONTROL_BLUE,
    ) {
        actions.push(WorldAction::SetLight {
            ambient: Some(ambient),
            intensity: None,
            shadow_darkness: None,
        });
    }
    ui.add_space(2.0);

    widgets::label_value(
        ui,
        crate::editor::i18n::tr(locale, "Intensidad de luz", "Light intensity"),
        &format!("{:.2}", world.light_intensity),
    );
    let mut intensity = world.light_intensity;
    if widgets::range_slider(
        ui,
        &mut intensity,
        0.2,
        2.5,
        0.05,
        crate::editor::theme::CONTROL_BLUE,
    ) {
        actions.push(WorldAction::SetLight {
            ambient: None,
            intensity: Some(intensity),
            shadow_darkness: None,
        });
    }
}
