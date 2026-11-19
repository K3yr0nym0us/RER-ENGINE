//! Cuerpo de la ventana nativa de propiedades de entidad.
//!
//! Traducción directa de `EntityPropertiesModalContent.tsx`: misma toolbar,
//! misma sección de física, mismas pestañas (transform / animaciones / scripting /
//! huesos / proyectil) y mismo diálogo de confirmación. Los comandos enviados al
//! motor son idénticos a los que enviaba React a través de `entityPropertiesAction`.

use egui::{Align2, Color32, FontId, RichText, Sense, Stroke, StrokeKind, Vec2};
use egui_code_editor::CodeEditor;
use egui_phosphor::regular as icons;
use serde_json::{Value, json};

use crate::editor::app::EditorApp;
use crate::editor::cli::Locale;
use crate::editor::entity_properties::{
    ConfirmAction, EntityPropertiesState, NativeNested, ProjectileCfg, PropTab,
};
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::scene_script_modal::{RER_THEME, rhai_syntax};
use super::widgets;

fn t(locale: Locale, es: &'static str, en: &'static str) -> String {
    tr(locale, es, en).to_string()
}

fn muted(text: &str, size: f32) -> RichText {
    RichText::new(text).size(size).color(theme::TEXT_MUTED)
}

fn body(text: &str, size: f32) -> RichText {
    RichText::new(text).size(size).color(theme::TEXT)
}

fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(muted(text, 12.0));
}

/// Botón con borde (equivalente a `.btn-outline-*` de Bootstrap), icono centrado
/// y ancho calculado a partir del texto.
fn outline_button(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    color: Color32,
    enabled: bool,
    width: Option<f32>,
) -> bool {
    let font = FontId::proportional(12.5);
    let text = if icon.is_empty() {
        label.to_string()
    } else {
        format!("{icon}  {label}")
    };
    let measured = ui
        .painter()
        .layout_no_wrap(text.clone(), font.clone(), color)
        .size()
        .x;
    let width = width.unwrap_or(measured + 20.0).max(30.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 30.0), sense);
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };
    let alpha = if enabled { 1.0 } else { 0.45 };
    let fill = if enabled && response.hovered() {
        color.gamma_multiply(0.14)
    } else {
        Color32::TRANSPARENT
    };
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect(
        rect,
        egui::CornerRadius::same(5),
        fill,
        Stroke::new(1.0, color.gamma_multiply(alpha)),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        font,
        color.gamma_multiply(alpha),
    );
    enabled && response.clicked()
}

/// Botón cuadrado de icono (Edit, copiar ID, borrar, play, ...).
fn icon_button(
    ui: &mut egui::Ui,
    icon: &str,
    color: Color32,
    tooltip: &str,
    enabled: bool,
) -> bool {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(30.0, 30.0), sense);
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };
    let alpha = if enabled { 1.0 } else { 0.4 };
    let painter = ui.painter();
    painter.rect(
        rect,
        egui::CornerRadius::same(5),
        Color32::TRANSPARENT,
        Stroke::new(1.0, color.gamma_multiply(alpha)),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(14.0),
        color.gamma_multiply(alpha),
    );
    let response = response.on_hover_text(tooltip);
    enabled && response.clicked()
}

/// Pestaña con el mismo aspecto que las `.nav-tabs` del renderer.
fn prop_tab(ui: &mut egui::Ui, label: &str, active: bool) -> bool {
    let font = FontId::proportional(13.0);
    let width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), theme::TEXT)
        .size()
        .x
        + 16.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click());
    ui.painter().text(
        rect.center() - Vec2::new(0.0, 3.0),
        Align2::CENTER_CENTER,
        label,
        font,
        if active {
            theme::TEXT
        } else {
            theme::TEXT_MUTED
        },
    );
    ui.painter().line_segment(
        [
            egui::pos2(rect.min.x, rect.max.y - 1.0),
            egui::pos2(rect.max.x, rect.max.y - 1.0),
        ],
        Stroke::new(
            if active { 2.0 } else { 1.0 },
            if active { theme::INFO } else { theme::BORDER },
        ),
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// Campo numérico de texto. Devuelve `true` cuando el usuario termina de
/// editar (pierde el foco o pulsa Enter), como el `onBlur` de React.
fn number_field(ui: &mut egui::Ui, draft: &mut String, width: f32) -> egui::Response {
    ui.add_sized([width, 26.0], egui::TextEdit::singleline(draft))
}

/// Etiqueta de eje coloreada (X rojo, Y verde, Z azul, W gris).
fn axis_label(ui: &mut egui::Ui, axis: usize) {
    let (text, color) = match axis {
        0 => ("X", theme::DANGER),
        1 => ("Y", theme::SUCCESS),
        2 => ("Z", theme::INFO),
        _ => ("W", theme::TEXT_MUTED),
    };
    ui.label(RichText::new(text).size(11.0).strong().color(color));
}

fn parse_f32(draft: &str) -> Option<f32> {
    draft.trim().parse::<f32>().ok()
}

/// Dibuja la ventana completa.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    {
        let mut props = std::mem::take(&mut app.entity_properties);
        props.refresh(app);
        app.entity_properties = props;
    }

    let locale = app.cli.locale;
    let ctx = ui.ctx().clone();
    let mut commands: Vec<Value> = Vec::new();

    let frame = egui::Frame::new()
        .fill(theme::PANEL_BG)
        .inner_margin(egui::Margin::same(12));
    egui::CentralPanel::default().frame(frame).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("entity_properties_scroll")
            .show(ui, |ui| {
                let state = &mut app.entity_properties;
                if state.multi_ids.len() > 1 {
                    multi_selection(ui, state, locale, &mut commands);
                } else if state.entity.is_some() {
                    toolbar(ui, state, locale, &mut commands);
                    ui.add_space(6.0);
                    model_actions(ui, state, &app.project_state.models, locale);
                    ui.add_space(6.0);
                    physics(ui, state, locale, &mut commands);
                    ui.add_space(8.0);
                    tabs(ui, state, locale, &mut commands);
                } else {
                    ui.label(
                        RichText::new(t(
                            locale,
                            "Haz clic en un objeto para verlo",
                            "Click on an object to view it",
                        ))
                        .size(12.0)
                        .italics()
                        .color(theme::TEXT_MUTED),
                    );
                }
            });
    });

    // Diálogos anidados: se pintan encima de todo el contenido.
    confirm_dialog(&ctx, app, locale, &mut commands);
    model_picker_dialog(&ctx, app, locale, &mut commands);
    nested_dialog(&ctx, app, locale, &mut commands);

    for command in commands {
        app.send_engine(command);
    }
}

