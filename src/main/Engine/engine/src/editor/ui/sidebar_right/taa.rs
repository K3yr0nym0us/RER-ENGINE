use super::*;

pub(super) fn taa_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::hint(
        ui,
        crate::editor::i18n::tr(
            locale,
            "El antialiasing temporal suaviza bordes y estabiliza las sombras entre frames.",
            "Temporal antialiasing smooths edges and stabilizes shadows across frames.",
        ),
    );
    widgets::caption(ui, "TAA");

    let options = [
        (
            crate::editor::i18n::tr(locale, "On", "On"),
            world.taa_enabled,
        ),
        (
            crate::editor::i18n::tr(locale, "Off", "Off"),
            !world.taa_enabled,
        ),
    ];
    if let Some(index) =
        widgets::tier_row_choice(ui, &options, crate::editor::i18n::tr(locale, "TAA", "TAA"))
    {
        actions.push(WorldAction::SetTaa {
            enabled: index == 0,
            blend: world.taa_blend,
            jitter_scale: world.taa_jitter_scale,
        });
    }
    ui.add_space(4.0);

    widgets::label_value(
        ui,
        crate::editor::i18n::tr(locale, "Mezcla", "Blend"),
        &format!("{:.2}", world.taa_blend),
    );
    let mut blend = world.taa_blend;
    if widgets::range_slider(
        ui,
        &mut blend,
        0.0,
        1.0,
        0.01,
        crate::editor::theme::CONTROL_BLUE,
    ) {
        actions.push(WorldAction::SetTaa {
            enabled: world.taa_enabled,
            blend,
            jitter_scale: world.taa_jitter_scale,
        });
    }
    widgets::range_end_labels(
        ui,
        crate::editor::i18n::tr(locale, "Nítido", "Sharp"),
        crate::editor::i18n::tr(locale, "Suave", "Smooth"),
    );
    ui.add_space(4.0);

    widgets::label_value(ui, "Jitter", &format!("{:.2}", world.taa_jitter_scale));
    let mut jitter = world.taa_jitter_scale;
    if widgets::range_slider(
        ui,
        &mut jitter,
        0.0,
        2.0,
        0.05,
        crate::editor::theme::CONTROL_BLUE,
    ) {
        actions.push(WorldAction::SetTaa {
            enabled: world.taa_enabled,
            blend: world.taa_blend,
            jitter_scale: jitter,
        });
    }
    widgets::range_end_labels(
        ui,
        crate::editor::i18n::tr(locale, "Menos", "Less"),
        crate::editor::i18n::tr(locale, "Más", "More"),
    );
}
