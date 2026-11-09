use super::*;

pub(super) fn msaa_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::hint(
        ui,
        crate::editor::i18n::tr(
            locale,
            "Más muestras suavizan bordes a costa de rendimiento.",
            "More samples smooth edges at the cost of performance.",
        ),
    );
    widgets::caption(
        ui,
        crate::editor::i18n::tr(locale, "Nivel MSAA", "MSAA tier"),
    );

    if let Some(index) = tier_choice(
        ui,
        locale,
        &MsaaTier::ALL,
        world.msaa_tier,
        MsaaTier::as_str,
        crate::editor::i18n::tr(locale, "Nivel MSAA", "MSAA tier"),
    ) {
        actions.push(WorldAction::SetMsaaTier(MsaaTier::ALL[index]));
    }
}
