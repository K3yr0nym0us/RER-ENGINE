//! Cuerpo de la ventana nativa del editor visual de lógica (nodos).
//!
//! Es el reemplazo de `VisualScriptingModalBody` + `VisualScriptCanvas` de
//! React. La distribución es la misma: barra de botones de nodos, panel de
//! referencias a la izquierda (240 px), lienzo en el centro y inspector del
//! nodo seleccionado a la derecha (220/240 px), con "Guardar y aplicar" abajo.

use egui::{
    Align, Color32, CornerRadius, FontId, Layout, Margin, RichText, Sense, Stroke, StrokeKind, Ui,
    Vec2,
};
use egui_phosphor::regular as icons;
use egui_snarl::{
    NodeId, Snarl,
    ui::{SnarlWidget, get_selected_nodes},
};
use serde_json::json;

use crate::editor::app::EditorApp;
use crate::editor::cli::Locale;
use crate::editor::i18n::tr;
use crate::editor::theme;
use crate::editor::visual_script::viewer::VisualViewer;
use crate::editor::visual_script::{
    GraphContext, NodeKind, VariableGroup, VariableItem, compile, load_document, scene_entities,
    variable_groups, viewer,
};

const SNARL_ID: &str = "rer_visual_script_snarl";

/// Dibuja la modal en su ventana nativa.
pub(crate) fn show_in_window(ui: &mut Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    let tr_text = |es: &'static str, en: &'static str| tr(locale, es, en);

    // El snapshot de React puede llegar después de abrir la ventana.
    if !app.visual_script.loaded {
        let (doc, scene_name, entity_name) = load_document(app);
        app.visual_script.doc = doc;
        app.visual_script.scene_name = scene_name;
        app.visual_script.entity_name = entity_name;
        app.visual_script.loaded = true;
    }

    let context = app.visual_script.context;
    let scene_id = app.visual_script.scene_id.unwrap_or(0);
    let entity_id = app.visual_script.entity_id.unwrap_or(0);
    let title_name = if context == GraphContext::Entity {
        app.visual_script.entity_name.clone()
    } else {
        app.visual_script.scene_name.clone()
    };
    let error_text = app
        .visual_script
        .error
        .as_deref()
        .map(|error| compile::localize_issue(error, locale));
    let groups = variable_groups(app, locale);
    let entities = scene_entities(app);
    let scene_ids: Vec<u64> = entities.iter().map(|entity| entity.id).collect();
    let mut open_groups = app.visual_script.open_groups.clone();
    let mut selected_uid = app.visual_script.selected.clone();
    let mut seq = app.visual_script.seq;
    let mut viewer = VisualViewer {
        locale,
        context,
        seq,
    };
    let doc = &mut app.visual_script.doc;
    let mut save = false;

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL_BG)
                .inner_margin(Margin::same(10)),
        )
        .show(ui, |ui| {
            // React: `Entity: <name> (id X)` / `Scene: <name> (id X)`.
            let (label, id) = if context == GraphContext::Entity {
                (tr_text("Entidad", "Entity"), entity_id)
            } else {
                (tr_text("Escena", "Scene"), scene_id)
            };
            if scene_id > 0 || entity_id > 0 {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{label}: "))
                            .size(12.0)
                            .color(theme::TEXT_MUTED),
                    );
                    ui.label(
                        RichText::new(if title_name.is_empty() {
                            id.to_string()
                        } else {
                            title_name.clone()
                        })
                        .size(12.0)
                        .strong()
                        .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new(format!("(id {id})"))
                            .size(12.0)
                            .color(theme::TEXT_MUTED),
                    );
                });
                ui.add_space(6.0);
            }

            if let Some(error) = &error_text {
                egui::Frame::new()
                    .fill(theme::DANGER.linear_multiply(0.18))
                    .stroke(Stroke::new(1.0, theme::DANGER))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.label(RichText::new(error).size(12.0).color(theme::TEXT));
                    });
                ui.add_space(6.0);
            }

            ui.vertical(|ui| {
                // Barra de botones: los mismos tipos que `nodesForContext`.
                ui.horizontal_wrapped(|ui| {
                    for kind in NodeKind::ALL {
                        if !kind.available_in(context) {
                            continue;
                        }
                        let label = format!("+ {}", kind.label(locale));
                        if outline_node_button(ui, &label, viewer::category_color(kind.category()))
                        {
                            seq = seq.wrapping_add(1);
                            let uid = format!(
                                "n_rust_{}_{}",
                                crate::editor::visual_script::chrono_stamp(),
                                seq
                            );
                            let pos = egui::pos2(
                                80.0 + doc.graph.nodes().count() as f32 * 24.0,
                                80.0 + doc.graph.nodes().count() as f32 * 24.0,
                            );
                            doc.graph.insert_node(
                                pos,
                                crate::editor::visual_script::VisualNode::new(
                                    kind,
                                    uid,
                                    kind.default_data(),
                                ),
                            );
                        }
                    }
                });
                ui.add_space(6.0);

                let main_height = ui.available_height().max(240.0) - 40.0;

                ui.horizontal(|ui| {
                    // Panel izquierdo de referencias (ancho fijo 240 px).
                    ui.allocate_ui_with_layout(
                        Vec2::new(240.0, main_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            variables_panel(
                                ui,
                                &groups,
                                &mut open_groups,
                                locale,
                                context,
                                &entities,
                                doc,
                                selected_uid.as_deref(),
                            );
                        },
                    );

                    // Lienzo central: el `Snarl` ocupa todo el espacio restante.
                    let canvas_width = ui.available_width() - 8.0;
                    ui.allocate_ui_with_layout(
                        Vec2::new(canvas_width.max(240.0), main_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            let canvas = egui::Frame::new()
                                .stroke(Stroke::new(1.0, theme::BORDER))
                                .corner_radius(CornerRadius::same(5))
                                .inner_margin(Margin::ZERO);
                            canvas.show(ui, |ui| {
                                let response = SnarlWidget::new()
                                    .id(egui::Id::new(SNARL_ID))
                                    .style(viewer::snarl_style())
                                    .show(&mut doc.graph, &mut viewer, ui);
                                draw_minimap(ui, response.rect, &doc.graph);

                                let selected =
                                    get_selected_nodes(egui::Id::new(SNARL_ID), ui.ctx());
                                selected_uid = selected
                                    .first()
                                    .and_then(|id| doc.graph.get_node(*id))
                                    .map(|node| node.uid.clone());
                            });
                        },
                    );

                    // Inspector derecho, solo para los nodos que tienen campos.
                    let selected_kind = selected_uid.as_deref().and_then(|uid| {
                        doc.graph
                            .node_ids()
                            .find(|(_, node)| node.uid == *uid)
                            .map(|(_, node)| node.kind)
                    });
                    if let Some(kind) = inspector_kind(selected_kind) {
                        let width = if matches!(
                            kind,
                            NodeKind::SetScale
                                | NodeKind::MoveTo
                                | NodeKind::Translate
                                | NodeKind::PlayAnimation
                                | NodeKind::SetReflectionTier
                        ) {
                            240.0
                        } else {
                            220.0
                        };
                        ui.add_space(8.0);
                        ui.allocate_ui_with_layout(
                            Vec2::new(width, main_height),
                            Layout::top_down(Align::Min),
                            |ui| {
                                inspector(
                                    ui,
                                    doc,
                                    selected_uid.as_deref(),
                                    kind,
                                    &entities,
                                    locale,
                                );
                            },
                        );
                    }
                });

                ui.add_space(10.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets_save_button(ui, locale) {
                        save = true;
                    }
                });
            });
        });

    app.visual_script.seq = seq.max(viewer.seq);
    app.visual_script.open_groups = open_groups;
    app.visual_script.selected = selected_uid;

    if save {
        app.visual_script.error = None;
        let graph = app.visual_script.doc.to_json();
        match compile::compile(&app.visual_script.doc, &scene_ids) {
            Err(errors) => {
                app.visual_script.error = Some(errors.join(" · "));
            }
            Ok(source) => {
                persist(app, graph, source);
                app.visual_script.open = false;
                app.visual_script.loaded = false;
            }
        }
    }
}

