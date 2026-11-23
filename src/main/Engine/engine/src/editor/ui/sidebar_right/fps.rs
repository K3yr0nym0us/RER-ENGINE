use super::*;

pub(super) fn fps_body(
    ui: &mut egui::Ui,
    selection: &mut u32,
    dirty: &mut bool,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    let unlimited = crate::editor::i18n::tr(locale, "Sin límites", "Unlimited");
    let mut options: Vec<(u32, String)> = [30u32, 60, 120, 144, 240]
        .into_iter()
        .map(|fps| (fps, format!("{fps} FPS")))
        .collect();
    options.push((0, unlimited.to_string()));
    if !options.iter().any(|(fps, _)| *fps == *selection) {
        options.push((*selection, format!("{selection} FPS")));
    }

    let width = (ui.available_width() - 34.0).max(0.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if widgets::combo_u32_labeled(ui, "editor_target_fps", width, &options, selection) {
            *dirty = true;
        }
        if widgets::small_icon_button_click(
            ui,
            icons::CHECK,
            crate::editor::i18n::tr(locale, "Aplicar límite de FPS", "Apply FPS limit"),
        ) {
            actions.push(WorldAction::SetTargetFps(*selection));
            *dirty = false;
        }
    });

    let current = if world.target_fps == 0 {
        unlimited.to_string()
    } else {
        world.target_fps.to_string()
    };
    widgets::tiny(
        ui,
        &format!(
            "{} {}",
            crate::editor::i18n::tr(locale, "Actual:", "Current:"),
            current
        ),
    );
}
