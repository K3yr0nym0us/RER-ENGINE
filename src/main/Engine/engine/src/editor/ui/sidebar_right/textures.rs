use super::*;

pub(super) fn textures_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::hint(
        ui,
        crate::editor::i18n::tr(
            locale,
            "El nivel alto mejora la nitidez de las texturas GLB a costa de VRAM.",
            "Higher tiers improve GLB texture sharpness at the cost of VRAM.",
        ),
    );
    widgets::caption(
        ui,
        crate::editor::i18n::tr(
            locale,
            "Nivel de texturas gráficas",
            "Graphics texture tier",
        ),
    );

    if let Some(index) = tier_choice(
        ui,
        locale,
        &TextureTier::ALL,
        world.graphics_texture_tier,
        TextureTier::as_str,
        crate::editor::i18n::tr(locale, "Nivel de texturas GLB", "GLB texture tier"),
    ) {
        actions.push(WorldAction::SetTextureTier(TextureTier::ALL[index]));
    }
    ui.add_space(4.0);

    widgets::label_value(
        ui,
        crate::editor::i18n::tr(locale, "Distancia de detalle", "Texture detail distance"),
        &format!("{:.0} m", world.texture_detail_distance),
    );
    let mut distance = world.texture_detail_distance;
    if widgets::range_slider(
        ui,
        &mut distance,
        5.0,
        80.0,
        1.0,
        crate::editor::theme::CONTROL_BLUE,
    ) {
        actions.push(WorldAction::SetTextureDetailDistance(distance));
    }
    widgets::tiny(
        ui,
        crate::editor::i18n::tr(
            locale,
            "Más distancia = texturas nítidas más lejos.",
            "Longer distance = sharp textures further away.",
        ),
    );
}
