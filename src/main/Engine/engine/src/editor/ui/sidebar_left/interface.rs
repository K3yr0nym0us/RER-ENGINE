use super::*;

pub(super) fn ui_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    assets: &AssetLists,
    locale: Locale,
    hint: &str,
    actions: &mut Vec<LeftAction>,
) {
    let active_screen = state
        .active_ui_screen
        .clone()
        .unwrap_or_else(|| "player-hud-01".to_string());
    let screen_text = format!(
        "{}: {active_screen}",
        crate::editor::i18n::tr(locale, "Pantalla activa", "Active screen")
    );
    let font = assets.fonts.first().map(|asset| asset.path.clone());
    let image = assets.hud_images.first().map(|asset| asset.path.clone());

    sub(
        ui,
        state,
        "ui_player",
        icons::CURSOR_CLICK,
        crate::editor::i18n::tr(locale, "Interfaz del jugador", "Player UI"),
        |ui| {
            widgets::tiny(ui, &screen_text);
            let width = ui.available_width();

            if let Some(font) = &font {
                if widgets::wide_button_click_sized(
                    ui,
                    width,
                    icons::PLUS,
                    crate::editor::i18n::tr(locale, "+ Añadir texto", "+ Add text"),
                    hint,
                    crate::editor::theme::INFO,
                ) {
                    actions.push(LeftAction::AddUiText(font.clone()));
                }
            } else {
                widgets::note(
                    ui,
                    crate::editor::i18n::tr(
                        locale,
                        "Carga una fuente en Recursos para añadir texto.",
                        "Load a font in Resources to add text.",
                    ),
                );
            }

            if let Some(image) = &image {
                if widgets::wide_button_click_sized(
                    ui,
                    width,
                    icons::IMAGE,
                    crate::editor::i18n::tr(locale, "+ Añadir imagen", "+ Add image"),
                    hint,
                    crate::editor::theme::INFO,
                ) {
                    actions.push(LeftAction::AddUiImage(image.clone()));
                }
            } else {
                widgets::note(
                    ui,
                    crate::editor::i18n::tr(
                        locale,
                        "Carga una imagen HUD en Recursos para añadir imágenes.",
                        "Load a HUD image in Resources to add images.",
                    ),
                );
            }

            widgets::note(
                ui,
                crate::editor::i18n::tr(
                    locale,
                    "Añadir botones y el editor visual de HUD siguen en la UI React.",
                    "Adding buttons and the visual HUD editor still live in the React UI.",
                ),
            );
        },
    );

    sub(
        ui,
        state,
        "ui_menu",
        icons::LIST,
        crate::editor::i18n::tr(locale, "Menú de UI", "UI Menu"),
        |ui| {
            widgets::note(
                ui,
                crate::editor::i18n::tr(
                    locale,
                    "La lista de pantallas y menús se gestiona en la UI React por ahora.",
                    "The screen and menu list is managed in the React UI for now.",
                ),
            );
        },
    );
}