fn multi_selection(
    ui: &mut egui::Ui,
    state: &mut EntityPropertiesState,
    locale: Locale,
    commands: &mut Vec<Value>,
) {
    let count = state.multi_ids.len();
    ui.label(body(
        &format!(
            "{} {}",
            count,
            t(locale, "entidades seleccionadas", "entities selected")
        ),
        12.0,
    ));
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(8.0);

    if state.is_3d && state.multi_already_merged {
        ui.label(
            RichText::new(t(
                locale,
                "Entidades ya fusionadas",
                "Entities already merged",
            ))
            .size(12.0)
            .italics()
            .color(theme::TEXT_MUTED),
        );
    } else if state.is_3d {
        let label = t(locale, "Fusionar entidades", "Merge entities");
        if outline_button(
            ui,
            icons::LINK,
            &label,
            theme::INFO,
            true,
            Some(ui.available_width()),
        ) {
            commands.push(json!({ "cmd": "merge_entities", "ids": state.multi_ids }));
        }
    }
    ui.add_space(6.0);
    let label = format!("{} ({})", t(locale, "Eliminar", "Delete"), count);
    if outline_button(
        ui,
        icons::TRASH,
        &label,
        theme::DANGER,
        true,
        Some(ui.available_width()),
    ) {
        state.confirm = Some(crate::editor::entity_properties::ConfirmDialog {
            title: t(locale, "Confirmar acción", "Confirm action"),
            message: format!(
                "{} {count} \n\n{}",
                t(
                    locale,
                    "¿Seguro que quieres eliminar",
                    "Are you sure you want to delete"
                ),
                t(
                    locale,
                    "Esta acción no se puede deshacer.",
                    "This action cannot be undone."
                )
            ),
            confirm_label: format!(
                "{} {}",
                t(locale, "Sí,", "Yes,"),
                t(locale, "eliminar", "delete")
            ),
            action: ConfirmAction::RemoveMultiple(state.multi_ids.clone()),
        });
    }
}

fn toolbar(
    ui: &mut egui::Ui,
    state: &mut EntityPropertiesState,
    locale: Locale,
    commands: &mut Vec<Value>,
) {
    let id = state.entity.as_ref().map(|e| e.id).unwrap_or(0);
    let editing = state.editing_name;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        let name_width = (ui.available_width() - 112.0).max(60.0);
        let mut name = state.name_draft.clone();
        let name_resp = ui.add_enabled(
            editing,
            egui::TextEdit::singleline(&mut name)
                .desired_width(name_width)
                .text_color(theme::INFO),
        );
        state.name_draft = name;
        if editing && name_resp.lost_focus() {
            state.editing_name = false;
        }

        if editing {
            let trimmed = state.name_draft.trim().to_string();
            let can_save = !trimmed.is_empty();
            if icon_button(
                ui,
                icons::CHECK_SQUARE,
                theme::INFO,
                &t(locale, "Guardar cambios", "Save changes"),
                can_save,
            ) {
                let old = state
                    .entity
                    .as_ref()
                    .map(|e| e.name.clone())
                    .unwrap_or_default();
                if trimmed != old {
                    commands.push(json!({ "cmd": "set_entity_name", "id": id, "name": trimmed }));
                }
                state.editing_name = false;
            }
        } else if outline_button(
            ui,
            icons::PENCIL,
            &t(locale, "Editar", "Edit"),
            theme::OUTLINE_SECONDARY,
            true,
            None,
        ) {
            state.editing_name = true;
        }

        let copied = state
            .copied_until
            .is_some_and(|until| until > std::time::Instant::now());
        let copy_tip = if copied {
            t(locale, "ID de entidad copiado", "Entity ID copied")
        } else {
            t(locale, "Copiar ID de entidad", "Copy entity ID")
        };
        if icon_button(
            ui,
            if copied { icons::CHECK } else { icons::COPY },
            if copied {
                theme::SUCCESS
            } else {
                theme::OUTLINE_SECONDARY
            },
            &copy_tip,
            true,
        ) {
            ui.ctx().copy_text(id.to_string());
            state.copied_until =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }

        if !state.is_player
            && icon_button(
                ui,
                icons::TRASH,
                theme::DANGER,
                &t(locale, "Eliminar", "Delete"),
                true,
            )
        {
            state.confirm = Some(crate::editor::entity_properties::ConfirmDialog {
                title: t(locale, "Confirmar acción", "Confirm action"),
                message: format!(
                    "{}\n\n{}",
                    t(
                        locale,
                        "¿Seguro que quieres eliminar esta entidad?",
                        "Are you sure you want to delete this entity?"
                    ),
                    t(
                        locale,
                        "Esta acción no se puede deshacer.",
                        "This action cannot be undone."
                    )
                ),
                confirm_label: format!(
                    "{} {}",
                    t(locale, "Sí,", "Yes,"),
                    t(locale, "eliminar", "delete")
                ),
                action: ConfirmAction::RemoveEntity(id),
            });
        }
    });
}

fn model_actions(
    ui: &mut egui::Ui,
    state: &mut EntityPropertiesState,
    models: &[Value],
    locale: Locale,
) {
    if state.is_collider || state.is_execution_area {
        return;
    }
    let is_3d = state.is_3d;
    let id = state.entity.as_ref().map(|e| e.id).unwrap_or(0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
        let width = ui.available_width() / 2.0 - 4.0;
        if is_3d && !state.is_from_blueprint {
            let label = t(locale, "Reemplazar modelo", "Replace model");
            if outline_button(ui, icons::CUBE, &label, theme::INFO, true, Some(width)) {
                let intent = if state.is_player || state.is_character {
                    "character"
                } else if state.is_environment {
                    "environment"
                } else {
                    "object"
                };
                let hint = if state.is_player {
                    t(
                        locale,
                        "Reemplazar el modelo del jugador. El nuevo modelo debe tener animaciones de caminar, correr y salto.",
                        "Replace the player model. The new model must have walk, run and jump animations.",
                    )
                } else {
                    t(
                        locale,
                        "Reemplazar el modelo de la entidad. Se conservarán transform y física.",
                        "Replace the entity model. Transform and physics are kept.",
                    )
                };
                let current = state.current_model_path.clone().unwrap_or_default();
                let mut choices: Vec<crate::editor::entity_properties::ModelChoice> = models
                    .iter()
                    .filter(|m| m.get("available_in_scene").and_then(Value::as_bool) != Some(false))
                    .filter(|m| {
                        let category = m.get("category").and_then(Value::as_str).unwrap_or("object");
                        match intent {
                            "character" => category == "character",
                            "environment" => category == "environment",
                            _ => category == "object" || category.is_empty(),
                        }
                    })
                    .filter(|m| {
                        let key = |v: Option<&str>| {
                            v.unwrap_or_default()
                                .replace('\\', "/")
                                .trim_end_matches('/')
                                .to_ascii_lowercase()
                        };
                        let model_key = key(m.get("path").and_then(Value::as_str));
                        let current_key = key(Some(&current));
                        // Se excluye el modelo ya asignado (React compara path,
                        // model_id y asset normalizados; aquí basta con path).
                        current_key.is_empty() || model_key != current_key
                    })
                    .map(|m| crate::editor::entity_properties::ModelChoice {
                        label: m
                            .get("name")
                            .and_then(Value::as_str)
                            .filter(|n| !n.is_empty())
                            .map(str::to_string)
                            .unwrap_or_else(|| {
                                m.get("path")
                                    .and_then(Value::as_str)
                                    .and_then(|p| p.rsplit(['/', '\\']).next())
                                    .unwrap_or("model")
                                    .to_string()
                            }),
                        path: m.get("path").and_then(Value::as_str).unwrap_or_default().to_string(),
                        state: m
                            .get("state")
                            .and_then(Value::as_str)
                            .unwrap_or("ready")
                            .to_string(),
                    })
                    .collect();
                choices.sort_by_key(|choice| choice.label.to_ascii_lowercase());
                state.model_picker = Some(crate::editor::entity_properties::ModelPicker {
                    hint,
                    choices,
                    selected: None,
                });
            }
        }
        if state.is_from_blueprint {
            let tooltip = match state.linked_blueprint_name.as_deref() {
                Some(name) => format!(
                    "{}: {}",
                    t(locale, "Basado en blueprint", "Based on blueprint"),
                    name
                ),
                None => t(locale, "Basado en blueprint", "Based on blueprint"),
            };
            let label = t(locale, "Basado en blueprint", "Based on blueprint");
            let _ = outline_button(
                ui,
                icons::SQUARE,
                &label,
                theme::OUTLINE_SECONDARY,
                false,
                Some(width),
            );
            ui.label(RichText::new("").size(1.0)).on_hover_text(tooltip);
        } else {
            let label = t(locale, "Convertir a Blueprint", "Convert to Blueprint");
            if outline_button(ui, icons::SQUARE, &label, theme::ACCENT, true, Some(width)) {
                state.confirm = Some(crate::editor::entity_properties::ConfirmDialog {
                    title: t(locale, "Convertir a Blueprint", "Convert to Blueprint"),
                    message: format!(
                        "{} {}?",
                        t(
                            locale,
                            "¿Convertir la entidad en una blueprint reutilizable?",
                            "Convert this entity into a reusable blueprint?"
                        ),
                        state.entity.as_ref().map(|e| e.name.as_str()).unwrap_or("")
                    ),
                    confirm_label: t(locale, "Confirmar", "Confirm"),
                    action: ConfirmAction::ConvertToBlueprint(id),
                });
            }
        }
    });
}

