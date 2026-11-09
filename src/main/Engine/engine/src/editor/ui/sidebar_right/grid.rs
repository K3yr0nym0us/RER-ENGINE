use super::*;

pub(super) fn grid_body(
    ui: &mut egui::Ui,
    locked: &mut bool,
    cell: &mut NumberDraft,
    context: ProjectContext,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    if context.is_3d() {
        widgets::hint(
            ui,
            crate::editor::i18n::tr(
                locale,
                "El tamaño de celda alinea el checker del suelo y el snap (Ctrl).",
                "Cell size aligns ground checker and object placement snap (Ctrl).",
            ),
        );
    } else {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let mut visible = world.grid_visible;
            let icon = if world.grid_visible {
                icons::EYE
            } else {
                icons::EYE_SLASH
            };
            if widgets::icon_toggle_value(
                ui,
                icon,
                &mut visible,
                crate::editor::i18n::tr(locale, "Mostrar/ocultar rejilla", "Show/hide grid"),
            ) {
                actions.push(WorldAction::SetGridVisible(visible));
            }
        });
    }

    let mut commit_cell = false;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(crate::editor::i18n::tr(
                locale,
                "Tamaño de celda",
                "Cell size",
            ))
            .size(12.0)
            .color(crate::editor::theme::TEXT_MUTED),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let icon = if *locked {
                icons::LOCK
            } else {
                icons::LOCK_OPEN
            };
            let mut lock_state = *locked;
            if widgets::icon_toggle_value(
                ui,
                icon,
                &mut lock_state,
                crate::editor::i18n::tr(locale, "Bloquear tamaño de rejilla", "Lock grid size"),
            ) {
                *locked = lock_state;
            }
            ui.add_enabled_ui(!*locked, |ui| {
                commit_cell = widgets::number_input(ui, 55.0, &mut cell.text);
            });
        });
    });
    ui.add_space(2.0);

    ui.add_enabled_ui(!*locked, |ui| {
        let mut value = world.grid_cell_size;
        if widgets::range_slider(
            ui,
            &mut value,
            0.25,
            10.0,
            0.25,
            crate::editor::theme::CONTROL_BLUE,
        ) {
            actions.push(WorldAction::SetGridCellSize(value));
        }
    });

    if !*locked
        && commit_cell
        && let Some(value) = cell.parsed()
        && value > 0.0
    {
        actions.push(WorldAction::SetGridCellSize(value));
    }
}
