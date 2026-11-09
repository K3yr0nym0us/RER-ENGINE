use super::*;

pub(super) fn reflections_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    if let Some(effective) = world.reflection_tier_effective
        && effective != world.reflection_tier
        && matches!(
            world.reflection_tier,
            ReflectionTier::High | ReflectionTier::Ultra
        )
    {
        ui.label(
            RichText::new(crate::editor::i18n::tr(
                locale,
                "La GPU no soporta el tier pedido; el motor usa un nivel menor.",
                "The GPU does not support the requested tier; the engine uses a lower one.",
            ))
            .size(10.0)
            .color(crate::editor::theme::WARNING_BADGE),
        );
    }

    let reflections_active = !world.reflection_tier.is_off();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(crate::editor::i18n::tr(
                locale,
                "Nivel de reflejos",
                "Reflection tier",
            ))
            .size(12.0)
            .strong()
            .color(crate::editor::theme::TEXT_MUTED),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(
                RichText::new("RT ON")
                    .size(9.5)
                    .color(if world.reflection_raytracing {
                        crate::editor::theme::INFO
                    } else {
                        crate::editor::theme::TEXT_MUTED
                    }),
            );
            ui.add_enabled_ui(reflections_active, |ui| {
                let mut enabled = world.reflection_raytracing;
                if widgets::switch_value(
                    ui,
                    &mut enabled,
                    crate::editor::i18n::tr(
                        locale,
                        "Ray tracing de reflejos",
                        "Reflection ray tracing",
                    ),
                ) {
                    actions.push(WorldAction::SetReflectionRaytracing(enabled));
                }
            });
            ui.label(
                RichText::new("RT OFF")
                    .size(9.5)
                    .color(if world.reflection_raytracing {
                        crate::editor::theme::TEXT_MUTED
                    } else {
                        crate::editor::theme::INFO
                    }),
            );
        });
    });

    if let Some(index) = tier_choice(
        ui,
        locale,
        &ReflectionTier::ALL,
        world.reflection_tier,
        ReflectionTier::as_str,
        crate::editor::i18n::tr(
            locale,
            "Nivel de reflejos (Off desactiva SSR y composición)",
            "Reflection tier (Off disables SSR and composite)",
        ),
    ) {
        actions.push(WorldAction::SetReflectionTier(ReflectionTier::ALL[index]));
    }
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.add_enabled_ui(reflections_active, |ui| {
            let mut probes = world.reflection_probes;
            if widgets::switch_value(
                ui,
                &mut probes,
                crate::editor::i18n::tr(
                    locale,
                    "Reflection probes en el render",
                    "Reflection probes in the render",
                ),
            ) {
                actions.push(WorldAction::SetReflectionProbes(probes));
            }
        });
        let width = ui.available_width();
        if widgets::wide_button_click_sized(
            ui,
            width,
            "",
            crate::editor::i18n::tr(
                locale,
                "Insertar sonda de reflexión",
                "Insert reflection probe",
            ),
            crate::editor::i18n::tr(
                locale,
                "Coloca una sonda en el centro del viewport",
                "Place a probe at the viewport center",
            ),
            crate::editor::theme::INFO,
        ) {
            actions.push(WorldAction::SpawnReflectionProbe);
        }
    });
    ui.add_space(4.0);

    widgets::caption(
        ui,
        crate::editor::i18n::tr(locale, "Vista de depuración", "Reflection debug view"),
    );
    let mut view = world.reflection_debug_view;
    let width = ui.available_width();
    if widgets::combo_text(
        ui,
        "editor_reflection_debug_view",
        width,
        &REFLECTION_DEBUG_VIEWS,
        &mut view,
    ) {
        actions.push(WorldAction::SetReflectionDebugView(normalize_debug_view(
            view,
        )));
    }
}