/// Selector nativo de modelos (antes `CreateEntityFromModelModalBody`).
fn model_picker_dialog(
    ctx: &egui::Context,
    app: &mut EditorApp,
    locale: Locale,
    commands: &mut Vec<Value>,
) {
    let Some(mut picker) = app.entity_properties.model_picker.take() else {
        return;
    };
    let mut close = false;
    egui::Window::new(t(locale, "Reemplazar modelo", "Replace model"))
        .id(egui::Id::new("entity_properties_model_picker"))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_size(Vec2::new(340.0, 120.0));
            ui.label(muted(&picker.hint, 12.0));
            ui.add_space(6.0);
            if picker.choices.is_empty() {
                ui.label(muted(
                    &t(
                        locale,
                        "No hay modelos precargados. Carga modelos primero en el acordeón de recursos.",
                        "No preloaded models. Load models first in the resources accordion.",
                    ),
                    12.0,
                ));
            } else {
                for choice in &picker.choices {
                    let importing = choice.state == "importing";
                    let label = format!(
                        "{}{}",
                        choice.label,
                        if importing {
                            format!(" ({})", t(locale, "cargando…", "loading…"))
                        } else {
                            String::new()
                        }
                    );
                    if ui
                        .add_enabled(
                            !importing,
                            egui::RadioButton::new(
                                picker.selected.as_deref() == Some(choice.path.as_str()),
                                label,
                            ),
                        )
                        .clicked()
                    {
                        picker.selected = Some(choice.path.clone());
                    }
                }
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if outline_button(
                        ui,
                        "",
                        &t(locale, "Cancelar", "Cancel"),
                        theme::OUTLINE_SECONDARY,
                        true,
                        None,
                    ) {
                        close = true;
                    }
                    let can_replace = picker
                        .selected
                        .as_deref()
                        .is_some_and(|path| !path.is_empty());
                    if outline_button(
                        ui,
                        "",
                        &t(locale, "Reemplazar", "Replace"),
                        theme::INFO,
                        can_replace,
                        None,
                    ) && let Some(path) = picker.selected.clone()
                    {
                        let id = app.entity_properties.entity().map(|e| e.id).unwrap_or(0);
                        commands.push(json!({
                            "cmd": "replace_entity_model", "id": id, "path": path
                        }));
                        close = true;
                    }
                });
            });
        });
    if !close {
        app.entity_properties.model_picker = Some(picker);
    }
}

fn physics(
    ui: &mut egui::Ui,
    state: &mut EntityPropertiesState,
    locale: Locale,
    commands: &mut Vec<Value>,
) {
    if state.is_collider || state.is_execution_area {
        return;
    }
    let id = state.entity.as_ref().map(|e| e.id).unwrap_or(0);
    let physics_enabled = state
        .entity
        .as_ref()
        .map(|e| e.physics_enabled)
        .unwrap_or(false);
    let physics_type = state
        .entity
        .as_ref()
        .map(|e| e.physics_type.clone())
        .unwrap_or_else(|| "dynamic".to_string());

    if state.is_environment {
        let mut checked = physics_enabled;
        ui.horizontal(|ui| {
            if ui.checkbox(&mut checked, "").clicked() {
                commands.push(json!({
                    "cmd": "set_physics", "id": id, "enabled": checked, "body_type": "static"
                }));
            }
            ui.label(body(&t(locale, "Con colisión", "With collision"), 12.0));
        });
    } else if state.is_player {
        ui.horizontal(|ui| {
            let mut checked = true;
            ui.add_enabled(false, egui::Checkbox::new(&mut checked, ""));
            ui.label(body(&t(locale, "Activar física", "Enable physics"), 12.0));
        });
        ui.add_enabled(
            false,
            egui::Label::new(body(
                &t(locale, "Dinámico (gravedad)", "Dynamic (gravity)"),
                12.0,
            )),
        );
    } else {
        let mut checked = physics_enabled;
        ui.horizontal(|ui| {
            if ui.checkbox(&mut checked, "").clicked() {
                let body_type = if checked && state.is_character {
                    "kinematic".to_string()
                } else {
                    physics_type.clone()
                };
                commands.push(json!({
                    "cmd": "set_physics", "id": id, "enabled": checked, "body_type": body_type
                }));
            }
            ui.label(body(&t(locale, "Activar física", "Enable physics"), 12.0));
        });
        if physics_enabled {
            let mut selected = physics_type.as_str();
            egui::ComboBox::from_id_salt("entity_physics_type")
                .selected_text(selected)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for (value, label) in [
                        (
                            "dynamic",
                            t(locale, "Dinámico (gravedad)", "Dynamic (gravity)"),
                        ),
                        (
                            "static",
                            t(locale, "Estático (no se mueve)", "Static (does not move)"),
                        ),
                        (
                            "kinematic",
                            t(locale, "Cinemático (por código)", "Kinematic (by code)"),
                        ),
                    ] {
                        if ui.selectable_label(selected == value, label).clicked() {
                            selected = value;
                            commands.push(json!({
                                "cmd": "set_physics", "id": id, "enabled": true, "body_type": value
                            }));
                        }
                    }
                });
        }
    }

    ui.add_space(6.0);
    section_label(ui, &t(locale, "Dureza", "Hardness"));
    let mut hardness = state.hardness_draft.clone();
    if number_field(ui, &mut hardness, ui.available_width()).lost_focus() {
        state.hardness_draft = hardness;
        let value = state.hardness_draft.trim().parse::<u32>().unwrap_or(0);
        commands.push(json!({ "cmd": "set_entity_hardness", "id": id, "hardness": value }));
    } else {
        state.hardness_draft = hardness;
    }
    ui.label(muted(
        &t(
            locale,
            "0 = los proyectiles ignoran esta entidad",
            "0 = projectiles ignore this entity",
        ),
        12.0,
    ));
}

