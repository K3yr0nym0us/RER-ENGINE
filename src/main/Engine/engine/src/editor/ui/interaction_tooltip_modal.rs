//! Modal nativa con el formulario de un tooltip de interacción.

use serde_json::Value;

use crate::editor::app::EditorApp;
use crate::editor::cli::Locale;
use crate::editor::i18n::tr;
use crate::editor::theme;
use crate::editor::ui::widgets;

use egui_phosphor::regular as icons;

pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale: Locale = app.cli.locale;
    let mut save = false;
    let mut open_node = false;

    egui::Frame::new()
        .fill(theme::PANEL_BG)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            let draft = &mut app.left_sidebar.tooltip_draft;
            field(ui, &tr(locale, "Nombre", "Name"), &mut draft.name);

            // Activación y uso único comparten línea, mitad para cada una.
            ui.columns(2, |columns| {
                columns[0].add_space(6.0);
                columns[0].label(
                    egui::RichText::new(tr(locale, "Modo de activación", "Activation mode"))
                        .size(11.0)
                        .color(theme::TEXT_MUTED),
                );
                // Reserva la misma altura que el label de la izquierda.
                columns[1].label(egui::RichText::new(" ").size(11.0));
                columns[0].horizontal(|ui| {
                    ui.radio_value(
                        &mut draft.trigger,
                        "press".to_string(),
                        tr(locale, "Pulsar", "Press"),
                    );
                    ui.radio_value(
                        &mut draft.trigger,
                        "hold".to_string(),
                        tr(locale, "Mantener", "Hold"),
                    );
                });
                let once =
                    columns[1].checkbox(&mut draft.once, tr(locale, "Solo una vez", "Only once"));
                once.on_hover_text(tr(
                    locale,
                    "Tras usarse una vez, la entidad ya no responde en esta sesión.",
                    "After one use, the entity no longer responds in this session.",
                ));
            });

            // Tecla 25%, acción 37,5%, script/nodo 37,5%. `columns` reparte a
            // partes iguales, así que se reparten los anchos a mano.
            ui.horizontal(|ui| {
                let gap = ui.spacing().item_spacing.x;
                let total = ui.available_width() - 2.0 * gap;
                let key_w = total * 0.15;
                let action_w = total * 0.375;
                let script_w = total * 0.475;
                const ROW_H: f32 = 56.0;
                let top = egui::Layout::top_down(egui::Align::Min);

                ui.allocate_ui_with_layout(egui::vec2(key_w, ROW_H), top, |ui| {
                    key_field(ui, &tr(locale, "Tecla", "Key"), &mut draft.key);
                });

                ui.allocate_ui_with_layout(egui::vec2(action_w, ROW_H), top, |ui| {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(tr(locale, "Tipo de acción", "Action type"))
                            .size(11.0)
                            .color(theme::TEXT_MUTED),
                    );
                    egui::ComboBox::from_id_salt("tooltip_action")
                        .width(ui.available_width())
                        .selected_text(match draft.action.as_str() {
                            "animation" => tr(locale, "Animación", "Animation"),
                            "node" => tr(locale, "Nodos", "Nodes"),
                            _ => tr(locale, "Script Rhai", "Rhai script"),
                        })
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                ("script", tr(locale, "Script Rhai", "Rhai script")),
                                ("animation", tr(locale, "Animación", "Animation")),
                                ("node", tr(locale, "Nodos", "Nodes")),
                            ] {
                                ui.selectable_value(&mut draft.action, value.to_string(), label);
                            }
                        });
                });

                ui.allocate_ui_with_layout(egui::vec2(script_w, ROW_H), top, |third| {
                    let is_script = draft.action != "node" && draft.action != "animation";
                    let is_node = draft.action == "node";
                    let is_animation = draft.action == "animation";
                    let has_ref = !draft.action_ref_name.trim().is_empty();
                    third.add_space(6.0);
                    if is_animation {
                        third.label(
                            egui::RichText::new(tr(locale, "ID de animación", "Animation ID"))
                                .size(11.0)
                                .color(theme::TEXT_MUTED),
                        );
                        third.add(
                            egui::TextEdit::singleline(&mut draft.animation_id)
                                .desired_width(f32::INFINITY)
                                .hint_text(tr(locale, "ID de animación", "Animation ID")),
                        );
                    } else if is_script || is_node {
                        third.label(
                            egui::RichText::new(if is_script {
                                tr(locale, "Script", "Script")
                            } else {
                                tr(locale, "Nodo", "Node")
                            })
                            .size(11.0)
                            .color(theme::TEXT_MUTED),
                        );
                        if !has_ref {
                            let label = if is_script {
                                tr(locale, "Cargar script", "Load script")
                            } else {
                                tr(locale, "Crear nodo", "Create node")
                            };
                            if widgets::modal_button(
                                third,
                                if is_script {
                                    icons::FILE_CODE
                                } else {
                                    icons::NETWORK
                                },
                                &label,
                                widgets::ModalButtonStyle::Secondary,
                                third.available_width(),
                                true,
                            ) {
                                if is_script {
                                    load_script_into(draft);
                                } else {
                                    open_node = true;
                                }
                            }
                        } else {
                            egui::Frame::new()
                                .inner_margin(egui::Margin::symmetric(8, 4))
                                .corner_radius(4)
                                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                                .show(third, |ui| {
                                    ui.horizontal(|ui| {
                                        let w = (ui.available_width() - 44.0).max(0.0);
                                        ui.add_sized(
                                            [w, 20.0],
                                            egui::Label::new(
                                                egui::RichText::new(&draft.action_ref_name)
                                                    .size(12.0)
                                                    .color(theme::TEXT),
                                            )
                                            .truncate(),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.spacing_mut().item_spacing.x = 2.0;
                                                if widgets::icon_action(
                                                    ui,
                                                    icons::TRASH,
                                                    18.0,
                                                    theme::DANGER,
                                                    &tr(locale, "Quitar", "Remove"),
                                                ) {
                                                    app.left_sidebar
                                                        .pending_remove_tooltip_script = true;
                                                }
                                                if widgets::icon_action(
                                                    ui,
                                                    icons::PENCIL,
                                                    18.0,
                                                    theme::INFO,
                                                    &tr(locale, "Editar", "Edit"),
                                                ) {
                                                    if is_script {
                                                        app.left_sidebar.tooltip_script_editor =
                                                            Some((
                                                                draft.action_ref_name.clone(),
                                                                draft.action_ref_source.clone(),
                                                            ));
                                                    } else {
                                                        open_node = true;
                                                    }
                                                }
                                            },
                                        );
                                    });
                                });
                        }
                    }
                });
            });

            // Distancias en la misma línea.
            ui.columns(2, |columns| {
                field(
                    &mut columns[0],
                    &tr(locale, "Distancia de spawn min", "Spawn distance min"),
                    &mut draft.max_distance,
                );
                field(
                    &mut columns[1],
                    &tr(locale, "Distancia de spawn max", "Spawn distance max"),
                    &mut draft.tooltip_distance,
                );
            });

            section_label(ui, &tr(locale, "Aura de resaltado", "Highlight aura"));
            ui.columns(2, |columns| {
                columns[0].checkbox(
                    &mut draft.highlight,
                    tr(locale, "Resaltar entidad", "Highlight entity"),
                );
                columns[1].horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(tr(locale, "Highlight color", "Highlight color"))
                            .size(11.0)
                            .color(theme::TEXT_MUTED),
                    );
                    ui.color_edit_button_srgb(&mut draft.highlight_color);
                });
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            let can_save = !draft.name.trim().is_empty();
            if widgets::modal_button_bold(
                ui,
                icons::FLOPPY_DISK,
                &tr(locale, "Actualizar tooltip", "Update tooltip"),
                widgets::ModalButtonStyle::Primary,
                ui.available_width(),
                can_save,
            ) {
                save = true;
            }
        });

    if open_node {
        open_node_editor(app);
    }

    if save {
        let draft = app.left_sidebar.tooltip_draft.clone();
        let id = if draft.id.trim().is_empty() {
            format!(
                "tt_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or_default()
            )
        } else {
            draft.id.clone()
        };
        let mut value = draft.to_value();
        value["id"] = Value::String(id.clone());
        let list = &mut app.project_state.interaction_tooltips;
        if let Some(existing) = list
            .iter_mut()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(id.as_str()))
        {
            *existing = value;
        } else {
            list.push(value);
        }
        app.send_engine(crate::editor::ui::sidebar_left::sync_command(app));
        app.left_sidebar.tooltip_modal_open = false;
        app.left_sidebar.tooltip_draft = Default::default();
        app.push_log("[Tooltip] Guardado", false);
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(label)
            .size(11.0)
            .color(theme::TEXT_MUTED),
    );
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .hint_text(label),
    );
    ui.add_space(6.0);
}

