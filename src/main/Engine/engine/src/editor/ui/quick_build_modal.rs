//! Contenido de la ventana nativa de construcción rápida (blueprints).
//!
//! Sustituye al `BluePrintModalBody` del renderer. La lista sale del `ProjectState`
//! que el editor mantiene como fuente de verdad. Al elegir, el propio editor
//! construye y envía el comando de ghost/colocación al motor; al borrar, elimina
//! entidades y blueprints de su estado sin pasar por React.

use egui::RichText;
use egui_phosphor::regular as icons;
use serde_json::{Value, json};

use crate::editor::ui::sidebar_left::QuickBuildDeleteDraft;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::widgets;

#[derive(Clone)]
struct BlueprintItem {
    id: String,
    name: String,
    category: &'static str,
    is_3d: bool,
    raw: Value,
}

fn text_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .get(key)
                .and_then(Value::as_u64)
                .map(|id| id.to_string())
        })
        .unwrap_or_default()
}

fn value_id(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn category_of(value: &Value) -> &'static str {
    let category = value
        .get("entity_category")
        .or_else(|| value.get("category"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let kind = value.get("kind").and_then(Value::as_str).unwrap_or("");
    if matches!(category, "character" | "player") || matches!(kind, "character" | "player") {
        "character"
    } else if category == "environment" || matches!(kind, "scenario" | "environment") {
        "environment"
    } else {
        "object"
    }
}

/// Tarjeta de 80×80 con icono/preview y nombre, como el `BluePrintCard` de React.
fn blueprint_card(
    ui: &mut egui::Ui,
    blueprint: &BlueprintItem,
    on_delete: &mut dyn FnMut(),
) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(80.0, 80.0), egui::Sense::click());
    let fill = if response.hovered() {
        egui::Color32::from_white_alpha(12)
    } else {
        egui::Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        egui::CornerRadius::same(8),
        fill,
        egui::Stroke::new(1.0, theme::OUTLINE_SECONDARY),
        egui::StrokeKind::Inside,
    );
    // El preview de sprite no está disponible en la ventana nativa: se usa el
    // icono de rejilla (2D) o de caja (3D), igual que el fallback de React.
    let icon = if blueprint.is_3d {
        icons::CUBE
    } else {
        icons::GRID_NINE
    };
    painter.text(
        egui::pos2(rect.center().x, rect.min.y + 28.0),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(24.0),
        theme::TEXT,
    );
    let name = if blueprint.name.chars().count() > 12 {
        format!("{}…", blueprint.name.chars().take(11).collect::<String>())
    } else {
        blueprint.name.clone()
    };
    painter.text(
        egui::pos2(rect.center().x, rect.max.y - 9.0),
        egui::Align2::CENTER_CENTER,
        name,
        egui::FontId::proportional(10.0),
        theme::TEXT,
    );
    // Botón rojo de borrado en la esquina, como en la tarjeta de React.
    let delete_rect = egui::Rect::from_center_size(
        rect.right_top() - egui::vec2(11.0, -11.0),
        egui::vec2(20.0, 20.0),
    );
    painter.circle_filled(delete_rect.center(), 10.0, theme::DANGER);
    painter.text(
        delete_rect.center(),
        egui::Align2::CENTER_CENTER,
        icons::TRASH,
        egui::FontId::proportional(10.0),
        egui::Color32::WHITE,
    );
    let clicked = response.clicked();
    let pointer = response.interact_pointer_pos();
    if clicked && pointer.is_some_and(|pos| delete_rect.contains(pos)) {
        on_delete();
        false
    } else {
        clicked
    }
}

fn array3(value: Option<&Value>, default: [f32; 3]) -> [f32; 3] {
    value
        .and_then(Value::as_array)
        .and_then(|values| {
            let floats: Vec<f32> = values
                .iter()
                .filter_map(Value::as_f64)
                .map(|v| v as f32)
                .collect();
            <[f32; 3]>::try_from(floats).ok()
        })
        .unwrap_or(default)
}