fn tabs(
    ui: &mut egui::Ui,
    state: &mut EntityPropertiesState,
    locale: Locale,
    commands: &mut Vec<Value>,
) {
    let is_3d = state.is_3d;
    let has_embedded = state
        .entity
        .as_ref()
        .is_some_and(|e| e.animations.iter().any(|a| a.embedded_in_model));
    let is_multi = state.multi_ids.len() > 1;

    let mut tabs = vec![PropTab::Transform];
    if !state.is_collider && !state.is_execution_area && (!is_3d || has_embedded) {
        tabs.push(PropTab::Animations);
    }
    if !state.is_collider {
        tabs.push(PropTab::Scripting);
    }
    if is_3d && state.can_have_bone_physics {
        tabs.push(PropTab::Bones);
    }
    if state.is_projectile && !is_multi {
        tabs.push(PropTab::Projectile);
    }
    if is_3d && !state.is_collider && !is_multi {
        tabs.push(PropTab::Interaction);
    }
    if !tabs.contains(&state.active_tab) {
        state.active_tab = tabs.first().copied().unwrap_or(PropTab::Transform);
    }

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
        for tab in &tabs {
            let label = match tab {
                PropTab::Transform => t(locale, "Transformaciones", "Transformations"),
                PropTab::Animations => t(locale, "Animaciones", "Animations"),
                PropTab::Scripting => t(locale, "Programar entidad", "Program entity"),
                PropTab::Bones => t(locale, "Huesos", "Bones"),
                PropTab::Projectile => t(locale, "Proyectil", "Projectile"),
                PropTab::Interaction => t(locale, "Interacción", "Interaction"),
            };
            if prop_tab(ui, &label, state.active_tab == *tab) {
                state.active_tab = *tab;
            }
        }
    });
    ui.add_space(10.0);

    match state.active_tab {
        PropTab::Transform => transform_tab(ui, state, locale, commands),
        PropTab::Animations => animations_tab(ui, state, locale, commands, is_3d),
        PropTab::Scripting => scripting_tab(ui, state, locale, commands),
        PropTab::Bones => bones_tab(ui, state, locale, commands),
        PropTab::Projectile => projectile_tab(ui, state, locale, commands, is_3d),
        PropTab::Interaction => interaction_tab(ui, state, locale, commands),
    }
}

fn transform_tab(
    ui: &mut egui::Ui,
    state: &mut EntityPropertiesState,
    locale: Locale,
    commands: &mut Vec<Value>,
) {
    let id = state.entity.as_ref().map(|e| e.id).unwrap_or(0);
    let is_editor_camera = state.is_editor_camera && state.is_3d;
    let is_2d = !state.is_3d;
    let is_play_character =
        state.is_player && !state.is_collider && !state.is_execution_area && !is_2d;

    section_label(ui, &t(locale, "Posición", "Position"));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for axis in 0..3 {
            let w = (ui.available_width() - 8.0) / 3.0;
            ui.allocate_ui_with_layout(
                Vec2::new(w, 26.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    axis_label(ui, axis);
                    let mut draft = state.pos_draft[axis].clone();
                    if number_field(ui, &mut draft, w - 22.0).changed()
                        && let Some(value) = parse_f32(&draft)
                    {
                        commands.push(json!({
                            "cmd": "set_transform", "id": id,
                            "position_axis": { "axis": axis, "value": value }
                        }));
                    }
                    state.pos_draft[axis] = draft;
                },
            );
        }
    });
    ui.add_space(8.0);

    if !is_editor_camera {
        if is_2d {
            section_label(
                ui,
                &t(locale, "Rotación (cuaternión)", "Rotation (quaternion)"),
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for axis in 0..4 {
                    let w = (ui.available_width() - 12.0) / 4.0;
                    ui.allocate_ui_with_layout(Vec2::new(w, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        axis_label(ui, axis);
                        let mut draft = state.rot_draft[axis].clone();
                        if number_field(ui, &mut draft, w - 22.0).changed() {
                            let mut drafts = state.rot_draft.clone();
                            drafts[axis] = draft.clone();
                            let values: Option<Vec<f32>> = drafts.iter().map(|v| parse_f32(v)).collect();
                            if let Some(values) = values {
                                commands.push(json!({ "cmd": "set_transform", "id": id, "rotation": values }));
                            }
                        }
                        state.rot_draft[axis] = draft;
                    });
                }
            });
        } else {
            section_label(ui, &t(locale, "Rotación (grados)", "Rotation (degrees)"));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for axis in 0..3 {
                    let w = (ui.available_width() - 8.0) / 3.0;
                    ui.allocate_ui_with_layout(Vec2::new(w, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        axis_label(ui, axis);
                        let mut draft = state.rot_draft[axis].clone();
                        if number_field(ui, &mut draft, w - 22.0).changed()
                            && let Some(value) = parse_f32(&draft)
                        {
                            let rounded = value.round();
                            let base = state.rot_focus_deg[axis];
                            let delta = shortest_delta(base, rounded);
                            if delta.abs() > 1e-6 {
                                if is_play_character {
                                    commands.push(json!({
                                        "cmd": "set_transform", "id": id,
                                        "body_rotation_only": true,
                                        "rotation_euler_delta": { "axis": axis, "degrees": delta }
                                    }));
                                } else {
                                    let mut degrees = state.rot_focus_deg;
                                    degrees[axis] = rounded;
                                    commands.push(json!({
                                        "cmd": "set_transform", "id": id, "rotation_euler_degrees": degrees
                                    }));
                                }
                                state.rot_focus_deg[axis] = rounded;
                            }
                        }
                        state.rot_draft[axis] = draft;
                    });
                }
            });
        }
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            section_label(ui, &t(locale, "Escala", "Scale"));
            let lock_tip = if state.lock_proportions {
                t(locale, "Bloquear proporciones", "Lock proportions")
            } else {
                t(locale, "Mantener proporciones", "Keep proportions")
            };
            if icon_button(
                ui,
                if state.lock_proportions {
                    icons::LOCK
                } else {
                    icons::LOCK_OPEN
                },
                theme::INFO,
                &lock_tip,
                true,
            ) {
                state.lock_proportions = !state.lock_proportions;
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            for axis in 0..3 {
                let w = (ui.available_width() - 8.0) / 3.0;
                ui.allocate_ui_with_layout(Vec2::new(w, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    axis_label(ui, axis);
                    let mut draft = state.scl_draft[axis].clone();
                    if number_field(ui, &mut draft, w - 22.0).changed()
                        && let Some(value) = parse_f32(&draft)
                    {
                        if state.lock_proportions {
                            let prev = state
                                .scl_draft
                                .iter()
                                .filter_map(|d| parse_f32(d))
                                .collect::<Vec<_>>();
                            if prev.len() == 3 {
                                let old = prev[axis];
                                if old.abs() > 1e-6 {
                                    let ratio = value / old;
                                    for (j, previous) in prev.iter().enumerate().take(3) {
                                        state.scl_draft[j] = format!("{:.3}", previous * ratio);
                                    }
                                }
                            }
                            let values: Option<Vec<f32>> =
                                state.scl_draft.iter().map(|d| parse_f32(d)).collect();
                            if let Some(values) = values {
                                commands
                                    .push(json!({ "cmd": "set_transform", "id": id, "scale": values }));
                            }
                        } else {
                            commands.push(json!({
                                "cmd": "set_transform", "id": id,
                                "scale_axis": { "axis": axis, "value": value }
                            }));
                        }
                    }
                    state.scl_draft[axis] = draft;
                });
            }
        });
    }
}

fn shortest_delta(prev: f32, next: f32) -> f32 {
    let mut delta = next - prev;
    while delta > 180.0 {
        delta -= 360.0;
    }
    while delta < -180.0 {
        delta += 360.0;
    }
    delta
}

