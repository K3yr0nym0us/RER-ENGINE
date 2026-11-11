use super::*;

pub(super) fn workspace_3d(
    ui: &mut egui::Ui,
    radius: &mut NumberDraft,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::label_value(
        ui,
        crate::editor::i18n::tr(locale, "Radio de esfera (u)", "Sphere radius (u)"),
        &format!("{:.1} u", world.world_radius),
    );

    let mut value = world.world_radius;
    if widgets::range_slider(
        ui,
        &mut value,
        5.0,
        500.0,
        1.0,
        crate::editor::theme::CONTROL_BLUE,
    ) {
        actions.push(WorldAction::SetRadius(value));
    }
    ui.add_space(2.0);

    ui.label(
        RichText::new(crate::editor::i18n::tr(locale, "Radio (u)", "Radius (u)"))
            .size(11.5)
            .color(crate::editor::theme::TEXT_MUTED),
    );
    let width = ui.available_width();
    let committed = widgets::number_input(ui, width, &mut radius.text);
    if committed && let Some(value) = radius.parsed() {
        actions.push(WorldAction::SetRadius(value));
    }
}

pub(super) fn workspace_2d(
    ui: &mut egui::Ui,
    width_draft: &mut NumberDraft,
    height_draft: &mut NumberDraft,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    let mut commit = false;
    ui.columns(2, |columns| {
        let label_width = columns[0].available_width();
        columns[0].label(
            RichText::new(crate::editor::i18n::tr(locale, "Ancho (u)", "Width (u)"))
                .size(11.5)
                .color(crate::editor::theme::TEXT_MUTED),
        );
        if widgets::number_input(&mut columns[0], label_width, &mut width_draft.text) {
            commit = true;
        }

        let label_height = columns[1].available_width();
        columns[1].label(
            RichText::new(crate::editor::i18n::tr(locale, "Alto (u)", "Height (u)"))
                .size(11.5)
                .color(crate::editor::theme::TEXT_MUTED),
        );
        if widgets::number_input(&mut columns[1], label_height, &mut height_draft.text) {
            commit = true;
        }
    });

    if commit
        && let (Some(width), Some(height)) = (width_draft.parsed(), height_draft.parsed())
        && width > 0.0
        && height > 0.0
    {
        actions.push(WorldAction::SetSize { width, height });
    }
}

pub(super) fn background_body(
    ui: &mut egui::Ui,
    locale: Locale,
    hint: &str,
    backgrounds: &[crate::editor::assets::AssetEntry],
    active: Option<&str>,
    actions: &mut Vec<WorldAction>,
) {
    if backgrounds.is_empty() {
        widgets::note(
            ui,
            crate::editor::i18n::tr(
                locale,
                "Carga un fondo en Recursos para poder aplicarlo.",
                "Load a background in Resources to apply it.",
            ),
        );
        return;
    }

    let width = ui.available_width();
    let options: Vec<(String, String)> = backgrounds
        .iter()
        .map(|asset| (asset.path.clone(), asset.name.clone()))
        .collect();
    let mut selected = active.map(str::to_string);

    if widgets::combo_string(ui, "world_background_pick", width, &options, &mut selected)
        && let Some(path) = selected
    {
        actions.push(WorldAction::SetBackground(Some(path)));
    }

    if active.is_some()
        && widgets::wide_button_click_sized(
            ui,
            width,
            icons::TRASH,
            crate::editor::i18n::tr(locale, "Quitar fondo", "Clear background"),
            hint,
            crate::editor::theme::DANGER,
        )
    {
        actions.push(WorldAction::SetBackground(None));
    }
}