/// Tipos de nodo que muestran inspector (los mismos que el JSX de React).
fn inspector_kind(kind: Option<NodeKind>) -> Option<NodeKind> {
    kind.filter(|kind| {
        matches!(
            kind,
            NodeKind::Log
                | NodeKind::If
                | NodeKind::ForRepeat
                | NodeKind::ForEachEntity
                | NodeKind::SetScale
                | NodeKind::MoveTo
                | NodeKind::Translate
                | NodeKind::SetReflectionTier
                | NodeKind::PlayAnimation
        )
    })
}

/// Persiste el grafo y aplica el Rhai al motor, igual que `saveSceneVisualGraph`.
fn persist(app: &mut EditorApp, graph: serde_json::Value, source: String) {
    // Guardado desde el formulario de un tooltip: el grafo y el Rhai compilado
    // viven en la definición compartida, no en la entidad ni en la escena.
    if let Some(tooltip_id) = app.visual_script.tooltip_target.clone() {
        for tooltip in app.project_state.interaction_tooltips.iter_mut() {
            if tooltip.get("id").and_then(serde_json::Value::as_str) == Some(tooltip_id.as_str()) {
                tooltip["action_ref_graph"] = graph.clone();
                tooltip["action_ref_source"] = serde_json::Value::String(source.clone());
                let name = tooltip
                    .get("action_ref_name")
                    .and_then(serde_json::Value::as_str)
                    .filter(|n| !n.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| app.visual_script.tooltip_name.clone());
                tooltip["action_ref_name"] = serde_json::Value::String(name);
            }
        }
        app.left_sidebar.tooltip_draft.action_ref_graph = graph;
        app.left_sidebar.tooltip_draft.action_ref_source = source;
        if app.left_sidebar.tooltip_draft.action_ref_name.is_empty() {
            app.left_sidebar.tooltip_draft.action_ref_name = app.visual_script.tooltip_name.clone();
        }
        app.send_engine(crate::editor::ui::sidebar_left::sync_command(app));
        return;
    }

    let context = app.visual_script.context;
    let scene_id = app.visual_script.scene_id.unwrap_or(0);
    let entity_id = app.visual_script.entity_id.unwrap_or(0);
    let active = app.project_state.active_scene_id == Some(scene_id);

    if context == GraphContext::Scene {
        let update = |scenes: &mut Vec<serde_json::Value>| {
            for scene in scenes.iter_mut() {
                if scene.get("id").and_then(serde_json::Value::as_u64) == Some(scene_id) {
                    scene["visualGraph"] = graph.clone();
                    scene["visualScriptRhai"] = json!(source);
                }
            }
        };
        update(&mut app.project_state.scenes);
        if let Some(pending) = app.pending_save.as_mut()
            && let Some(state) = pending.state.as_mut()
        {
            update(&mut state.scenes);
        }
        if active {
            app.send_engine(json!({
                "cmd": "load_scene_visual_script",
                "scene_id": scene_id,
                "source": source,
            }));
        }
    } else if let Some(meta) = app
        .project_state
        .entity_meta
        .get_mut(&entity_id.to_string())
    {
        meta["visualGraph"] = graph;
        meta["visualScriptRhai"] = json!(source);
        let mut scripts = meta
            .get("scripts")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        scripts.retain(|script| {
            script.get("name").and_then(serde_json::Value::as_str) != Some("visual_logic")
        });
        scripts.push(json!({ "name": "visual_logic", "source": source }));
        meta["scripts"] = json!(scripts);
        app.send_engine(json!({
            "cmd": "load_script",
            "id": entity_id,
            "path": "visual_logic",
            "source": source,
        }));
    }
}

