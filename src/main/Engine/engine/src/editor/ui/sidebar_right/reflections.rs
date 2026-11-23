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

    let ssr_active = world.reflection_mode == "ssr";
    let rt_active = world.reflection_mode == "rt";

    // SSR: la fila de tiers de siempre, con su propio check.
    reflection_section(
        ui,
        locale,
        actions,
        crate::editor::i18n::tr(locale, "SSR", "SSR"),
        ssr_active,
        world.reflection_tier,
        "ssr",
        crate::editor::i18n::tr(
            locale,
            "Nivel de reflejos SSR (Off desactiva SSR y composición)",
            "SSR reflection tier (Off disables SSR and composite)",
        ),
    );
    ui.add_space(4.0);

    // RTX: fila nueva con los parámetros del preset (resolución, rayos, rebotes...).
    reflection_section(
        ui,
        locale,
        actions,
        crate::editor::i18n::tr(locale, "RTX", "RTX"),
        rt_active,
        world.rt_tier,
        "rt",
        crate::editor::i18n::tr(
            locale,
            "Nivel de reflejos RTX (resolución, rayos, rebotes, denoise)",
            "RTX reflection tier (resolution, rays, bounces, denoise)",
        ),
    );
    ui.add_space(4.0);

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
    ui.add_space(4.0);
    if widgets::wide_button_click_sized(
        ui,
        ui.available_width(),
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

/// Un bloque completo: check de activación + fila de tiers. Los checks son radio: activar uno
/// desactiva el otro (SSR y RTX jamás están activos a la vez).
fn reflection_section(
    ui: &mut egui::Ui,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
    title: &'static str,
    active: bool,
    tier: ReflectionTier,
    mode: &'static str,
    hint: &str,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(title)
                .size(12.0)
                .strong()
                .color(crate::editor::theme::TEXT_MUTED),
        );
        let mut enabled = active;
        if widgets::switch_value(
            ui,
            &mut enabled,
            crate::editor::i18n::tr(locale, "Activar", "Enable"),
        ) {
            // El switch SIEMPRE arranca en Low. Solo un botón de tier explícito puede
            // activar el sistema en otro nivel (esa ruta empuja el tier antes que el modo).
            if enabled {
                if mode == "rt" {
                    actions.push(WorldAction::SetRtTier(ReflectionTier::Low));
                } else {
                    actions.push(WorldAction::SetReflectionTier(ReflectionTier::Low));
                }
            }
            actions.push(WorldAction::SetReflectionMode(if enabled {
                mode
            } else {
                "off"
            }));
        }
    });

    // El switch controla el on/off: la fila solo ofrece Low/Medium/High/Ultra. Si el sistema
    // no está activo se pasa `Off` (que no está en la lista) para que ningún botón quede
    // resaltado; el tier real sigue guardado y se restaurará al volver a encender el switch.
    let highlighted = if active { tier } else { ReflectionTier::Off };
    if let Some(index) = tier_choice(
        ui,
        locale,
        &ReflectionTier::SELECTABLE,
        highlighted,
        ReflectionTier::as_str,
        hint,
    ) {
        let chosen = ReflectionTier::SELECTABLE[index];
        if mode == "rt" {
            actions.push(WorldAction::SetRtTier(chosen));
        } else {
            actions.push(WorldAction::SetReflectionTier(chosen));
        }
        // Elegir un tier también activa el sistema si el switch estaba apagado.
        if !active {
            actions.push(WorldAction::SetReflectionMode(mode));
        }
    }
}
