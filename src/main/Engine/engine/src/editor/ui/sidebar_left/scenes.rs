use super::*;

pub(super) fn scenes_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    scenes: &SceneList,
    context: ProjectContext,
    locale: Locale,
    hint: &str,
    actions: &mut Vec<LeftAction>,
) {
    sub(
        ui,
        state,
        "scenes_programming",
        icons::CODE,
        crate::editor::i18n::tr(locale, "Programación", "Programming"),
        |ui| {
            let width = ui.available_width();
            if widgets::wide_button_click_sized(
                ui,
                width,
                icons::FILE_CODE,
                crate::editor::i18n::tr(locale, "Script de escena", "Scene script"),
                hint,
                crate::editor::theme::WARNING,
            ) {
                actions.push(LeftAction::OpenLegacyWindow("scene_script"));
            }
            if widgets::wide_button_click_sized(
                ui,
                width,
                icons::GRAPH,
                crate::editor::i18n::tr(locale, "Lógica de escena (nodos)", "Scene logic (nodes)"),
                hint,
                crate::editor::theme::INFO,
            ) {
                actions.push(LeftAction::OpenLegacyWindow("scene_visual_script"));
            }
        },
    );

    if !context.is_3d() {
        widgets::note(
            ui,
            crate::editor::i18n::tr(
                locale,
                "La gestión de escenas 2D sigue en la UI React.",
                "2D scene management still lives in the React UI.",
            ),
        );
        return;
    }

    let create_width = ui.available_width();
    if !scenes.loading
        && widgets::wide_button_click_sized(
            ui,
            create_width,
            icons::PLUS,
            crate::editor::i18n::tr(locale, "Crear nueva escena", "Create new scene"),
            crate::editor::i18n::tr(
                locale,
                "Crea una escena 3D vacía",
                "Create an empty 3D scene",
            ),
            crate::editor::theme::OUTLINE_SECONDARY,
        )
    {
        actions.push(LeftAction::OpenCreateScene);
    }
    ui.add_space(2.0);

    if scenes.loading && scenes.scenes.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add(egui::Spinner::new().size(16.0));
            ui.label(
                RichText::new(crate::editor::i18n::tr(
                    locale,
                    "Cargando escenas…",
                    "Loading scenes...",
                ))
                .size(12.0)
                .color(crate::editor::theme::TEXT_MUTED),
            );
        });
        return;
    }

    if scenes.scenes.is_empty() {
        widgets::note(
            ui,
            crate::editor::i18n::tr(
                locale,
                "Sin escenas del motor todavía.",
                "No engine scenes yet.",
            ),
        );
        return;
    }

    for scene in &scenes.scenes {
        let is_active = scene.id == scenes.active_scene_id;
        let switching = scenes.switching_to == Some(scene.id);
        let fill = if is_active {
            crate::editor::theme::SCENE_ACTIVE_BG
        } else {
            Color32::TRANSPARENT
        };
        let stroke = if is_active {
            egui::Stroke::new(1.0, crate::editor::theme::SCENE_ACTIVE_TEXT)
        } else {
            egui::Stroke::NONE
        };

        egui::Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let name_width = (ui.available_width() - 46.0).max(0.0);
                    let name_color = if is_active {
                        crate::editor::theme::SCENE_ACTIVE_TEXT
                    } else {
                        crate::editor::theme::TEXT
                    };
                    ui.add_sized(
                        [name_width, 18.0],
                        egui::Label::new(RichText::new(&scene.name).size(13.0).color(name_color))
                            .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let can_act = !scenes.loading;
                        if can_act
                            && scenes.can_delete()
                            && widgets::icon_action(
                                ui,
                                icons::TRASH,
                                20.0,
                                crate::editor::theme::DANGER,
                                crate::editor::i18n::tr(locale, "Eliminar escena", "Delete scene"),
                            )
                        {
                            actions.push(LeftAction::OpenDeleteScene(scene.id));
                        }
                        if !is_active {
                            if switching {
                                ui.add(egui::Spinner::new().size(14.0));
                            } else if can_act
                                && widgets::icon_action(
                                    ui,
                                    icons::SIGN_IN,
                                    20.0,
                                    crate::editor::theme::INFO,
                                    crate::editor::i18n::tr(locale, "Cargar escena", "Load scene"),
                                )
                            {
                                actions.push(LeftAction::SwitchScene(scene.id));
                            }
                        }
                    });
                });
            });
    }
}