/// Botón de contorno con el color de la categoría (Bootstrap `btn-outline-*`).
fn outline_node_button(ui: &mut Ui, label: &str, color: Color32) -> bool {
    let font = FontId::proportional(12.5);
    let text_width = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), theme::TEXT)
        .size()
        .x;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(text_width + 16.0, 28.0), Sense::click());
    let hovered = response.hovered();
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        if hovered {
            color.linear_multiply(0.22)
        } else {
            Color32::TRANSPARENT
        },
        Stroke::new(1.0, color),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        font,
        if hovered { Color32::WHITE } else { color },
    );
    response.clicked()
}

/// Panel izquierdo: acordeones con variables y referencias copiables.
fn variables_panel(
    ui: &mut Ui,
    groups: &[VariableGroup],
    open: &mut std::collections::BTreeSet<String>,
    locale: Locale,
    context: GraphContext,
    entities: &[crate::editor::visual_script::SceneEntity],
    doc: &mut crate::editor::visual_script::GraphDocument,
    selected: Option<&str>,
) {
    let title = if context == GraphContext::Entity {
        tr(locale, "Variables de entidad", "Entity variables")
    } else {
        tr(locale, "Referencias de escena", "Scene references")
    };
    ui.label(
        RichText::new(title)
            .size(13.0)
            .strong()
            .color(Color32::from_rgb(0xf0, 0xf4, 0xfa)),
    );
    ui.add_space(6.0);

    egui::ScrollArea::vertical()
        .id_salt("visual_script_variables_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if groups.is_empty() {
                ui.label(
                    RichText::new(tr(
                        locale,
                        "No hay variables disponibles",
                        "No variables available",
                    ))
                    .size(12.0)
                    .color(Color32::from_rgb(0x9a, 0xa8, 0xbc)),
                );
            }
            for group in groups {
                let is_open = open.contains(&group.key);
                let response =
                    egui::CollapsingHeader::new(RichText::new(&group.label).size(12.0).strong())
                        .id_salt(&group.key)
                        .default_open(is_open)
                        .show(ui, |ui| {
                            for nested in &group.nested {
                                egui::CollapsingHeader::new(
                                    RichText::new(&nested.label).size(12.0),
                                )
                                .id_salt(&nested.key)
                                .default_open(open.contains(&nested.key))
                                .show(ui, |ui| {
                                    variable_list(ui, &nested.items, locale, doc, selected);
                                });
                            }
                            variable_list(ui, &group.items, locale, doc, selected);
                        });
                if response.body_response.is_some() {
                    if is_open {
                        open.remove(&group.key);
                    } else {
                        open.insert(group.key.clone());
                    }
                }
            }
            let _ = entities;
        });
}

