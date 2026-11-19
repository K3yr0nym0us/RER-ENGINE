use super::*;

pub(super) fn lighting_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    sun_cycle_day: &mut NumberDraft,
    sun_cycle_night: &mut NumberDraft,
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
            sun_cycle_enabled: None,
            sun_rise_cardinal: None,
            sun_cycle_day: None,
            sun_cycle_night: None,
            sun_cycle_phase: None,
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
            sun_cycle_enabled: None,
            sun_rise_cardinal: None,
            sun_cycle_day: None,
            sun_cycle_night: None,
            sun_cycle_phase: None,
        });
    }

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(4.0);

    // ── Ciclo día/noche ──────────────────────────────────────────────────────
    let mut cycle = world.sun_cycle_enabled;
    if ui
        .checkbox(
            &mut cycle,
            crate::editor::i18n::tr(locale, "Ciclo día/noche", "Day/night cycle"),
        )
        .changed()
    {
        actions.push(WorldAction::SetLight {
            ambient: None,
            intensity: None,
            shadow_darkness: None,
            sun_cycle_enabled: Some(cycle),
            sun_rise_cardinal: None,
            sun_cycle_day: None,
            sun_cycle_night: None,
            sun_cycle_phase: None,
        });
    }

    // Orientación del amanecer, alineada con la rosa de los vientos del mundo.
    const CARDINALS: [(&str, &str, &str); 4] = [
        ("east", "Sale por el Este", "Rises in the East"),
        ("north", "Sale por el Norte", "Rises in the North"),
        ("west", "Sale por el Oeste", "Rises in the West"),
        ("south", "Sale por el Sur", "Rises in the South"),
    ];
    let current_label = CARDINALS
        .iter()
        .find(|(wire, _, _)| *wire == world.sun_rise_cardinal)
        .map(|(_, es, en)| crate::editor::i18n::tr(locale, es, en))
        .unwrap_or("Este");
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(crate::editor::i18n::tr(
                locale,
                "Orientación del sol",
                "Sun orientation",
            ))
            .size(11.0)
            .color(crate::editor::theme::TEXT_MUTED),
        );
        let mut next = world.sun_rise_cardinal;
        egui::ComboBox::from_id_salt("sun_rise_cardinal_combo")
            .selected_text(RichText::new(current_label).size(12.0))
            .show_ui(ui, |ui| {
                for (wire, es, en) in CARDINALS {
                    ui.selectable_value(
                        &mut next,
                        wire,
                        RichText::new(crate::editor::i18n::tr(locale, es, en)).size(12.0),
                    );
                }
            });
        if next != world.sun_rise_cardinal {
            actions.push(WorldAction::SetLight {
                ambient: None,
                intensity: None,
                shadow_darkness: None,
                sun_cycle_enabled: None,
                sun_rise_cardinal: Some(next),
                sun_cycle_day: None,
                sun_cycle_night: None,
                sun_cycle_phase: None,
            });
        }
    });

    // La UI lo etiqueta en milisegundos: 1000 de día + 1000 de noche = 2000.
    let cycle_field = |ui: &mut egui::Ui,
                       label: String,
                       draft: &mut NumberDraft,
                       value: f32,
                       actions: &mut Vec<WorldAction>,
                       is_day: bool| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(label)
                    .size(11.0)
                    .color(crate::editor::theme::TEXT_MUTED),
            );
            if widgets::number_input(ui, 56.0, &mut draft.text) {
                if let Some(parsed) = draft.parsed().map(|v| v.clamp(100.0, 100_000.0)) {
                    draft.force(parsed);
                    actions.push(WorldAction::SetLight {
                        ambient: None,
                        intensity: None,
                        shadow_darkness: None,
                        sun_cycle_enabled: None,
                        sun_rise_cardinal: None,
                        sun_cycle_day: is_day.then_some(parsed),
                        sun_cycle_night: (!is_day).then_some(parsed),
                        sun_cycle_phase: None,
                    });
                } else {
                    draft.sync(value);
                }
            }
        });
    };
    cycle_field(
        ui,
        crate::editor::i18n::tr(
            locale,
            "Ciclo de 24hrs en milisegundos (día)",
            "24h cycle in milliseconds (day)",
        )
        .to_string(),
        sun_cycle_day,
        world.sun_cycle_day,
        actions,
        true,
    );
    cycle_field(
        ui,
        crate::editor::i18n::tr(
            locale,
            "Ciclo de 24hrs en milisegundos (noche)",
            "24h cycle in milliseconds (night)",
        )
        .to_string(),
        sun_cycle_night,
        world.sun_cycle_night,
        actions,
        false,
    );

    // Fase manual: cubre la vuelta completa [0,1). Durante el ciclo automático
    // el motor reporta la fase y el slider la sigue.
    let mut phase = world.sun_cycle_phase;
    widgets::label_value(
        ui,
        crate::editor::i18n::tr(locale, "Posición del sol", "Sun position"),
        &format!("{:.0}%", phase * 100.0),
    );
    if widgets::range_slider(
        ui,
        &mut phase,
        0.0,
        1.0,
        0.001,
        crate::editor::theme::CONTROL_BLUE,
    ) {
        actions.push(WorldAction::SetLight {
            ambient: None,
            intensity: None,
            shadow_darkness: None,
            sun_cycle_enabled: None,
            sun_rise_cardinal: None,
            sun_cycle_day: None,
            sun_cycle_night: None,
            sun_cycle_phase: Some(phase),
        });
    }
}
