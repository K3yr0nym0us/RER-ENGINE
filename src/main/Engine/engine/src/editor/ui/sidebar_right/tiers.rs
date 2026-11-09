use super::*;

pub(super) fn tier_choice<T: Copy + PartialEq, F: Fn(T) -> &'static str>(
    ui: &mut egui::Ui,
    locale: Locale,
    all: &[T],
    active: T,
    key: F,
    hint: &str,
) -> Option<usize> {
    let options: Vec<(&'static str, bool)> = all
        .iter()
        .map(|tier| (tier_label(locale, key(*tier)), *tier == active))
        .collect();
    widgets::tier_row_choice(ui, &options, hint)
}

pub(super) fn tier_label(locale: Locale, key: &'static str) -> &'static str {
    match key {
        "off" => crate::editor::i18n::tr(locale, "Off", "Off"),
        "low" => crate::editor::i18n::tr(locale, "Bajo", "Low"),
        "medium" => crate::editor::i18n::tr(locale, "Medio", "Medium"),
        "high" => crate::editor::i18n::tr(locale, "Alto", "High"),
        "ultra" => "Ultra",
        _ => key,
    }
}