fn key_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    *value = value.to_uppercase();
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(label)
            .size(11.0)
            .color(theme::TEXT_MUTED),
    );
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .hint_text(label),
    );
    *value = value.to_uppercase();
}

fn section_label(ui: &mut egui::Ui, label: &str) {
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(label)
            .size(11.0)
            .strong()
            .color(theme::TEXT),
    );
    ui.add_space(2.0);
}

/// Abre el diálogo nativo para cargar un script Rhai en el tooltip.
fn load_script_into(draft: &mut crate::editor::ui::sidebar_left::TooltipDraft) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Script Rhai", &["rhai", "rs"])
        .pick_file()
    else {
        return;
    };
    let Ok(source) = std::fs::read_to_string(&path) else {
        return;
    };
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("script")
        .to_string();
    draft.action_ref_name = name;
    draft.action_ref_source = source;
}

/// Abre el editor de nodos apuntando al tooltip que se está editando.
fn open_node_editor(app: &mut EditorApp) {
    let draft = &app.left_sidebar.tooltip_draft;
    let name = if draft.action_ref_name.trim().is_empty() {
        draft.name.clone()
    } else {
        draft.action_ref_name.clone()
    };
    let tooltip_id = draft.id.clone();
    app.left_sidebar.tooltip_draft.action_ref_name = name.clone();
    app.visual_script.open = true;
    app.visual_script.context = crate::editor::visual_script::GraphContext::Entity;
    app.visual_script.entity_id = None;
    app.visual_script.scene_id = None;
    app.visual_script.tooltip_target = Some(tooltip_id);
    app.visual_script.tooltip_name = name;
    app.visual_script.loaded = false;
    app.visual_script.error = None;
}