fn animations_tab(
    ui: &mut egui::Ui,
    state: &mut EntityPropertiesState,
    locale: Locale,
    commands: &mut Vec<Value>,
    is_3d: bool,
) {
    let id = state.entity.as_ref().map(|e| e.id).unwrap_or(0);
    let animations = state
        .entity
        .as_ref()
        .map(|e| e.animations.clone())
        .unwrap_or_default();
    if !is_3d {
        let label = format!(
            "{} {}",
            icons::PLUS,
            t(locale, "Nueva animación", "New animation")
        );
        if outline_button(
            ui,
            "",
            &label,
            theme::SUCCESS,
            true,
            Some(ui.available_width()),
        ) {
            open_nested("createAnimation", state, commands);
        }
        ui.add_space(6.0);
        if animations.is_empty() {
            ui.label(
                RichText::new(t(
                    locale,
                    "Sin animaciones. Añade una para empezar.",
                    "No animations. Add a new one to start.",
                ))
                .size(12.0)
                .color(theme::TEXT_MUTED),
            );
            return;
        }
    }
    for (index, anim) in animations.iter().enumerate() {
        let can_play = if is_3d {
            anim.embedded_in_model
        } else {
            anim.frame_count > 0
        };
        let playing = state.playing_animation_name.as_deref() == Some(anim.name.as_str());
        let frame = egui::Frame::new()
            .fill(theme::ACCORDION_BG)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .corner_radius(egui::CornerRadius::same(5))
            .inner_margin(egui::Margin::same(6));
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(body(&anim.name, 12.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !is_3d
                        && icon_button(ui, icons::TRASH, theme::DANGER, &t(locale, "Eliminar animación", "Delete animation"), true)
                    {
                        state.confirm = Some(crate::editor::entity_properties::ConfirmDialog {
                            title: t(locale, "Confirmar acción", "Confirm action"),
                            message: format!(
                                "{} {}?",
                                t(locale, "¿Seguro que quieres eliminar la animación", "Are you sure you want to delete the animation"),
                                anim.name
                            ),
                            confirm_label: t(locale, "Sí, eliminar", "Yes, delete"),
                            action: ConfirmAction::DeleteAnimation(index),
                        });
                    }
                    if !is_3d
                        && icon_button(ui, icons::PENCIL, theme::WARNING, &t(locale, "Editar animación", "Edit animation"), can_play)
                    {
                        open_nested("editAnimation", state, commands);
                    }
                    if icon_button(
                        ui,
                        if playing { icons::STOP } else { icons::PLAY },
                        if playing { theme::DANGER } else { theme::SUCCESS },
                        if playing { "Stop" } else { "Play" },
                        can_play,
                    ) {
                        if playing {
                            commands.push(json!({ "cmd": "stop_animation", "id": id }));
                            state.playing_animation_name = None;
                        } else {
                            if state.playing_animation_name.is_some() {
                                commands.push(json!({ "cmd": "stop_animation", "id": id }));
                            }
                            commands.push(json!({
                                "cmd": "play_animation", "id": id, "name": anim.name, "loop": anim.loop_
                            }));
                            state.playing_animation_name = Some(anim.name.clone());
                        }
                    }
                });
            });
        });
        ui.add_space(4.0);
    }
}

fn scripting_tab(
    ui: &mut egui::Ui,
    state: &mut EntityPropertiesState,
    locale: Locale,
    commands: &mut Vec<Value>,
) {
    let id = state.entity.as_ref().map(|e| e.id).unwrap_or(0);
    ui.add_space(4.0);
    let label = format!(
        "{} {}",
        icons::NETWORK,
        t(locale, "Lógica de entidad (nodos)", "Entity logic (nodes)")
    );
    if outline_button(
        ui,
        "",
        &label,
        theme::INFO,
        true,
        Some(ui.available_width()),
    ) {
        open_nested("visualScripting", state, commands);
    }
    ui.add_space(6.0);
    let label = format!(
        "{} {}",
        icons::FILE_CODE,
        t(locale, "Nuevo script (Rhai)", "New script (Rhai)")
    );
    if outline_button(
        ui,
        "",
        &label,
        theme::WARNING,
        true,
        Some(ui.available_width()),
    ) {
        open_nested("scriptEditor", state, commands);
    }
    ui.add_space(8.0);

    if state.scripts.is_empty() {
        ui.label(muted(
            &t(locale, "No hay scripts asociados.", "No scripts attached."),
            12.0,
        ));
        return;
    }
    for script in state.scripts.clone() {
        let frame = egui::Frame::new()
            .fill(theme::ACCORDION_BG)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .corner_radius(egui::CornerRadius::same(5))
            .inner_margin(egui::Margin::same(6));
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icons::FILE_CODE)
                        .size(14.0)
                        .color(theme::WARNING),
                );
                let name = ui
                    .label(body(&script.name, 12.0))
                    .on_hover_text(&script.source);
                let _ = name;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(
                        ui,
                        icons::TRASH,
                        theme::DANGER,
                        &t(locale, "Eliminar script", "Remove script"),
                        true,
                    ) {
                        state.confirm = Some(crate::editor::entity_properties::ConfirmDialog {
                            title: t(locale, "Confirmar acción", "Confirm action"),
                            message: format!(
                                "{} {}?",
                                t(
                                    locale,
                                    "¿Seguro que quieres eliminar el script",
                                    "Delete script confirm"
                                ),
                                script.name
                            ),
                            confirm_label: t(locale, "Sí, eliminar", "Yes, delete"),
                            action: ConfirmAction::DeleteScript(script.name.clone()),
                        });
                    }
                    if icon_button(
                        ui,
                        icons::PENCIL,
                        theme::INFO,
                        &t(locale, "Editar script", "Edit script"),
                        true,
                    ) {
                        state.nested = Some(NativeNested::ScriptEditor {
                            name: script.name.clone(),
                            source: script.source.clone(),
                        });
                    }
                });
            });
        });
        ui.add_space(4.0);
    }
    let _ = id;
}

fn bones_tab(
    ui: &mut egui::Ui,
    state: &mut EntityPropertiesState,
    locale: Locale,
    commands: &mut Vec<Value>,
) {
    let id = state.entity.as_ref().map(|e| e.id).unwrap_or(0);
    ui.add_space(4.0);
    if outline_button(
        ui,
        icons::CROSSHAIR,
        &t(locale, "Seleccionar hueso", "Select bone"),
        theme::INFO,
        true,
        Some(ui.available_width()),
    ) {
        commands
            .push(json!({ "cmd": "set_bone_physics_pick_mode", "entity_id": id, "active": true }));
    }
    ui.add_space(6.0);
    ui.label(muted(
        &t(
            locale,
            "Haz clic en un hueso del viewport para asignarle física.",
            "Click a bone in the viewport to assign bone physics.",
        ),
        12.0,
    ));
}