fn variable_list(
    ui: &mut Ui,
    items: &[VariableItem],
    locale: Locale,
    doc: &mut crate::editor::visual_script::GraphDocument,
    selected: Option<&str>,
) {
    for item in items {
        let snippet = if item.snippet.is_empty() {
            item.label.clone()
        } else {
            item.snippet.clone()
        };
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().min(220.0), 38.0),
            Sense::click(),
        );
        let hovered = response.hovered();
        ui.painter().rect(
            rect,
            CornerRadius::same(4),
            if hovered {
                Color32::from_rgb(0x2b, 0x30, 0x35)
            } else {
                Color32::from_rgb(0x21, 0x25, 0x29)
            },
            Stroke::new(
                1.0,
                if hovered {
                    theme::OUTLINE_SECONDARY
                } else {
                    theme::TEXT_MUTED
                },
            ),
            StrokeKind::Inside,
        );
        ui.painter().text(
            rect.min + Vec2::new(8.0, 11.0),
            egui::Align2::LEFT_TOP,
            &item.label,
            FontId::proportional(12.0),
            Color32::from_rgb(0xe8, 0xec, 0xf4),
        );
        ui.painter().text(
            rect.min + Vec2::new(8.0, 25.0),
            egui::Align2::LEFT_TOP,
            &snippet,
            FontId::monospace(10.5),
            Color32::from_rgb(0xb8, 0xc4, 0xd6),
        );
        let mut tooltip = String::new();
        if !item.detail.is_empty() {
            tooltip.push_str(&item.detail);
        }
        tooltip.push('\n');
        tooltip.push_str(&tr(
            locale,
            "Clic para copiar a portapapeles",
            "Click to copy snippet to clipboard",
        ));
        let response = response.on_hover_text(tooltip);
        if response.clicked()
            && let Some(copy) = &item.copy
        {
            ui.ctx().copy_text(copy.clone());
            // Igual que React: al elegir una animación se rellena el nodo
            // "Play animation" seleccionado.
            if let Some((entity_id, animation)) = &item.animation
                && let Some(node) = selected.and_then(|uid| doc.node_by_uid(uid))
                && doc.graph[node].kind == crate::editor::visual_script::NodeKind::PlayAnimation
            {
                let data = &mut doc.graph[node].data;
                data.entity_id = *entity_id;
                data.animation_name = animation.clone();
            }
        }
    }
}

