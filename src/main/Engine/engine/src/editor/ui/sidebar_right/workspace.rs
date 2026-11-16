use super::*;

/// Alto reservado por cada cara en la rosa de los vientos.
const COMPASS_FIELD_HEIGHT: f32 = 40.0;
/// Separación vertical entre las tres filas de la rosa.
const COMPASS_ROW_GAP: f32 = 6.0;
/// Separación horizontal entre Oeste, la cruz y Este.
const COMPASS_COL_GAP: f32 = 6.0;

/// Dibuja una rosa de los vientos: N arriba, O a la izquierda, E a la derecha,
/// S abajo y una cruz en el centro que une las cuatro caras.
pub(super) fn workspace_3d(
    ui: &mut egui::Ui,
    north: &mut NumberDraft,
    south: &mut NumberDraft,
    east: &mut NumberDraft,
    west: &mut NumberDraft,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::caption(
        ui,
        crate::editor::i18n::tr(locale, "Límites del mundo (m)", "World bounds (m)"),
    );
    widgets::hint(
        ui,
        crate::editor::i18n::tr(
            locale,
            "Cada cara crece hacia fuera; la cara opuesta se queda fija.",
            "Each face grows outward; the opposite side stays put.",
        ),
    );
    ui.add_space(4.0);

    let width = ui.available_width();
    let cell_width = ((width - COMPASS_COL_GAP * 2.0) / 3.0).max(44.0);
    let block_height = COMPASS_FIELD_HEIGHT * 3.0 + COMPASS_ROW_GAP * 2.0;
    let block = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(width, block_height));

    let cell = |row: usize, col: usize| {
        egui::Rect::from_min_size(
            egui::pos2(
                block.left() + col as f32 * (cell_width + COMPASS_COL_GAP),
                block.top() + row as f32 * (COMPASS_FIELD_HEIGHT + COMPASS_ROW_GAP),
            ),
            egui::vec2(cell_width, COMPASS_FIELD_HEIGHT),
        )
    };

    let mut commit = false;
    let mut field =
        |ui: &mut egui::Ui, row: usize, col: usize, label: &str, draft: &mut NumberDraft| {
            let rect = cell(row, col);
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                ui.set_width(rect.width());
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(label)
                            .size(11.5)
                            .color(crate::editor::theme::TEXT_MUTED),
                    );
                    let input_width = (rect.width() - 8.0).clamp(44.0, 76.0);
                    if widgets::number_input(ui, input_width, &mut draft.text) {
                        commit = true;
                    }
                });
            });
        };

    field(
        ui,
        0,
        1,
        crate::editor::i18n::tr(locale, "Norte", "North"),
        north,
    );
    field(
        ui,
        1,
        0,
        crate::editor::i18n::tr(locale, "Oeste", "West"),
        west,
    );
    draw_compass_cross(ui, cell(1, 1));
    field(
        ui,
        1,
        2,
        crate::editor::i18n::tr(locale, "Este", "East"),
        east,
    );
    field(
        ui,
        2,
        1,
        crate::editor::i18n::tr(locale, "Sur", "South"),
        south,
    );

    // Las celdas se posicionan a mano: se reserva el bloque completo para que
    // el total quede debajo y no dentro de la rosa.
    ui.advance_cursor_after_rect(block);

    if commit {
        actions.push(WorldAction::SetBounds {
            north: north.parsed().unwrap_or(world.north),
            south: south.parsed().unwrap_or(world.south),
            east: east.parsed().unwrap_or(world.east),
            west: west.parsed().unwrap_or(world.west),
        });
    }

    ui.add_space(2.0);
    ui.vertical_centered(|ui| {
        widgets::tiny(
            ui,
            &format!(
                "{:.0} × {:.0} m",
                world.world_width.max(0.0),
                world.world_depth.max(0.0)
            ),
        );
    });
}

/// Cruz de la rosa: brazos hacia N/S/E/O y punto central.
fn draw_compass_cross(ui: &egui::Ui, rect: egui::Rect) {
    let center = rect.center();
    let arm = (rect.width().min(rect.height()) * 0.34).max(8.0);
    let line = crate::editor::theme::PRIMARY_OUTLINE.gamma_multiply(0.9);
    let stroke = egui::Stroke::new(1.2, line);
    let painter = ui.painter();

    painter.line_segment(
        [center - egui::vec2(arm, 0.0), center + egui::vec2(arm, 0.0)],
        stroke,
    );
    painter.line_segment(
        [center - egui::vec2(0.0, arm), center + egui::vec2(0.0, arm)],
        stroke,
    );
    for dir in [
        egui::vec2(arm, 0.0),
        egui::vec2(-arm, 0.0),
        egui::vec2(0.0, arm),
        egui::vec2(0.0, -arm),
    ] {
        painter.circle_filled(center + dir, 1.4, line);
    }
    painter.circle_filled(center, 2.6, crate::editor::theme::CONTROL_BLUE);
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