fn interaction_tab(
    ui: &mut egui::Ui,
    state: &mut EntityPropertiesState,
    locale: Locale,
    commands: &mut Vec<Value>,
) {
    let id = state.entity.as_ref().map(|e| e.id).unwrap_or(0);
    let mut enabled = state.interaction_enabled_draft;
    ui.horizontal(|ui| {
        if ui.checkbox(&mut enabled, "").clicked() {
            state.interaction_enabled_draft = enabled;
            commit_interaction(state, id, commands);
        }
        ui.label(body(
            &t(locale, "Entidad interactuable", "Interactable entity"),
            12.0,
        ));
    });
    ui.add_space(6.0);
    ui.label(muted(
        &t(
            locale,
            "Muestra un prompt anclado a la entidad cuando se mira en play.",
            "Shows an entity-anchored prompt when looked at in play.",
        ),
        11.0,
    ));
    ui.add_space(8.0);

    let field = |ui: &mut egui::Ui, label: String, draft: &mut String, width: f32| {
        section_label(ui, &label);
        let response = number_field(ui, draft, width);
        ui.add_space(4.0);
        response
    };

    if field(
        ui,
        t(locale, "Texto del prompt (ES)", "Prompt text (ES)"),
        &mut state.interaction_prompt_es_draft,
        ui.available_width(),
    )
    .lost_focus()
    {
        commit_interaction(state, id, commands);
    }
    if field(
        ui,
        t(locale, "Texto del prompt (EN)", "Prompt text (EN)"),
        &mut state.interaction_prompt_en_draft,
        ui.available_width(),
    )
    .lost_focus()
    {
        commit_interaction(state, id, commands);
    }
    if field(
        ui,
        t(locale, "Tecla", "Key"),
        &mut state.interaction_key_draft,
        ui.available_width(),
    )
    .lost_focus()
    {
        commit_interaction(state, id, commands);
    }
    if field(
        ui,
        t(locale, "Distancia máxima (m)", "Max distance (m)"),
        &mut state.interaction_max_distance_draft,
        ui.available_width(),
    )
    .lost_focus()
    {
        commit_interaction(state, id, commands);
    }

    let mut once = state.interaction_once_draft;
    ui.horizontal(|ui| {
        if ui.checkbox(&mut once, "").clicked() {
            state.interaction_once_draft = once;
            commit_interaction(state, id, commands);
        }
        ui.label(body(&t(locale, "Solo una vez", "Only once"), 12.0));
    });
}

fn commit_interaction(state: &EntityPropertiesState, id: u64, commands: &mut Vec<Value>) {
    let key = {
        let k = state.interaction_key_draft.trim();
        if k.is_empty() {
            "E".to_string()
        } else {
            k.to_string()
        }
    };
    let max_distance = state
        .interaction_max_distance_draft
        .trim()
        .parse::<f32>()
        .unwrap_or(3.5)
        .clamp(0.5, 100.0);
    commands.push(json!({
        "cmd": "set_interactable",
        "id": id,
        "enabled": state.interaction_enabled_draft,
        "prompt_es": state.interaction_prompt_es_draft,
        "prompt_en": state.interaction_prompt_en_draft,
        "key": key,
        "once": state.interaction_once_draft,
        "max_distance": max_distance,
    }));
}

