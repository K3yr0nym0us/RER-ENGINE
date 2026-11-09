use super::*;

pub(super) fn fps_body(
    ui: &mut egui::Ui,
    selection: &mut u32,
    dirty: &mut bool,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    let mut options = vec![60u32, 120, 144, 240];
    if !options.contains(selection) {
        options.push(*selection);
    }

    let width = (ui.available_width() - 34.0).max(0.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if widgets::combo_u32(ui, "editor_target_fps", width, &options, selection) {
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

    widgets::tiny(
        ui,
        &format!(
            "{} {}",
            crate::editor::i18n::tr(locale, "Actual:", "Current:"),
            world.target_fps
        ),
    );
}
