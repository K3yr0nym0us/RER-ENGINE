//! Contenido de la ventana nativa de configuración de sockets.
//!
//! Sustituye al `SocketConfigModalElectronHost`. Los sockets, huesos y el hueso
//! elegido en el viewport llegan como eventos del motor a `SocketConfigState`.
//! Las acciones envían los mismos comandos que enviaba React.

use egui::RichText;
use egui_phosphor::regular as icons;
use serde_json::{Value, json};

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::widgets;

fn text_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Dibuja un recuadro con borde, igual que los `.border.rounded.p-2` de React.
fn bordered(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::same(8))
        .show(ui, content);
}

/// Dibuja la ventana de sockets con el mismo diseño que el modal de React.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;

    // Si no hay entidad elegida, tomar la selección del viewport en cuanto exista.
    if app.socket_config.target_id.is_none()
        && let Some(selected) = app.selected_entity.as_ref()
    {
        app.socket_config.target_id = Some(selected.id);
        app.socket_config.target_name = selected.name.clone();
        if let Some(id) = app.socket_config.target_id {
            app.send_engine(json!({ "cmd": "list_entity_sockets", "entity_id": id }));
            app.send_engine(json!({ "cmd": "list_entity_bones", "entity_id": id }));
        }
    }

    // Vinculación pendiente: el siguiente clic en el viewport elige la entidad hija.
    if let (Some(socket), Some(child)) = (
        app.socket_config.pending_link_socket.clone(),
        app.selected_entity.as_ref(),
    ) {
        let host = app.socket_config.target_id;
        if host.is_some_and(|id| id != child.id) {
            app.send_engine(json!({
                "cmd": "attach_to_socket",
                "child_ids": [child.id],
                "host_id": host,
                "socket_name": socket,
            }));
            app.socket_config.pending_link_socket = None;
        }
    }

    let mut tab = app.socket_config.links_tab;
    let mut form_open = app.socket_config.form_open;
    let entity_id = app.socket_config.target_id.unwrap_or(0);
    let sockets = app.socket_config.sockets.clone();
    let picked_bone = app.socket_config.picked_bone.clone();
    let bone_pick_active = app.socket_config.bone_pick_active;
    let pending_link = app.socket_config.pending_link_socket.clone();
    let mut name_draft = app.socket_config.name_draft.clone();
    let mut selected_bone = app.socket_config.selected_bone.clone();
    let mut commands: Vec<Value> = Vec::new();

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL_BG)
                .inner_margin(egui::Margin::same(12)),
        )
        .show(ui, |ui| {
            // El contenido puede crecer (formulario, listas): scroll solo si la
            // ventana alcanza el tope de altura permitido.
            egui::ScrollArea::vertical()
                .id_salt("socket_config_scroll")
                .show(ui, |ui| {
            // Cabecera: personaje seleccionado + badge con el número de asignaciones.
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{}: ",
                        tr(locale, "Personaje seleccionado", "Selected character")
                    ))
                    .size(12.0)
                    .color(theme::TEXT_MUTED),
                );
                ui.label(
                    RichText::new(&app.socket_config.target_name)
                        .size(12.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let badge = format!(
                        "{} {}",
                        sockets.len(),
                        tr(locale, "asignaciones", "assignments")
                    );
                    let font = egui::FontId::proportional(11.0);
                    let width = ui.painter().layout_no_wrap(badge.clone(), font.clone(), theme::TEXT).size().x + 12.0;
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(width, 20.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(
                        rect,
                        egui::CornerRadius::same(4),
                        theme::OUTLINE_SECONDARY,
                    );
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        badge,
                        font,
                        theme::TEXT,
                    );
                });
            });
            ui.add_space(8.0);

            // Pestañas con icono, como los Nav.Link del renderer.
            ui.horizontal(|ui| {
                if widgets::modal_tab(
                    ui,
                    &format!("{} {}", icons::CUBE, tr(locale, "Crear sockets", "Create sockets")),
                    !tab,
                ) {
                    tab = false;
                }
                if widgets::modal_tab(
                    ui,
                    &format!("{} {}", icons::LINK, tr(locale, "Vincular sockets", "Link sockets")),
                    tab,
                ) {
                    tab = true;
                }
            });
            ui.add_space(8.0);

            if app.socket_config.target_id.is_some() {
            if !tab {
                // Botón "Añadir socket" a lo ancho, como el original.
                if widgets::wide_button_click_sized(
                    ui,
                    ui.available_width(),
                    icons::PLUS,
                    tr(locale, "Añadir socket", "Add Socket"),
                    "",
                    theme::INFO,
                ) {
                    form_open = !form_open;
                }
                ui.add_space(8.0);

                if form_open {
                    bordered(ui, |ui| {
                        ui.label(
                            RichText::new(tr(locale, "Nombre", "Name"))
                                .size(12.0)
                                .color(theme::TEXT_MUTED),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut name_draft)
                                .desired_width(f32::INFINITY)
                                .hint_text(tr(locale, "Nombre del socket", "Socket name")),
                        );
                        ui.add_space(6.0);

                        // El hueso es de solo lectura; se elige en el viewport.
                        ui.label(
                            RichText::new(tr(locale, "Hueso", "Bone"))
                                .size(12.0)
                                .color(theme::TEXT_MUTED),
                        );
                        let bone = picked_bone
                            .clone()
                            .or_else(|| selected_bone.clone())
                            .unwrap_or_default();
                        egui::Frame::new()
                            .fill(theme::ACCORDION_BG)
                            .inner_margin(egui::Margin::same(6))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(if bone.is_empty() {
                                        tr(locale, "Ningún hueso seleccionado", "No bone selected")
                                    } else {
                                        &bone
                                    })
                                    .size(12.0)
                                    .color(if bone.is_empty() {
                                        theme::TEXT_MUTED
                                    } else {
                                        theme::TEXT
                                    }),
                                );
                            });
                        if widgets::wide_button_click_sized(
                            ui,
                            ui.available_width(),
                            icons::CROSSHAIR,
                            tr(
                                locale,
                                if bone_pick_active {
                                    "Cancelar selección de hueso"
                                } else {
                                    "Seleccionar hueso"
                                },
                                if bone_pick_active {
                                    "Cancel bone selection"
                                } else {
                                    "Select bone"
                                },
                            ),
                            "",
                            if bone_pick_active {
                                theme::WARNING
                            } else {
                                theme::INFO
                            },
                        ) {
                            commands.push(json!({
                                "cmd": "set_socket_bone_pick_mode",
                                "entity_id": entity_id,
                                "active": !bone_pick_active,
                            }));
                        }
                        if bone_pick_active {
                            ui.label(
                                RichText::new(tr(
                                    locale,
                                    "Haz clic en un hueso del viewport para asignarlo.",
                                    "Click a bone in the viewport to assign it.",
                                ))
                                .size(12.0)
                                .color(theme::WARNING),
                            );
                        }
                        ui.add_space(6.0);
                        if widgets::modal_button(
                            ui,
                            icons::FLOPPY_DISK,
                            tr(locale, "Guardar", "Save"),
                            widgets::ModalButtonStyle::Primary,
                            100.0,
                            !name_draft.trim().is_empty() && !bone.is_empty(),
                        ) {
                            commands.push(json!({
                                "cmd": "upsert_entity_socket",
                                "entity_id": entity_id,
                                "name": name_draft.trim(),
                                "bone_name": bone,
                                "local_position": [0.0, 0.0, 0.0],
                                "local_rotation": [0.0, 0.0, 0.0, 1.0],
                            }));
                            name_draft.clear();
                            form_open = false;
                        }
                    });
                    ui.add_space(8.0);
                }

                // Lista de sockets: fila con nombre/hueso + editar/borrar.
                for socket in &sockets {
                    bordered(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(&socket.name)
                                        .size(13.0)
                                        .strong()
                                        .color(theme::TEXT),
                                );
                                ui.label(
                                    RichText::new(&socket.bone_name)
                                        .size(12.0)
                                        .color(theme::TEXT_MUTED),
                                );
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if widgets::small_icon_button_click(
                                    ui,
                                    icons::TRASH,
                                    tr(locale, "Eliminar", "Remove"),
                                ) {
                                    commands.push(json!({
                                        "cmd": "remove_entity_socket",
                                        "entity_id": entity_id,
                                        "name": socket.name,
                                    }));
                                }
                                if widgets::small_icon_button_click(
                                    ui,
                                    icons::PENCIL,
                                    tr(locale, "Editar", "Edit"),
                                ) {
                                    name_draft = socket.name.clone();
                                    selected_bone = Some(socket.bone_name.clone());
                                    app.socket_config.picked_bone = None;
                                    form_open = true;
                                }
                            });
                        });
                    });
                    ui.add_space(6.0);
                }
            } else {
                ui.label(
                    RichText::new(format!("{}: ", tr(locale, "Host", "Host")))
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                );
                ui.label(
                    RichText::new(&app.socket_config.target_name)
                        .size(12.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.add_space(8.0);

                if let Some(socket) = pending_link.as_ref() {
                    egui::Frame::new()
                        .fill(theme::ACCORDION_BG)
                        .stroke(egui::Stroke::new(1.0, theme::INFO))
                        .corner_radius(egui::CornerRadius::same(4))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(tr(
                                        locale,
                                        "Haz clic en el objeto del viewport para vincularlo a",
                                        "Click the object in the viewport to link it to",
                                    ))
                                    .size(12.0)
                                    .color(theme::TEXT),
                                );
                                ui.label(
                                    RichText::new(socket).size(12.0).strong().color(theme::INFO),
                                );
                                if widgets::small_icon_button_click(
                                    ui,
                                    icons::X,
                                    tr(locale, "Cancelar", "Cancel"),
                                ) {
                                    app.socket_config.pending_link_socket = None;
                                }
                            });
                        });
                    ui.add_space(8.0);
                }

                if sockets.is_empty() {
                    ui.label(
                        RichText::new(tr(
                            locale,
                            "No hay sockets en esta entidad. Usa Crear sockets primero.",
                            "No sockets on this entity. Use Create Sockets first.",
                        ))
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                    );
                }

                if !sockets.is_empty() {
                for socket in &sockets {
                    bordered(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(&socket.name)
                                        .size(13.0)
                                        .strong()
                                        .color(theme::TEXT),
                                );
                                ui.label(
                                    RichText::new(&socket.bone_name)
                                        .size(12.0)
                                        .color(theme::TEXT_MUTED),
                                );
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if widgets::wide_button_click_sized(
                                    ui,
                                    110.0,
                                    icons::LINK,
                                    tr(locale, "Vincular objeto", "Link object"),
                                    "",
                                    theme::INFO,
                                ) {
                                    app.socket_config.pending_link_socket = Some(socket.name.clone());
                                }
                            });
                        });
                        // Objetos vinculados según el metadata de entidades (React).
                        let linked: Vec<(String, String)> = app
                            .project_state
                            .entity_meta
                            .iter()
                            .filter(|(_, meta)| {
                                meta.get("attachSocketHostId").and_then(Value::as_u64)
                                    == Some(entity_id)
                                    && meta.get("attachSocketName").and_then(Value::as_str)
                                        == Some(socket.name.as_str())
                            })
                            .map(|(child_id, meta)| {
                                let name = text_field(meta, "name");
                                (
                                    if name.is_empty() { child_id.clone() } else { name },
                                    child_id.clone(),
                                )
                            })
                            .collect();
                        ui.add_space(4.0);
                        if linked.is_empty() {
                            ui.label(
                                RichText::new(tr(locale, "Sin objetos vinculados", "No linked objects"))
                                    .size(12.0)
                                    .color(theme::TEXT_MUTED),
                            );
                        } else {
                            for (child_name, child_id) in linked {
                                ui.horizontal(|ui| {
                                    ui.label(child_name);
                                    if widgets::small_icon_button_click(
                                        ui,
                                        icons::LINK_BREAK,
                                        tr(locale, "Desvincular", "Unlink"),
                                    )
                                        && let Ok(id) = child_id.parse::<u64>() {
                                            commands.push(json!({
                                                "cmd": "detach_from_socket",
                                                "child_id": id,
                                            }));
                                        }
                                });
                            }
                        }
                    });
                    ui.add_space(6.0);
                }
                }
            }
            }

            // Cuadro de flujo al final de la ventana (debajo de todo el contenido).
            ui.add_space(10.0);
            egui::Frame::new()
                .fill(theme::ACCORDION_BG)
                .stroke(egui::Stroke::new(1.0, theme::INFO))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(icons::INFO).size(14.0).color(theme::INFO));
                        ui.label(
                            RichText::new(tr(locale, "Flujo de sockets", "Socket workflow"))
                                .size(12.0)
                                .strong()
                                .color(theme::INFO),
                        );
                    });
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(tr(
                            locale,
                            "1. Selecciona una entidad en el viewport (esta ventana se queda encima).",
                            "1. Select an entity in the viewport (this window stays on top).",
                        ))
                        .size(12.0)
                        .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new(tr(
                            locale,
                            "2. Añade un socket, escribe el nombre y elige el hueso en el viewport.",
                            "2. Add a socket, set its name and pick the bone in the viewport.",
                        ))
                        .size(12.0)
                        .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new(tr(
                            locale,
                            "3. En Vincular sockets, elige un socket y haz clic en la entidad hija.",
                            "3. In Link sockets, pick a socket and click the child entity.",
                        ))
                        .size(12.0)
                        .color(theme::TEXT),
                    );
                });
            app.socket_config.content_height = ui.cursor().min.y + 16.0;
                });
        });

    app.socket_config.links_tab = tab;
    app.socket_config.form_open = form_open;
    app.socket_config.name_draft = name_draft;
    app.socket_config.selected_bone = selected_bone;
    for command in commands {
        app.send_engine(command);
    }
}
