use super::*;

pub(super) fn gravity_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::label_value(
        ui,
        crate::editor::i18n::tr(locale, "Gravedad", "Gravity"),
        &format!("{:.2} u/s²", world.gravity),
    );
    let mut gravity = world.gravity;
    if widgets::range_slider(
        ui,
        &mut gravity,
        0.0,
        50.0,
        0.5,
        crate::editor::theme::CONTROL_BLUE,
    ) {
        actions.push(WorldAction::SetGravity(gravity));
    }
}
