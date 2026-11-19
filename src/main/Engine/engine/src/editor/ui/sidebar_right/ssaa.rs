use super::*;

pub(super) fn ssaa_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::hint(
        ui,
        crate::editor::i18n::tr(
            locale,
            "Supermuestrea la escena y la reduce al presentar: bordes más suaves a costa de rendimiento.",
            "Supersamples the scene and downsamples when presenting: smoother edges at the cost of performance.",
        ),
    );
    widgets::caption(
        ui,
        crate::editor::i18n::tr(locale, "Nivel SSAA", "SSAA tier"),
    );

    if let Some(index) = tier_choice(
        ui,
        locale,
        &SsaaTier::ALL,
        world.ssaa_tier,
        SsaaTier::as_str,
        crate::editor::i18n::tr(locale, "Nivel SSAA", "SSAA tier"),
    ) {
        actions.push(WorldAction::SetSsaaTier(SsaaTier::ALL[index]));
    }

    let desired = world.ssaa_tier.desired_scale();
    let effective = if world.ssaa_tier == SsaaTier::Low {
        crate::editor::i18n::tr(locale, "Sin SSAA (1×)", "No SSAA (1×)").to_string()
    } else if world.ssaa_scale + 0.005 < desired {
        format!(
            "{:.2}× {} ({:.2}× {})",
            world.ssaa_scale,
            crate::editor::i18n::tr(locale, "efectivo", "effective"),
            desired,
            crate::editor::i18n::tr(locale, "pedido", "requested")
        )
    } else {
        format!(
            "{:.2}× {} · {} {}",
            world.ssaa_scale,
            crate::editor::i18n::tr(locale, "efectivo", "effective"),
            world.ssaa_tier.desired_samples(),
            crate::editor::i18n::tr(locale, "muestras", "samples")
        )
    };
    widgets::hint(ui, &effective);
}