/// Inspector derecho, con los mismos campos que `VisualScriptCanvas.tsx`.
fn inspector(
    ui: &mut Ui,
    doc: &mut crate::editor::visual_script::GraphDocument,
    uid: Option<&str>,
    kind: NodeKind,
    entities: &[crate::editor::visual_script::SceneEntity],
    locale: Locale,
) {
    egui::Frame::new()
        .fill(theme::SIDEBAR_BG)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            let Some(uid) = uid else { return };
            let Some(node) = doc.node_by_uid(uid) else {
                return;
            };
            let data = &mut doc.graph[node].data;
            let field = |ui: &mut Ui, label: &'static str, value: &mut String| {
                ui.label(
                    RichText::new(tr(locale, label, label))
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                );
                ui.add(
                    egui::TextEdit::singleline(value)
                        .desired_width(f32::INFINITY)
                        .font(FontId::proportional(12.5)),
                );
            };

            match kind {
                NodeKind::Log => field(ui, "Message", &mut data.message),
                NodeKind::If => field(ui, "Condition (Rhai)", &mut data.expression),
                NodeKind::ForRepeat => {
                    ui.label(
                        RichText::new(tr(locale, "Cantidad", "Count"))
                            .size(12.0)
                            .color(theme::TEXT_MUTED),
                    );
                    let mut count = data.count.to_string();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut count)
                                .desired_width(f32::INFINITY)
                                .font(FontId::proportional(12.5)),
                        )
                        .changed()
                    {
                        data.count = count.parse().unwrap_or(0);
                    }
                }
                NodeKind::ForEachEntity => {
                    ui.label(
                        RichText::new(tr(
                            locale,
                            "Ids de entidad (opcional)",
                            "Entity ids (optional)",
                        ))
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                    );
                    let mut ids = data
                        .entity_ids
                        .iter()
                        .map(u64::to_string)
                        .collect::<Vec<_>>()
                        .join(", ");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut ids)
                                .hint_text(tr(
                                    locale,
                                    "Todas las entidades de la escena",
                                    "All scene entities",
                                ))
                                .desired_width(f32::INFINITY)
                                .font(FontId::proportional(12.5)),
                        )
                        .changed()
                    {
                        data.entity_ids = ids
                            .split(',')
                            .filter_map(|part| part.trim().parse::<u64>().ok())
                            .filter(|id| *id > 0)
                            .collect();
                    }
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(tr(
                            locale,
                            "Variable de bucle: entity_id",
                            "Loop variable: entity_id",
                        ))
                        .size(11.0)
                        .color(theme::TEXT_MUTED),
                    );
                }
                NodeKind::SetScale => {
                    field(ui, "Scale X (Rhai)", &mut data.scale_x);
                    ui.add_space(6.0);
                    field(ui, "Scale Y (Rhai)", &mut data.scale_y);
                }
                NodeKind::MoveTo => {
                    field(ui, "X (Rhai)", &mut data.x);
                    ui.add_space(6.0);
                    field(ui, "Y (Rhai)", &mut data.y);
                }
                NodeKind::Translate => {
                    field(ui, "Delta X (Rhai)", &mut data.dx);
                    ui.add_space(6.0);
                    field(ui, "Delta Y (Rhai)", &mut data.dy);
                }
                NodeKind::SetReflectionTier => {
                    ui.label(
                        RichText::new(tr(locale, "Nivel de reflexión", "Reflection tier"))
                            .size(12.0)
                            .color(theme::TEXT_MUTED),
                    );
                    let mut tier = data.tier.clone();
                    egui::ComboBox::from_id_salt("visual_script_tier")
                        .selected_text(tier.clone())
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                ("off", tr(locale, "Desactivado", "Off")),
                                ("low", tr(locale, "Bajo", "Low")),
                                ("medium", tr(locale, "Medio", "Medium")),
                                ("high", tr(locale, "Alto", "High")),
                            ] {
                                ui.selectable_value(&mut tier, value.to_string(), label);
                            }
                        });
                    data.tier = tier;
                }
                NodeKind::PlayAnimation => {
                    ui.label(
                        RichText::new(tr(locale, "Entidad", "Entity"))
                            .size(12.0)
                            .color(theme::TEXT_MUTED),
                    );
                    let mut entity_id = if data.entity_id == 0 {
                        entities.first().map(|e| e.id).unwrap_or(0)
                    } else {
                        data.entity_id
                    };
                    egui::ComboBox::from_id_salt("visual_script_entity")
                        .selected_text(
                            entities
                                .iter()
                                .find(|e| e.id == entity_id)
                                .map(|e| format!("{} ({})", e.name, e.id))
                                .unwrap_or_else(|| entity_id.to_string()),
                        )
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for entity in entities {
                                ui.selectable_value(
                                    &mut entity_id,
                                    entity.id,
                                    format!("{} ({})", entity.name, entity.id),
                                );
                            }
                        });
                    if entity_id != data.entity_id {
                        data.entity_id = entity_id;
                        let names = crate::editor::visual_script::animation_names_for(
                            entities.iter().find(|e| e.id == entity_id),
                        );
                        if !names.contains(&data.animation_name) {
                            data.animation_name = names.first().cloned().unwrap_or_default();
                        }
                    }
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(tr(locale, "Animación", "Animation"))
                            .size(12.0)
                            .color(theme::TEXT_MUTED),
                    );
                    let names = crate::editor::visual_script::animation_names_for(
                        entities.iter().find(|e| e.id == data.entity_id),
                    );
                    if names.is_empty() {
                        ui.label(
                            RichText::new(tr(
                                locale,
                                "Sin animaciones para esta entidad",
                                "No animations for this entity",
                            ))
                            .size(12.0)
                            .color(theme::TEXT_MUTED),
                        );
                    } else {
                        let mut animation = data.animation_name.clone();
                        egui::ComboBox::from_id_salt("visual_script_animation")
                            .selected_text(animation.clone())
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(&mut animation, name.clone(), name);
                                }
                            });
                        data.animation_name = animation;
                    }
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(tr(
                            locale,
                            "Elige en el panel Animaciones o aquí",
                            "Pick from Animations panel or select here",
                        ))
                        .size(11.0)
                        .color(theme::TEXT_MUTED),
                    );
                }
                _ => {}
            }
        });
}