fn projectile_tab(
    ui: &mut egui::Ui,
    state: &mut EntityPropertiesState,
    locale: Locale,
    commands: &mut Vec<Value>,
    is_3d: bool,
) {
    let id = state.entity.as_ref().map(|e| e.id).unwrap_or(0);
    let mut cfg = state.projectile_config.clone().unwrap_or_default();

    let field = |ui: &mut egui::Ui, label: String, draft: &mut String, width: f32| {
        section_label(ui, &label);
        let response = number_field(ui, draft, width);
        ui.add_space(4.0);
        response
    };

    if field(
        ui,
        t(locale, "Velocidad (m/s)", "Speed (m/s)"),
        &mut state.speed_draft,
        ui.available_width(),
    )
    .lost_focus()
    {
        commit_projectile(state, &mut cfg, id, commands, None);
    }
    if field(
        ui,
        t(locale, "Duración (s)", "Lifetime (s)"),
        &mut state.lifetime_draft,
        ui.available_width(),
    )
    .lost_focus()
    {
        commit_projectile(state, &mut cfg, id, commands, None);
    }

    let mut affected = state.affected_by_gravity_draft;
    ui.horizontal(|ui| {
        if ui.checkbox(&mut affected, "").clicked() {
            state.affected_by_gravity_draft = affected;
            if affected && parse_f32(&state.gravity_draft).unwrap_or(0.0) <= 0.0 {
                state.gravity_draft = "1".to_string();
            }
            cfg = commit_projectile(state, &mut cfg, id, commands, Some(affected));
        }
        ui.label(body(
            &t(locale, "Afectado por gravedad", "Affected by gravity"),
            12.0,
        ));
    });
    state.affected_by_gravity_draft = affected;

    if affected
        && field(
            ui,
            t(locale, "Escala de gravedad", "Gravity scale"),
            &mut state.gravity_draft,
            ui.available_width(),
        )
        .lost_focus()
    {
        commit_projectile(state, &mut cfg, id, commands, None);
    }
    if is_3d {
        section_label(ui, &t(locale, "Socket de boca", "Muzzle socket"));
        if number_field(ui, &mut state.muzzle_draft, ui.available_width()).lost_focus() {
            commit_projectile(state, &mut cfg, id, commands, None);
        }
        ui.add_space(4.0);
    }

    let mut align = state.align_draft;
    ui.horizontal(|ui| {
        if ui.checkbox(&mut align, "").clicked() {
            state.align_draft = align;
            cfg = commit_projectile(state, &mut cfg, id, commands, None);
        }
        ui.label(body(
            &t(locale, "Alinear a la velocidad", "Align to velocity"),
            12.0,
        ));
    });
    state.align_draft = align;

    if field(
        ui,
        t(locale, "Dureza", "Hardness"),
        &mut state.projectile_hardness_draft,
        ui.available_width(),
    )
    .lost_focus()
    {
        commit_projectile(state, &mut cfg, id, commands, None);
    }
    if field(
        ui,
        t(locale, "Rebotes máximos", "Max bounces"),
        &mut state.max_bounces_draft,
        ui.available_width(),
    )
    .lost_focus()
    {
        commit_projectile(state, &mut cfg, id, commands, None);
    }
    if field(
        ui,
        t(
            locale,
            "Pérdida de velocidad al impactar (0–1)",
            "Hit speed loss (0–1)",
        ),
        &mut state.bounce_loss_draft,
        ui.available_width(),
    )
    .lost_focus()
    {
        commit_projectile(state, &mut cfg, id, commands, None);
    }

    ui.add_space(8.0);
    let label = if is_3d {
        t(
            locale,
            "Prueba de disparo (hacia -Z)",
            "Test fire (forward -Z)",
        )
    } else {
        t(
            locale,
            "Prueba de disparo (hacia +X)",
            "Test fire (forward +X)",
        )
    };
    if outline_button(
        ui,
        "",
        &label,
        theme::OUTLINE_SECONDARY,
        true,
        Some(ui.available_width()),
    ) {
        commit_projectile(state, &mut cfg, id, commands, None);
        let dir = if is_3d {
            [0.0, 0.0, -1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        commands.push(json!({
            "cmd": "fire_projectile", "template_id": id,
            "dir_x": dir[0], "dir_y": dir[1], "dir_z": dir[2]
        }));
    }
    ui.add_space(6.0);
    ui.label(muted(
        &t(
            locale,
            "Editor o Play. Origen: socket de boca en from_id / arma adjunta. Impactos generan despawn + projectile_hit.",
            "Editor or Play. Origin: muzzle socket on from_id / attached weapon. Hits despawn + projectile_hit.",
        ),
        12.0,
    ));
}

/// Actualiza la configuración desde los drafts y envía el comando completo.
fn commit_projectile(
    state: &EntityPropertiesState,
    cfg: &mut ProjectileCfg,
    id: u64,
    commands: &mut Vec<Value>,
    override_affected: Option<bool>,
) -> ProjectileCfg {
    cfg.speed = parse_f32(&state.speed_draft).unwrap_or(cfg.speed);
    cfg.lifetime_s = parse_f32(&state.lifetime_draft).unwrap_or(cfg.lifetime_s);
    cfg.affected_by_gravity = override_affected.unwrap_or(state.affected_by_gravity_draft);
    cfg.gravity_scale = parse_f32(&state.gravity_draft).unwrap_or(cfg.gravity_scale);
    cfg.align_to_velocity = state.align_draft;
    cfg.muzzle_socket = state.muzzle_draft.trim().to_string();
    if cfg.muzzle_socket.is_empty() {
        cfg.muzzle_socket = "muzzle".to_string();
    }
    cfg.bounceable = state.bounceable_draft;
    cfg.max_bounces = state
        .max_bounces_draft
        .trim()
        .parse::<u32>()
        .unwrap_or(cfg.max_bounces);
    cfg.bounce_speed_loss = parse_f32(&state.bounce_loss_draft).unwrap_or(cfg.bounce_speed_loss);
    cfg.hardness = state
        .projectile_hardness_draft
        .trim()
        .parse::<u32>()
        .unwrap_or(cfg.hardness);

    commands.push(json!({
        "cmd": "set_projectile_config",
        "id": id,
        "speed": cfg.speed,
        "lifetime_s": cfg.lifetime_s,
        "affected_by_gravity": cfg.affected_by_gravity,
        "gravity_scale": cfg.gravity_scale,
        "align_to_velocity": cfg.align_to_velocity,
        "muzzle_socket": cfg.muzzle_socket,
        "bounceable": cfg.bounceable,
        "max_bounces": cfg.max_bounces,
        "bounce_speed_loss": cfg.bounce_speed_loss,
        "hardness": cfg.hardness,
    }));
    cfg.clone()
}

/// Abre un submodal nativo desde la pestaña correspondiente. Sin Electron ni React.
fn open_nested(kind: &str, state: &mut EntityPropertiesState, _commands: &mut Vec<Value>) {
    state.nested = Some(match kind {
        "scriptEditor" => NativeNested::ScriptEditor {
            name: String::new(),
            source: DEFAULT_ENTITY_SCRIPT.to_string(),
        },
        "createAnimation" => NativeNested::Animation {
            index: None,
            name: String::new(),
            sprite: String::new(),
            fps: 12,
            loop_: true,
            columns: 4,
            rows: 1,
        },
        "editAnimation" => {
            let (index, name, fps, loop_, columns) = state
                .entity
                .as_ref()
                .and_then(|entity| {
                    entity
                        .animations
                        .iter()
                        .enumerate()
                        .find(|(_, animation)| !animation.embedded_in_model)
                        .map(|(index, animation)| {
                            (
                                index,
                                animation.name.clone(),
                                animation.frame_count.max(1) as u32 * 4,
                                animation.loop_,
                                4u32,
                            )
                        })
                })
                .unwrap_or((0, String::new(), 12, true, 4));
            NativeNested::Animation {
                index: Some(index),
                name,
                sprite: String::new(),
                fps,
                loop_,
                columns,
                rows: 1,
            }
        }
        "visualScripting" => NativeNested::VisualScript,
        _ => return,
    });
}

const DEFAULT_ENTITY_SCRIPT: &str = "\
// Script de entidad — se ejecuta cada frame sobre la entidad seleccionada\n\
// API: engine.log(msg), entity.x / entity.y\n\
\n\
fn on_keep(entity) {\n\
    // Tu lógica aquí\n\
}\n";

/// Submodales nativos: editor Rhai, editor de animación y editor de nodos.
fn nested_dialog(
    ctx: &egui::Context,
    app: &mut EditorApp,
    locale: Locale,
    commands: &mut Vec<Value>,
) {
    let Some(nested) = app.entity_properties.nested.clone() else {
        return;
    };

    // El editor de nodos es una ventana nativa aparte: solo hay que configurarla.
    if let NativeNested::VisualScript = nested {
        let id = app
            .entity_properties
            .entity
            .as_ref()
            .map(|entity| entity.id);
        let name = app
            .entity_properties
            .entity
            .as_ref()
            .map(|entity| entity.name.clone())
            .unwrap_or_default();
        app.visual_script.open = true;
        app.visual_script.context = crate::editor::visual_script::GraphContext::Entity;
        app.visual_script.entity_id = id;
        app.visual_script.entity_name = name;
        app.visual_script.scene_id = None;
        app.visual_script.loaded = false;
        app.visual_script.error = None;
        app.entity_properties.nested = None;
        return;
    }

    let (modal_id, title) = match &nested {
        NativeNested::ScriptEditor { name, .. } => (
            "entity_props_script_editor",
            if name.is_empty() {
                t(locale, "Nuevo script (Rhai)", "New script (Rhai)")
            } else {
                format!("{}: {name}", t(locale, "Editar script", "Edit script"))
            },
        ),
        NativeNested::Animation { index, .. } => (
            "entity_props_animation_editor",
            if index.is_some() {
                t(locale, "Editar animación", "Edit animation")
            } else {
                t(locale, "Nueva animación", "New animation")
            },
        ),
        NativeNested::VisualScript => unreachable!(),
    };

    let mut save = false;
    let mut cancel = false;
    let syntax = rhai_syntax();
    let sprites: Vec<(String, String)> = app
        .assets
        .sprites
        .iter()
        .map(|sprite| (sprite.path.clone(), sprite.name.clone()))
        .collect();

    let modal = egui::Modal::new(egui::Id::new(modal_id))
        .frame(
            egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
                .fill(theme::PANEL_BG)
                .stroke(Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::same(12)),
        )
        .show(ctx, |ui| {
            ui.set_width(440.0);
            ui.label(body(&title, 14.0));
            ui.add_space(8.0);
            match &mut app.entity_properties.nested {
                Some(NativeNested::ScriptEditor { name, source }) => {
                    ui.add(
                        egui::TextEdit::singleline(name)
                            .desired_width(f32::INFINITY)
                            .hint_text(t(locale, "Nombre del script", "Script name")),
                    );
                    ui.add_space(6.0);
                    CodeEditor::default()
                        .id_source("entity_props_rhai")
                        .with_rows(16)
                        .with_fontsize(13.0)
                        .with_theme(RER_THEME)
                        .with_numlines(true)
                        .desired_width(ui.available_width())
                        .show(ui, source, &syntax);
                    save = !name.trim().is_empty();
                }
                Some(NativeNested::Animation {
                    name,
                    sprite,
                    fps,
                    loop_,
                    columns,
                    rows,
                    ..
                }) => {
                    egui::ComboBox::from_id_salt("entity_props_anim_sprite")
                        .selected_text(
                            sprites
                                .iter()
                                .find(|(path, _)| path == sprite)
                                .map(|(_, name)| name.clone())
                                .unwrap_or_else(|| sprite.clone()),
                        )
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (path, label) in &sprites {
                                ui.selectable_value(sprite, path.clone(), label);
                            }
                        });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(muted(&t(locale, "Nombre", "Name"), 12.0));
                        ui.add(egui::TextEdit::singleline(name).desired_width(180.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label(muted(&t(locale, "FPS", "FPS"), 12.0));
                        ui.add(egui::DragValue::new(fps).range(1..=60));
                        ui.toggle_value(loop_, &t(locale, "Bucle", "Loop"));
                    });
                    ui.horizontal(|ui| {
                        ui.label(muted(&t(locale, "Columnas", "Columns"), 12.0));
                        ui.add(egui::DragValue::new(columns).range(1..=32));
                        ui.label(muted(&t(locale, "Filas", "Rows"), 12.0));
                        ui.add(egui::DragValue::new(rows).range(1..=32));
                    });
                    save = !name.trim().is_empty() && !sprite.is_empty();
                }
                _ => {}
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::modal_button(
                        ui,
                        icons::FLOPPY_DISK,
                        &t(locale, "Guardar", "Save"),
                        widgets::ModalButtonStyle::Primary,
                        100.0,
                        save,
                    ) {
                        save = true;
                    }
                    ui.add_space(6.0);
                    if widgets::modal_button(
                        ui,
                        icons::X,
                        &t(locale, "Cancelar", "Cancel"),
                        widgets::ModalButtonStyle::Secondary,
                        100.0,
                        true,
                    ) {
                        cancel = true;
                    }
                });
            });
        });

    if modal.should_close() {
        cancel = true;
    }
    if cancel {
        app.entity_properties.nested = None;
        return;
    }
    if !save {
        return;
    }

    match nested {
        NativeNested::ScriptEditor { name, source } => {
            if let Some(entity) = app.entity_properties.entity.clone() {
                let key = entity.id.to_string();
                let mut scripts: Vec<Value> = app
                    .project_state
                    .entity_meta
                    .get(&key)
                    .and_then(|meta| meta.get("scripts"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if let Some(existing) = scripts
                    .iter_mut()
                    .find(|script| script.get("name").and_then(Value::as_str) == Some(name.trim()))
                {
                    existing["source"] = json!(source);
                } else {
                    scripts.push(json!({ "name": name.trim(), "source": source }));
                }
                if let Some(meta) = app.project_state.entity_meta.get_mut(&key) {
                    meta["scripts"] = json!(scripts);
                }
                app.send_engine(json!({
                    "cmd": "load_script",
                    "id": entity.id,
                    "path": name.trim(),
                    "source": source,
                }));
            }
        }
        NativeNested::Animation {
            name,
            sprite,
            fps,
            loop_,
            columns,
            rows,
            ..
        } => {
            if let Some(entity) = app.entity_properties.entity.clone() {
                let cell_w = 64u32;
                let cell_h = 64u32;
                let frames: Vec<Value> = (0..columns.saturating_mul(rows))
                    .map(|index| {
                        json!({
                            "path": sprite,
                            "src_x": (index % columns) * cell_w,
                            "src_y": (index / columns) * cell_h,
                            "src_w": cell_w,
                            "src_h": cell_h,
                        })
                    })
                    .collect();
                commands.push(json!({
                    "cmd": "set_animation",
                    "id": entity.id,
                    "name": name.trim(),
                    "frames": frames,
                    "fps": fps,
                    "loop_": loop_,
                    "flip_horizontal": false,
                    "audio_path": Value::Null,
                    "logical_w": columns * cell_w,
                    "logical_h": rows * cell_h,
                    "scripts": [],
                    "is_cancelable": true,
                }));
            }
        }
        NativeNested::VisualScript => {}
    }
    app.entity_properties.nested = None;
}

fn confirm_dialog(
    ctx: &egui::Context,
    app: &mut EditorApp,
    locale: Locale,
    commands: &mut Vec<Value>,
) {
    let Some(dialog) = app.entity_properties.confirm.clone() else {
        return;
    };
    let mut close = false;
    let mut confirmed = false;
    egui::Window::new(&dialog.title)
        .id(egui::Id::new("entity_properties_confirm"))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(280.0);
            ui.label(body(&dialog.message, 13.0));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let cancel = t(locale, "Cancelar", "Cancel");
                    let confirm = dialog.confirm_label.clone();
                    if outline_button(ui, "", &confirm, theme::DANGER, true, None) {
                        confirmed = true;
                    }
                    if outline_button(ui, "", &cancel, theme::OUTLINE_SECONDARY, true, None) {
                        close = true;
                    }
                });
            });
        });
    if confirmed {
        match dialog.action {
            ConfirmAction::RemoveEntity(id) => {
                commands.push(json!({ "cmd": "remove_entity", "id": id }));
            }
            ConfirmAction::RemoveMultiple(ids) => {
                for id in ids {
                    commands.push(json!({ "cmd": "remove_entity", "id": id }));
                }
            }
            ConfirmAction::DeleteAnimation(index) => {
                if let Some(anim) = app
                    .entity_properties
                    .entity()
                    .and_then(|e| e.animations.get(index))
                {
                    let id = app.entity_properties.entity().map(|e| e.id).unwrap_or(0);
                    commands.push(json!({ "cmd": "stop_animation", "id": id }));
                    commands.push(json!({
                        "cmd": "remove_animation", "id": id, "name": anim.name.clone()
                    }));
                }
            }
            ConfirmAction::DeleteScript(name) => {
                let id = app.entity_properties.entity().map(|e| e.id).unwrap_or(0);
                let remaining: Vec<Value> = app
                    .entity_properties
                    .scripts
                    .iter()
                    .filter(|s| s.name != name)
                    .map(|s| json!({ "name": s.name, "source": s.source }))
                    .collect();
                if remaining.is_empty() {
                    commands.push(json!({ "cmd": "unload_script", "id": id }));
                } else {
                    commands.push(json!({ "cmd": "unload_script", "id": id }));
                    for script in &remaining {
                        commands.push(json!({
                            "cmd": "load_script", "id": id,
                            "path": script["name"], "source": script["source"]
                        }));
                    }
                }
            }
            ConfirmAction::ConvertToBlueprint(id) => {
                let Some(entity) = app.entity_properties.entity().cloned() else {
                    return;
                };
                let meta = app
                    .project_state
                    .entity_meta_for(id)
                    .cloned()
                    .unwrap_or_default();
                let path = meta
                    .get("visualModelPath")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .or_else(|| meta.get("path").and_then(Value::as_str))
                    .unwrap_or_default();
                let blueprint_id = format!("bp_{}", now_millis());
                let name = format!("{}_blueprint", entity.name.replace([' ', '/', '\\'], "_"));
                let mut blueprint = json!({
                    "id": blueprint_id,
                    "name": name,
                    "category": if app.entity_properties.is_character {
                        "character"
                    } else if app.entity_properties.is_environment {
                        "environment"
                    } else {
                        "object"
                    },
                    "kind": meta.get("kind").and_then(Value::as_str).unwrap_or("model"),
                    "path": path,
                    "model": path,
                    "scale": entity.scale,
                    "rotation": entity.rotation,
                });
                if let Some(animations) = meta.get("animations") {
                    blueprint["animations"] = animations.clone();
                }
                if let Some(scripts) = meta.get("scripts") {
                    blueprint["scripts"] = scripts.clone();
                }
                app.project_state.blueprints.push(blueprint);
                if let Some(entry) = app.project_state.entity_meta.get_mut(&id.to_string()) {
                    entry["blueprintId"] = json!(blueprint_id);
                }
            }
        }
    }
    if confirmed || close {
        app.entity_properties.confirm = None;
    }
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}