fn array4(value: Option<&Value>, default: [f32; 4]) -> [f32; 4] {
    value
        .and_then(Value::as_array)
        .and_then(|values| {
            let floats: Vec<f32> = values
                .iter()
                .filter_map(Value::as_f64)
                .map(|v| v as f32)
                .collect();
            <[f32; 4]>::try_from(floats).ok()
        })
        .unwrap_or(default)
}

/// Comando de colocación que antes construía React (QuickBuildBridge). El motor
/// completa el resto del metadata desde su registro por `preview_blueprint_id`.
fn placement_command(app: &EditorApp, blueprint: &Value) -> Value {
    let id = value_id(blueprint.get("id")).unwrap_or_default();
    let name = text_field(blueprint, "name");
    let model = text_field(blueprint, "model");
    let path = if model.is_empty() {
        text_field(blueprint, "path")
    } else {
        model.clone()
    };
    let first_frame = blueprint
        .get("animations")
        .and_then(Value::as_array)
        .and_then(|animations| animations.first())
        .and_then(|animation| animation.get("frames"))
        .and_then(Value::as_array)
        .and_then(|frames| frames.first())
        .and_then(|frame| frame.get("path"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let is_3d = app.cli.context.is_3d();
    let preview_path = if is_3d {
        if path.is_empty() {
            first_frame.to_string()
        } else {
            app.assets
                .models
                .iter()
                .find(|asset| {
                    asset.path == path || asset.model_id.as_deref() == Some(path.as_str())
                })
                .map(|asset| asset.path.clone())
                .unwrap_or(path)
        }
    } else if first_frame.is_empty() {
        path.clone()
    } else {
        first_frame.to_string()
    };
    let kind = text_field(blueprint, "kind");
    let category = text_field(blueprint, "entity_category");
    let category = if category.is_empty() {
        text_field(blueprint, "category")
    } else {
        category
    };
    let category = if category.is_empty() {
        "character".to_string()
    } else {
        category
    };
    let scale = array3(blueprint.get("scale"), [1.0, 1.0, 1.0]);
    let rotation = array4(blueprint.get("rotation"), [0.0, 0.0, 0.0, 1.0]);
    let blueprint_id = value_id(blueprint.get("id"));
    let mut command = json!({
        "cmd": "set_active_tool",
        "tool": "quick_build_place",
        "preview_path": preview_path,
        "preview_kind": if is_3d { "model" } else if kind == "scenario" { "scenario" } else { "character" },
        "preview_scale": scale,
        "preview_name": name,
        "preview_blueprint_id": id,
        "preview_blueprint": {
            "category": category,
            "model": model,
            "colision": blueprint.get("colision").or_else(|| blueprint.get("physics_enabled")).cloned().unwrap_or(json!(category == "environment")),
            "physics_type": blueprint.get("physics_type").cloned().unwrap_or(Value::Null),
            "physics_enabled": blueprint.get("physics_enabled").cloned().unwrap_or(Value::Null),
            "rotation": rotation,
            "scale": scale,
            "blueprint_id": blueprint_id,
            "template_name": text_field(blueprint, "name"),
        }
    });
    if is_3d {
        command["preview_entity_category"] = json!(category);
        command["preview_rotation"] = json!(rotation);
    }
    command
}

/// Dibuja el selector en la ventana modal externa.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    let mut tab = app.left_sidebar.quick_build_tab;
    let blueprints: Vec<BlueprintItem> = app
        .project_state
        .blueprints
        .iter()
        .map(|value| BlueprintItem {
            id: text_field(value, "id"),
            name: {
                let name = text_field(value, "name");
                if name.is_empty() {
                    text_field(value, "id")
                } else {
                    name
                }
            },
            category: category_of(value),
            is_3d: ["model", "model_id", "visualModelPath"]
                .iter()
                .filter_map(|key| value.get(*key).and_then(serde_json::Value::as_str))
                .any(|path| {
                    let path = path.to_ascii_lowercase();
                    path.ends_with(".glb") || path.ends_with(".gltf") || path.ends_with(".fbx")
                }),
            raw: value.clone(),
        })
        .collect();
    let mut selected: Option<BlueprintItem> = None;
    let mut delete_clicked: Option<BlueprintItem> = None;
    tab = tab.min(2);

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL_BG)
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ui, |ui| {
            let tabs = [
                tr(locale, "Personaje", "Character"),
                tr(locale, "Entorno", "Environment"),
                tr(locale, "Objetos", "Objects"),
            ];
            ui.horizontal(|ui| {
                for (index, label) in tabs.iter().enumerate() {
                    if widgets::modal_tab(ui, label, tab == index) {
                        tab = index;
                    }
                }
            });
            ui.add_space(10.0);

            let keys = ["character", "environment", "object"];
            let filtered: Vec<&BlueprintItem> = blueprints
                .iter()
                .filter(|blueprint| blueprint.category == keys[tab])
                .collect();
            if filtered.is_empty() {
                ui.label(
                    RichText::new(tr(
                        locale,
                        "No hay blueprints en esta categoría",
                        "No blueprints in this category",
                    ))
                    .size(13.0)
                    .italics()
                    .color(theme::TEXT_MUTED),
                );
                return;
            }

            ui.label(
                RichText::new(tr(
                    locale,
                    "Selecciona un blueprint para activar el modo construcción rápida.",
                    "Select a blueprint to activate quick build mode.",
                ))
                .size(13.0)
                .color(theme::TEXT_MUTED),
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .id_salt("quick_build_cards")
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for blueprint in filtered {
                            if blueprint_card(ui, blueprint, &mut || {
                                delete_clicked = Some(blueprint.clone());
                            }) {
                                selected = Some(blueprint.clone());
                            }
                        }
                    });
                });
        });

    // Al pulsar la papelera se prepara el dialogo anidado de confirmacion.
    if let Some(blueprint) = delete_clicked {
        let count = app
            .project_state
            .entity_meta
            .values()
            .filter(|meta| value_id(meta.get("blueprintId")) == Some(blueprint.id.clone()))
            .count();
        app.left_sidebar.quick_build_delete = Some(QuickBuildDeleteDraft {
            blueprint: blueprint.raw,
            linked_count: count,
        });
    }

    // Dialogo anidado: mismo flujo que el InlineNestedDialog del renderer.
    let draft = app.left_sidebar.quick_build_delete.take();
    if let Some(draft) = draft {
        let name = text_field(&draft.blueprint, "name");
        let count = draft.linked_count;
        let mut mode: Option<&'static str> = None;
        let mut dismiss = false;
        let modal = egui::Modal::new(egui::Id::new("quick_build_delete_dialog"))
            .frame(
                egui::Frame::popup(&ui.ctx().style_of(egui::Theme::Dark))
                    .fill(theme::PANEL_BG)
                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ui.ctx(), |ui| {
                ui.label(
                    RichText::new(tr(locale, "Eliminar blueprint", "Delete blueprint"))
                        .size(15.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(tr(
                            locale,
                            "Esta acción eliminará el blueprint",
                            "This action will delete the blueprint",
                        ))
                        .size(13.0)
                        .color(theme::TEXT),
                    );
                    ui.label(RichText::new(&name).size(13.0).strong().color(theme::TEXT));
                });
                ui.add_space(8.0);
                if count > 0 {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(tr(locale, "Hay", "There are"))
                                .size(13.0)
                                .color(theme::TEXT),
                        );
                        ui.label(RichText::new(count.to_string()).size(13.0).strong().color(theme::TEXT));
                        ui.label(
                            RichText::new(tr(
                                locale,
                                "entidades basadas en este blueprint. ¿Qué quieres hacer con ellas?",
                                "entities based on this blueprint. What do you want to do with them?",
                            ))
                            .size(13.0)
                            .color(theme::TEXT),
                        );
                    });
                    ui.add_space(12.0);
                    let delete_all_label = format!(
                        "{} ({})",
                        tr(locale, "Eliminar todas las entidades", "Delete all entities"),
                        count
                    );
                    if widgets::modal_button(
                        ui,
                        icons::TRASH,
                        &delete_all_label,
                        widgets::ModalButtonStyle::Danger,
                        220.0,
                        true,
                    ) {
                        mode = Some("with_entities");
                    }
                    ui.add_space(6.0);
                    if widgets::modal_button(
                        ui,
                        icons::ARROW_RIGHT,
                        tr(
                            locale,
                            "Convertir en entidades independientes",
                            "Convert to standalone entities",
                        ),
                        widgets::ModalButtonStyle::Primary,
                        220.0,
                        true,
                    ) {
                        mode = Some("keep_entities");
                    }
                } else {
                    ui.label(
                        RichText::new(tr(locale, "No se puede deshacer.", "Cannot be undone."))
                            .size(12.0)
                            .color(theme::TEXT_MUTED),
                    );
                    ui.add_space(12.0);
                    if widgets::modal_button(
                        ui,
                        icons::TRASH,
                        tr(locale, "Eliminar", "Delete"),
                        widgets::ModalButtonStyle::Danger,
                        110.0,
                        true,
                    ) {
                        mode = Some("with_entities");
                    }
                }
                ui.add_space(10.0);
                if widgets::modal_button(
                    ui,
                    icons::X,
                    tr(locale, "Cancelar", "Cancel"),
                    widgets::ModalButtonStyle::Secondary,
                    100.0,
                    true,
                ) {
                    dismiss = true;
                }
            });
        if modal.should_close() {
            dismiss = true;
        }
        if let Some(mode) = mode {
            // Borrado 100% nativo: entidades y catálogo los gestiona el editor.
            let bp = &draft.blueprint;
            let bp_id = value_id(bp.get("id")).unwrap_or_default();
            if mode == "with_entities" {
                let ids: Vec<u64> = app
                    .project_state
                    .entity_meta
                    .iter()
                    .filter(|(_, meta)| value_id(meta.get("blueprintId")) == Some(bp_id.clone()))
                    .filter_map(|(id, _)| id.parse::<u64>().ok())
                    .collect();
                for id in ids {
                    app.send_engine(json!({ "cmd": "remove_entity", "id": id }));
                }
            } else {
                for meta in app.project_state.entity_meta.values_mut() {
                    if value_id(meta.get("blueprintId")) != Some(bp_id.clone()) {
                        continue;
                    }
                    for (source, target) in [
                        ("physics_enabled", "physicsEnabled"),
                        ("physics_type", "physicsType"),
                        ("animations", "animations"),
                        ("scripts", "scripts"),
                        ("control_bindings", "controlBindings"),
                    ] {
                        if let Some(value) = bp.get(source) {
                            meta[target] = value.clone();
                        }
                    }
                    if let Some(object) = meta.as_object_mut() {
                        object.remove("blueprintId");
                    }
                }
            }
            app.project_state
                .blueprints
                .retain(|entry| value_id(entry.get("id")) != Some(bp_id.clone()));
            if app.quick_build_active.as_deref() == Some(bp_id.as_str()) {
                app.quick_build_active = None;
            }
        } else if !dismiss {
            app.left_sidebar.quick_build_delete = Some(draft);
        }
    }

    if let Some(blueprint) = selected {
        let name = blueprint.name.clone();
        app.quick_build_active = Some(name.clone());
        app.send_engine(placement_command(app, &blueprint.raw));
        app.placement_hint = Some(
            tr(
                locale,
                "Mueve el cursor sobre el mundo y haz clic para colocar la entidad",
                "Move the cursor over the world and click to place the entity",
            )
            .to_string(),
        );
        app.left_sidebar.quick_build_open = false;
    }
    app.left_sidebar.quick_build_tab = tab;
}
