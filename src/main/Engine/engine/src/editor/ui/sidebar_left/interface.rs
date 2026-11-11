use super::*;

pub(super) fn ui_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    ui_screens: &crate::editor::ui_screens::UiScreenLists,
    locale: Locale,
    hint: &str,
    actions: &mut Vec<LeftAction>,
) {
    sub(
        ui,
        state,
        "ui_player",
        icons::CURSOR_CLICK,
        crate::editor::i18n::tr(locale, "Interfaz del jugador", "Player UI"),
        |ui| {
            let width = ui.available_width();

            if widgets::wide_button_click_sized(
                ui,
                width,
                icons::PLUS,
                crate::editor::i18n::tr(locale, "Nuevo HUD", "New HUD"),
                hint,
                crate::editor::theme::SUCCESS,
            ) {
                actions.push(LeftAction::CreateUiScreen("player"));
            }
            ui.add_space(2.0);

            for screen in &ui_screens.player {
                screen_row(ui, screen, "player", locale, actions);
            }
        },
    );

    sub(
        ui,
        state,
        "ui_menu",
        icons::LIST,
        crate::editor::i18n::tr(locale, "Men\u{00fa} de UI", "UI Menu"),
        |ui| {
            let width = ui.available_width();
            if widgets::wide_button_click_sized(
                ui,
                width,
                icons::PLUS,
                crate::editor::i18n::tr(locale, "Nuevo men\u{00fa}", "New UI"),
                hint,
                crate::editor::theme::SUCCESS,
            ) {
                actions.push(LeftAction::CreateUiScreen("menu"));
            }
            ui.add_space(2.0);

            for screen in &ui_screens.menu {
                screen_row(ui, screen, "menu", locale, actions);
            }
        },
    );
}

fn screen_row(
    ui: &mut egui::Ui,
    screen: &crate::editor::ui_screens::UiScreenEntry,
    scope: &'static str,
    locale: Locale,
    actions: &mut Vec<LeftAction>,
) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(8, 6))
        .corner_radius(4)
        .stroke(egui::Stroke::new(1.0, crate::editor::theme::BORDER))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let name_width = (ui.available_width() - 70.0).max(0.0);
                ui.add_sized(
                    [name_width, 18.0],
                    egui::Label::new(
                        RichText::new(&screen.name)
                            .size(13.0)
                            .color(crate::editor::theme::TEXT),
                    )
                    .truncate(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if widgets::icon_action(
                        ui,
                        icons::TRASH,
                        18.0,
                        crate::editor::theme::DANGER,
                        crate::editor::i18n::tr(locale, "Eliminar pantalla", "Delete screen"),
                    ) {
                        actions.push(LeftAction::DeleteUiScreen {
                            scope,
                            id: screen.id.clone(),
                            name: screen.name.clone(),
                        });
                    }
                    if widgets::icon_action(
                        ui,
                        icons::PENCIL,
                        18.0,
                        crate::editor::theme::WARNING,
                        if scope == "player" {
                            crate::editor::i18n::tr(locale, "Editar HUD", "Edit HUD")
                        } else {
                            crate::editor::i18n::tr(locale, "Renombrar men\u{00fa}", "Rename UI")
                        },
                    ) {
                        actions.push(LeftAction::OpenUiEditor {
                            scope,
                            screen_id: screen.id.clone(),
                        });
                    }
                    if scope == "player" {
                        let mut active = screen.active;
                        if widgets::switch_value(
                            ui,
                            &mut active,
                            crate::editor::i18n::tr(
                                locale,
                                "Activo en modo juego",
                                "Active in play",
                            ),
                        ) {
                            actions.push(LeftAction::SetActiveUiScreen(if active {
                                Some(screen.id.clone())
                            } else {
                                None
                            }));
                        }
                    }
                });
            });
        });
    ui.add_space(2.0);
}