/// Botón "Guardar y aplicar" (la X de la ventana hace las veces de Cancelar).
fn widgets_save_button(ui: &mut Ui, locale: Locale) -> bool {
    let label = format!(
        "{}  {}",
        icons::FLOPPY_DISK,
        tr(locale, "Guardar y aplicar", "Save and apply")
    );
    let font = FontId::proportional(13.0);
    let width = ui
        .painter()
        .layout_no_wrap(label.clone(), font.clone(), Color32::WHITE)
        .size()
        .x
        + 20.0;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width.max(130.0), 30.0), Sense::click());
    let hovered = response.hovered();
    ui.painter().rect(
        rect,
        CornerRadius::same(6),
        if hovered {
            theme::SUCCESS_HOVER
        } else {
            theme::SUCCESS
        },
        Stroke::NONE,
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        font,
        Color32::WHITE,
    );
    response.clicked()
}

/// Miniatura estilo React Flow (minimap) en la esquina inferior derecha.
fn draw_minimap(ui: &Ui, rect: egui::Rect, doc: &Snarl<crate::editor::visual_script::VisualNode>) {
    if doc.nodes().count() == 0 {
        return;
    }
    let size = Vec2::new(130.0, 88.0);
    let map = egui::Rect::from_min_max(
        rect.max - size - Vec2::new(10.0, 10.0),
        rect.max - Vec2::new(10.0, 10.0),
    );
    let painter = ui.painter().with_clip_rect(map);
    painter.rect_filled(
        map,
        CornerRadius::same(6),
        Color32::from_rgb(0x1a, 0x1f, 0x2e),
    );
    painter.rect_stroke(
        map,
        CornerRadius::same(6),
        Stroke::new(1.0, Color32::from_rgb(0x3d, 0x44, 0x66)),
        StrokeKind::Inside,
    );

    let positions: Vec<(egui::Pos2, NodeId, Color32)> = doc
        .nodes_pos_ids()
        .map(|(id, pos, node)| {
            let color = match node.kind.category() {
                crate::editor::visual_script::NodeCategory::Event => theme::INFO,
                crate::editor::visual_script::NodeCategory::Flow => theme::CONTROL_BLUE,
                crate::editor::visual_script::NodeCategory::Action => theme::SUCCESS,
            };
            (pos, id, color)
        })
        .collect();
    let min_x = positions.iter().map(|p| p.0.x).fold(f32::MAX, f32::min);
    let min_y = positions.iter().map(|p| p.0.y).fold(f32::MAX, f32::min);
    let max_x = positions.iter().map(|p| p.0.x).fold(f32::MIN, f32::max);
    let max_y = positions.iter().map(|p| p.0.y).fold(f32::MIN, f32::max);
    let span_x = (max_x - min_x).max(1.0);
    let span_y = (max_y - min_y).max(1.0);
    let scale = (map.width() / span_x).min(map.height() / span_y) * 0.8;
    for (pos, _id, color) in positions {
        let center = egui::pos2(
            map.min.x + (map.width() - span_x * scale) / 2.0 + (pos.x - min_x) * scale,
            map.min.y + (map.height() - span_y * scale) / 2.0 + (pos.y - min_y) * scale,
        );
        painter.rect_filled(
            egui::Rect::from_center_size(center, Vec2::new(16.0 * scale, 8.0 * scale)),
            CornerRadius::same(1),
            color,
        );
    }
}
