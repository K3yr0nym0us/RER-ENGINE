use super::*;

pub(super) fn shadows_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::hint(
        ui,
        crate::editor::i18n::tr(
            locale,
            "El nivel de sombras define resolución y suavizado.",
            "Shadow tier sets resolution and softness.",
        ),
    );
    widgets::caption(
        ui,
        crate::editor::i18n::tr(locale, "Nivel de sombras", "Shadow tier"),
    );

    if let Some(index) = tier_choice(
        ui,
        locale,
        &ShadowTier::ALL,
        world.shadow_tier,
        ShadowTier::as_str,
        crate::editor::i18n::tr(locale, "Nivel de sombras", "Shadow tier"),
    ) {
        actions.push(WorldAction::SetShadowTier(ShadowTier::ALL[index]));
    }

    widgets::label_value(
        ui,
        crate::editor::i18n::tr(locale, "Oscuridad de sombra", "Shadow darkness"),
        &format!("{:.3}", world.shadow_darkness),
    );
    let mut darkness = world.shadow_darkness;
    if widgets::range_slider(
        ui,
        &mut darkness,
        crate::engine3d::SHADOW_DARKNESS_MIN,
        crate::engine3d::SHADOW_DARKNESS_MAX,
        0.001,
        crate::editor::theme::CONTROL_BLUE,
    ) {
        actions.push(WorldAction::SetLight {
            ambient: None,
            intensity: None,
            shadow_darkness: Some(darkness),
        });
    }
}
