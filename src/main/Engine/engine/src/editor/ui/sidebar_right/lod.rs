use super::*;

pub(super) fn lod_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::hint(
        ui,
        crate::editor::i18n::tr(
            locale,
            "Cambia las mallas GLB por versiones simplificadas al alejarse de la cámara.",
            "Swaps GLB meshes for simplified versions as they get far from the camera.",
        ),
    );

    let mut enabled = world.model_lod_enabled;
    if widgets::switch_value(
        ui,
        &mut enabled,
        crate::editor::i18n::tr(locale, "LOD de modelos", "Model LOD"),
    ) {
        actions.push(WorldAction::SetModelLod {
            enabled: Some(enabled),
            lod1_distance: None,
            lod2_distance: None,
        });
    }

    widgets::label_value(
        ui,
        crate::editor::i18n::tr(locale, "Distancia LOD 1", "LOD 1 distance"),
        &format!("{:.0} m", world.model_lod1_distance),
    );
    let mut lod1 = world.model_lod1_distance;
    if widgets::range_slider(
        ui,
        &mut lod1,
        5.0,
        200.0,
        1.0,
        crate::editor::theme::CONTROL_BLUE,
    ) {
        actions.push(WorldAction::SetModelLod {
            enabled: None,
            lod1_distance: Some(lod1),
            lod2_distance: None,
        });
    }

    widgets::label_value(
        ui,
        crate::editor::i18n::tr(locale, "Distancia LOD 2", "LOD 2 distance"),
        &format!("{:.0} m", world.model_lod2_distance),
    );
    let mut lod2 = world.model_lod2_distance;
    if widgets::range_slider(
        ui,
        &mut lod2,
        10.0,
        400.0,
        1.0,
        crate::editor::theme::CONTROL_BLUE,
    ) {
        actions.push(WorldAction::SetModelLod {
            enabled: None,
            lod1_distance: None,
            lod2_distance: Some(lod2),
        });
    }

    widgets::tiny(
        ui,
        crate::editor::i18n::tr(
            locale,
            "Mallas con menos de 2048 vértices no generan variantes LOD.",
            "Meshes with fewer than 2048 vertices do not generate LOD variants.",
        ),
    );
}
