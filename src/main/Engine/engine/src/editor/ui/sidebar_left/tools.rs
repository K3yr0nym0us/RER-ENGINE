use super::*;

pub(super) fn tools_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    is_3d: bool,
    locale: Locale,
    quick_build_active: Option<&str>,
    actions: &mut Vec<LeftAction>,
) {
    // Tarjetas 64×64 como React; en 3D hay 4 y la cuarta cae a una segunda fila.
    type ToolCard = (
        Option<&'static str>,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        egui::Color32,
        &'static str,
    );
    let mut cards: Vec<ToolCard> = vec![
        (
            Some("draw_collider"),
            icons::WALL,
            crate::editor::i18n::tr(locale, "Crear", "Create"),
            crate::editor::i18n::tr(locale, "Muro", "Wall"),
            crate::editor::i18n::tr(locale, "Colisionador", "Collider"),
            crate::editor::theme::INFO,
            crate::editor::i18n::tr(
                locale,
                "Activar herramienta de colisionador",
                "Activate collider drawing tool",
            ),
        ),
        (
            Some("draw_execution_area"),
            icons::CODE_BLOCK,
            crate::editor::i18n::tr(locale, "Crear", "Create"),
            "Trigger",
            crate::editor::i18n::tr(locale, "Área de ejecución", "Execution area"),
            crate::editor::theme::DANGER,
            crate::editor::i18n::tr(
                locale,
                "Activar herramienta de área de ejecución",
                "Activate execution area tool",
            ),
        ),
        (
            Some("quick_build"),
            icons::WRENCH,
            crate::editor::i18n::tr(locale, "Construcción", "Construction"),
            crate::editor::i18n::tr(locale, "Rápida", "Quick"),
            crate::editor::i18n::tr(locale, "Quick build", "Quick build"),
            crate::editor::theme::WARNING,
            crate::editor::i18n::tr(
                locale,
                "Construcción rápida con blueprint",
                "Quick build from blueprint",
            ),
        ),
    ];

    if is_3d {
        cards.push((
            Some("socket_config"),
            icons::TREE_STRUCTURE,
            "Socket",
            "Config",
            crate::editor::i18n::tr(locale, "Sockets", "Sockets"),
            crate::editor::theme::ACCENT,
            crate::editor::i18n::tr(
                locale,
                "Abrir configuración de sockets",
                "Open socket configuration",
            ),
        ));
    }

    for row in cards.chunks(3) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            let count = row.len();
            let gap = if count > 1 {
                ((ui.available_width() - 64.0 * count as f32) / (count - 1) as f32).max(4.0)
            } else {
                0.0
            };

            for (index, (tool, icon, top, bottom, label, color, tooltip)) in row.iter().enumerate()
            {
                if index > 0 {
                    ui.add_space(gap);
                }
                #[allow(clippy::collapsible_match)]
                match tool {
                    Some("socket_config") => {
                        if widgets::tool_card_click(
                            ui, icon, top, bottom, label, *color, false, tooltip,
                        ) {
                            actions.push(LeftAction::OpenSockets);
                        }
                    }
                    Some(tool) if *tool != "quick_build" => {
                        let active = state.active_tool == Some(*tool);
                        if widgets::tool_card_click(
                            ui, icon, top, bottom, label, *color, active, tooltip,
                        ) {
                            let next = if active { "" } else { *tool };
                            actions.push(LeftAction::SetActiveTool(next));
                        }
                    }
                    Some("quick_build") => {
                        let active = quick_build_active.is_some();
                        let card_hint = match quick_build_active {
                            Some(name) => format!("{tooltip} · {name}"),
                            None => (*tooltip).to_string(),
                        };
                        if widgets::tool_card_click(
                            ui,
                            icon,
                            top,
                            bottom,
                            label,
                            *color,
                            active,
                            card_hint.as_str(),
                        ) {
                            actions.push(LeftAction::QuickBuild { active });
                        }
                    }
                    _ => {}
                }
            }
        });
    }

    widgets::tiny(
        ui,
        crate::editor::i18n::tr(
            locale,
            "Colisionador y trigger: activa y haz clic en el viewport. Quick build: elige un blueprint. Los sockets se configuran en esta ventana.",
            "Collider and trigger: activate and click in the viewport. Quick build: pick a blueprint. Sockets are configured in this window.",
        ),
    );
}
