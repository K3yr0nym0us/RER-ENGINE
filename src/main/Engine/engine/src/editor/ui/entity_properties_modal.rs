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
use crate::editor::assets::AssetEntry;
use crate::editor::cli::Locale;
use crate::editor::entity_properties::{
    ConfirmAction, EntityPropertiesState, NativeNested, ProjectileCfg, PropTab,
};
use crate::editor::i18n::tr;
use crate::editor::theme;
use crate::editor::ui::number_draft::format_number;
use crate::editor::ui::sidebar_right::particles::emitter_command_for;
use crate::engine3d::ipc::normalize_model_library_category;

use super::particles_config_modal::particle_properties_fields;
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

/// Campo numérico de transformación con botones `−`/`+` a la derecha, uno al
/// lado del otro y de la misma altura que el input. El paso es 0.1 para valores
/// decimales y 1 para enteros.
fn transform_number_field(ui: &mut egui::Ui, draft: &mut String, width: f32) -> egui::Response {
    const BUTTON_W: f32 = 15.0;
    const HEIGHT: f32 = 26.0;
    let text_width = (width - 2.0 * BUTTON_W).max(24.0);
    let mut response = ui.add_sized([text_width, HEIGHT], egui::TextEdit::singleline(draft));
    let value = draft.trim().parse::<f32>().ok().unwrap_or(0.0);
    let step = if value.fract().abs() > 1e-4 { 0.1 } else { 1.0 };
    let mut delta = 0.0;
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    if ui.add_sized([BUTTON_W, HEIGHT], egui::Button::new("−")).clicked() {
        delta = -step;
    }
    if ui.add_sized([BUTTON_W, HEIGHT], egui::Button::new("+")).clicked() {
        delta = step;
    }
    if delta != 0.0 {
        *draft = format_number(value + delta);
        response.mark_changed();
    }
    response
}

/// Reparte el ancho disponible en `count` celdas iguales para los campos de
/// transformación. Se calcula una sola vez: `ui.available_width()` se reduce en
/// cada iteración dentro de un `horizontal`, lo que daba inputs de tamaños
/// distintos (el primero ancho, el último estrecho).
fn axis_width(ui: &egui::Ui, count: usize) -> f32 {
    let gap = 4.0;
    (ui.available_width() - gap * (count - 1) as f32) / count as f32
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
    // La modal debe usar la biblioteca viva del motor (`models_list` /
    // `model_asset_loaded`), no el snapshot del manifest: `project_state.models`
    // nunca se actualiza tras importar un modelo y guarda `category: object`.
    // Se clona antes de tomar `&mut app.entity_properties` más abajo.
    let models = app.assets.models.clone();
    let active_scene_id = app.scenes.active_scene_id;

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
                } else if state.entity.is_some() || state.particle_draft.is_some() {
                    toolbar(ui, state, locale, &mut commands);
                    ui.add_space(6.0);
                    model_actions(ui, state, &models, active_scene_id, locale);
                    ui.add_space(8.0);
                    tabs(
                        ui,
                        state,
                        locale,
                        &mut commands,
                        &app.project_state.interaction_tooltips,
                    );
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

    // La modal externa usa esta altura para ajustarse al contenido real.
    app.entity_properties.content_height = ui.min_rect().height();

    // Diálogos anidados: se pintan encima de todo el contenido. El selector de
    // modelos NO está aquí: es una ventana nativa aparte (`NativeModal::ModelPicker`).
    confirm_dialog(&ctx, app, locale, &mut commands);
    nested_dialog(&ctx, app, locale, &mut commands);

    // El checkbox de root motion vive en `entityMeta[id].animations[]`; el motor
    // recibe el comando, y el editor actualiza la fuente de verdad del `.save`.
    for command in &commands {
        if command.get("cmd").and_then(Value::as_str) != Some("set_animation_root_motion") {
            continue;
        }
        let Some(id) = command.get("id").and_then(Value::as_u64) else {
            continue;
        };
        let Some(name) = command.get("name").and_then(Value::as_str) else {
            continue;
        };
        let enabled = command
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let Some(meta) = app.project_state.entity_meta.get_mut(&id.to_string())
            && let Some(animations) = meta.get_mut("animations").and_then(Value::as_array_mut)
            && let Some(anim) = animations
                .iter_mut()
                .find(|a| a.get("name").and_then(Value::as_str) == Some(name))
        {
            anim["root_motion"] = json!(enabled);
        }
    }

    for command in commands {
        app.send_engine(command);
    }

    // Tras «Aplicar» en la pestaña de partículas, la fila del sidebar (fuente de
    // los snapshots y del próximo click derecho) refleja los valores editados.
    if app.entity_properties.particle_applied {
        app.entity_properties.particle_applied = false;
        if let Some(draft) = app.entity_properties.particle_draft.clone() {
            if let Some(row) = app
                .right_sidebar
                .particles
                .emitters
                .iter_mut()
                .find(|row| row.id == draft.id)
            {
                *row = draft;
            }
        }
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
    let id = state
        .entity
        .as_ref()
        .map(|e| e.id)
        .or_else(|| state.particle_draft.as_ref().map(|d| d.id))
        .unwrap_or(0);
    let editing = state.editing_name;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        let name_width = (ui.available_width() - 112.0).max(60.0);
        let mut name = state.name_draft.clone();
        // El frame por defecto del TextEdit usa `PANEL_BG`, el mismo color que
        // el fondo de la modal, así que el campo casi no se ve. Le damos un
        // fondo y un borde propios (también cuando está deshabilitado, que es cuando
        // se muestra el nombre en modo lectura).
        let name_frame = egui::Frame::new()
            .fill(theme::INPUT_BG)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(6, 3));
        let name_resp = ui.add_enabled(
            editing,
            egui::TextEdit::singleline(&mut name)
                .frame(name_frame)
                .desired_width(name_width)
                .text_color(theme::TEXT),
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
        } else if icon_button(
            ui,
            icons::PENCIL,
            theme::WARNING,
            &t(locale, "Editar", "Edit"),
            true,
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

/// Selector «Reemplazar modelo» a partir de la biblioteca viva del motor.
/// Muestra solo los modelos de la categoría de la entidad (la misma
/// normalización que el acordeón «Crear entidad») y deja el modelo actual en la
/// lista, preseleccionado, para que se vea qué tiene asignado.
fn model_picker_for(
    models: &[AssetEntry],
    active_scene_id: u64,
    intent: &str,
    current: &str,
) -> crate::editor::entity_properties::ModelPicker {
    let key = |v: &str| {
        v.replace('\\', "/")
            .trim_end_matches('/')
            .to_ascii_lowercase()
    };
    let base = |v: &str| key(v.rsplit(['/', '\\']).next().unwrap_or(v));
    let current_key = key(current);
    let current_base = base(current);
    let in_category = |m: &&crate::editor::assets::AssetEntry| {
        m.is_visible_in_scene(active_scene_id)
            && normalize_model_library_category(m.category.as_deref())
                .unwrap_or_else(|| "object".to_string())
                == match intent {
                    "character" => "character",
                    "environment" => "environment",
                    "weapon" => "weapon",
                    "projectile" => "projectile",
                    "animal" => "animal",
                    "creature" => "creature",
                    _ => "object",
                }
    };
    // El meta puede guardar la ruta de origen, el `model_id` o un marcador:
    // compara por path, por nombre de archivo, por label y por `model_id`.
    let is_current = |m: &&crate::editor::assets::AssetEntry| {
        let label = if m.name.trim().is_empty() {
            base(&m.path)
        } else {
            m.name.clone()
        };
        key(&m.path) == current_key
            || base(&m.path) == current_base
            || key(&label) == current_base
            || m.model_id.as_deref().is_some_and(|id| key(id) == current_key)
    };

    // Nombre del modelo asignado, para mostrarlo encima del select.
    let current_label = models
        .iter()
        .find(|m| is_current(m))
        .map(|m| {
            if m.name.trim().is_empty() {
                base(&m.path)
            } else {
                m.name.clone()
            }
        })
        .unwrap_or_else(|| if current.is_empty() { "-".to_string() } else { base(current) });

    let mut choices: Vec<crate::editor::entity_properties::ModelChoice> = models
        .iter()
        .filter(|m| in_category(m))
        // El modelo ya asignado no es una opción válida de reemplazo.
        .filter(|m| !is_current(m))
        .map(|m| crate::editor::entity_properties::ModelChoice {
            label: if m.name.trim().is_empty() {
                base(&m.path)
            } else {
                m.name.clone()
            },
            path: m.path.clone(),
            state: m.state.clone().unwrap_or_else(|| "ready".to_string()),
        })
        .collect();
    choices.sort_by_key(|choice| choice.label.to_ascii_lowercase());

    crate::editor::entity_properties::ModelPicker {
        choices,
        // El select arranca vacío: el usuario elige explícitamente el nuevo modelo.
        selected: None,
        active_scene_id,
        intent: intent.to_string(),
        current: current.to_string(),
        current_label,
    }
}

fn model_actions(
    ui: &mut egui::Ui,
    state: &mut EntityPropertiesState,
    models: &[AssetEntry],
    active_scene_id: u64,
    locale: Locale,
) {
    if state.is_collider || state.is_execution_area {
        return;
    }
    let is_3d = state.is_3d;
    let id = state.entity.as_ref().map(|e| e.id).unwrap_or(0);
    // Un emisor de partículas no usa modelo: solo se muestra «Convertir a
    // Blueprint» y ocupa todo el ancho. En entidades normales, 50% cada uno.
    let show_replace = is_3d && !state.is_from_blueprint && state.particle_draft.is_none();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
        let half = (ui.available_width() - ui.spacing().item_spacing.x) * 0.5;
        let action_width = if show_replace { half } else { ui.available_width() };
        if show_replace {
            let label = t(locale, "Reemplazar modelo", "Replace model");
            if outline_button(ui, icons::CUBE, &label, theme::INFO, true, Some(half)) {
                // Cada entidad solo ofrece modelos de su propia categoría.
                let intent = if state.is_player || state.is_character {
                    "character"
                } else if state.is_projectile {
                    "projectile"
                } else if state.is_environment {
                    "environment"
                } else {
                    state.entity_category.as_deref().unwrap_or("object")
                };
                let current = state.current_model_path.clone().unwrap_or_default();
                state.model_picker = Some(model_picker_for(
                    models,
                    active_scene_id,
                    intent,
                    &current,
                ));
                // El inspector se cierra mientras el selector está abierto. Si se
                // cancela (X de la ventana) se reabrirá; si se confirma, no.
                state.open = false;
                state.reopen_after_model_picker = true;
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
                Some(action_width),
            );
            ui.label(RichText::new("").size(1.0)).on_hover_text(tooltip);
        } else {
            let label = t(locale, "Convertir a Blueprint", "Convert to Blueprint");
            if outline_button(ui, icons::SQUARE, &label, theme::ACCENT, true, Some(action_width)) {
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

/// Selector de modelos en su propia ventana nativa (antes submodal de Electron).
/// Se dibuja igual que el resto de modales del sistema: `CentralPanel` con el
/// frame y el espaciado de tema, sin un `egui::Window` anidado en el inspector.
pub(crate) fn model_picker_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    let Some(previous) = app.entity_properties.model_picker.take() else {
        return;
    };
    let mut close = false;
    let mut commands: Vec<Value> = Vec::new();

    // Reconstruye la lista cada frame desde la biblioteca viva del motor: los
    // modelos en carga no entran en el combo y aparecen como spinner; cuando
    // llega `model_asset_loaded` se vuelven elegibles sin reabrir la modal.
    let models = app.assets.models.clone();
    let mut picker = model_picker_for(
        &models,
        previous.active_scene_id,
        &previous.intent,
        &previous.current,
    );
    let is_ready = |path: &str| {
        picker
            .choices
            .iter()
            .find(|choice| choice.path == path)
            .is_some_and(|choice| choice.state == "ready")
    };
    if previous
        .selected
        .as_deref()
        .is_some_and(is_ready)
    {
        picker.selected = previous.selected;
    } else if !is_ready(picker.selected.as_deref().unwrap_or("")) {
        // Nunca preseleccionar un modelo en carga: el actual ya viene marcado
        // de `model_picker_for`; si no está listo, se deja sin selección.
        picker.selected = None;
    }

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL_BG)
                .inner_margin(egui::Margin::same(12)),
        )
        .show(ui, |ui| {
            section_label(ui, &t(locale, "Modelo actual:", "Current model:"));
            ui.label(body(&picker.current_label, 12.0));
            ui.add_space(8.0);
            section_label(ui, &t(locale, "Nuevo modelo", "New model"));

            let selected_label = picker
                .selected
                .as_deref()
                .and_then(|path| picker.choices.iter().find(|c| c.path == path))
                .map(|choice| choice.label.clone())
                .unwrap_or_else(|| "-".to_string());
            let mut changed: Option<String> = None;
            egui::ComboBox::from_id_salt("replace_model_picker")
                .width(ui.available_width())
                .selected_text(&selected_label)
                .show_ui(ui, |ui| {
                    for choice in &picker.choices {
                        let loading = choice.state == "importing";
                        let failed = choice.state == "failed";
                        // Los que aún cargan (o fallaron) se muestran con
                        // spinner pero no se pueden elegir.
                        let enabled = !loading && !failed;
                        ui.horizontal(|ui| {
                            if loading {
                                ui.add(egui::Spinner::new().size(12.0));
                            }
                            // Ocupa todo el ancho de la opción: el click debe
                            // funcionar en cualquier punto de la fila, no solo
                            // sobre el texto del nombre.
                            let width = ui.available_width();
                            let response = ui.add_enabled(
                                enabled,
                                egui::Button::selectable(
                                    picker.selected.as_deref() == Some(choice.path.as_str()),
                                    choice.label.clone(),
                                )
                                .frame(false)
                                .min_size(Vec2::new(width, 0.0)),
                            );
                            // `add_enabled` ya marca el Response como deshabilitado;
                            // la guarda extra evita cualquier selección residual.
                            if enabled && response.clicked() {
                                changed = Some(choice.path.clone());
                            }
                        });
                    }
                });
            if let Some(path) = changed {
                picker.selected = Some(path);
            }

            ui.add_space(12.0);
            // Se renderiza siempre, pero solo se habilita con un modelo elegido.
            if widgets::modal_button(
                ui,
                icons::CUBE,
                &t(
                    locale,
                    "Reemplazar con este modelo",
                    "Replace with this model",
                ),
                widgets::ModalButtonStyle::Primary,
                ui.available_width(),
                picker.selected.is_some(),
            ) && let Some(path) = picker.selected.clone()
                && picker
                    .choices
                    .iter()
                    .find(|choice| choice.path == path)
                    .is_some_and(|choice| {
                        !matches!(choice.state.as_str(), "importing" | "failed")
                    })
            {
                let id = app.entity_properties.entity().map(|e| e.id).unwrap_or(0);
                commands.push(json!({
                    "cmd": "replace_entity_model", "id": id, "path": path
                }));
                close = true;
            }
        });

    if !close {
        app.entity_properties.model_picker = Some(picker);
    } else {
        // Confirmado: no se reabre el inspector (la X del picker sí lo hará).
        app.entity_properties.reopen_after_model_picker = false;
    }
    for command in commands {
        app.send_engine(command);
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
    tooltips: &[Value],
) {
    let is_3d = state.is_3d;
    let has_embedded = state
        .entity
        .as_ref()
        .is_some_and(|e| e.animations.iter().any(|a| a.embedded_in_model));
    let is_multi = state.multi_ids.len() > 1;

    let tabs = if state.particle_draft.is_some() {
        // Un emisor solo ofrece transform, sus props y programación.
        vec![PropTab::Transform, PropTab::Particles, PropTab::Scripting]
    } else {
        let mut list = vec![PropTab::Transform];
        if !state.is_collider && !state.is_execution_area {
            list.push(PropTab::Physics);
        }
        if !state.is_collider && !state.is_execution_area && (!is_3d || has_embedded) {
            list.push(PropTab::Animations);
        }
        if !state.is_collider {
            list.push(PropTab::Scripting);
        }
        if is_3d && state.can_have_bone_physics {
            list.push(PropTab::Bones);
        }
        if state.is_projectile && !is_multi {
            list.push(PropTab::Projectile);
        }
        if is_3d && !state.is_collider && !is_multi {
            list.push(PropTab::Interaction);
        }
        list
    };
    if !tabs.contains(&state.active_tab) {
        state.active_tab = tabs.first().copied().unwrap_or(PropTab::Transform);
    }

    let labels: Vec<String> = tabs
        .iter()
        .map(|tab| match tab {
            PropTab::Transform => t(locale, "Transformaciones", "Transformations"),
            PropTab::Particles => t(locale, "Partículas", "Particles"),
            PropTab::Physics => t(locale, "Física", "Physics"),
            PropTab::Animations => t(locale, "Animaciones", "Animations"),
            PropTab::Scripting => t(locale, "Programar entidad", "Program entity"),
            PropTab::Bones => t(locale, "Huesos", "Bones"),
            PropTab::Projectile => t(locale, "Proyectil", "Projectile"),
            PropTab::Interaction => t(locale, "Interacción", "Interaction"),
        })
        .collect();
    // Ancho que necesitan las pestañas en una sola fila; la ventana lo usa para
    // adaptarse al contenido en vez de quedarse en 348 px y recortarlas.
    let font = FontId::proportional(13.0);
    let tabs_width: f32 = labels
        .iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap(label.clone(), font.clone(), theme::TEXT)
                .size()
                .x
                + 16.0
        })
        .sum::<f32>()
        + 2.0 * labels.len().saturating_sub(1) as f32;
    // Ancho intrínseco de las pestañas, independiente del ancho de la ventana.
    state.content_width = tabs_width + 80.0; // márgenes del frame y barra de scroll

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
        for (tab, label) in tabs.iter().zip(labels) {
            if prop_tab(ui, &label, state.active_tab == *tab) {
                state.active_tab = *tab;
            }
        }
    });
    ui.add_space(10.0);

    match state.active_tab {
        PropTab::Transform => transform_tab(ui, state, locale, commands),
        PropTab::Particles => particles_tab(ui, state, locale, commands),
        PropTab::Physics => physics(ui, state, locale, commands),
        PropTab::Animations => animations_tab(ui, state, locale, commands, is_3d),
        PropTab::Scripting => scripting_tab(ui, state, locale, commands),
        PropTab::Bones => bones_tab(ui, state, locale, commands),
        PropTab::Projectile => projectile_tab(ui, state, locale, commands, is_3d),
        PropTab::Interaction => interaction_tab(ui, state, locale, commands, tooltips),
    }
}

/// Pestaña condicional del inspector cuando el seleccionado es un emisor de
/// partículas. Reutiliza los mismos campos por material que la modal de emisores.
fn particles_tab(
    ui: &mut egui::Ui,
    state: &mut EntityPropertiesState,
    locale: Locale,
    commands: &mut Vec<Value>,
) {
    let Some(mut draft) = state.particle_draft.clone() else {
        return;
    };
    let material = crate::editor::ui::sidebar_right::particles::MATERIAL_OPTIONS
        .iter()
        .find(|(wire, _, _)| *wire == draft.material)
        .map(|option| tr(locale, option.1, option.2))
        .unwrap_or_else(|| draft.material);
    section_label(ui, &material);
    ui.add_space(6.0);
    particle_properties_fields(ui, locale, &mut draft);

    ui.add_space(18.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::modal_button(
                ui,
                icons::CHECK,
                &t(locale, "Aplicar", "Apply"),
                widgets::ModalButtonStyle::Primary,
                ui.available_width(),
                true,
            ) {
                commands.push(emitter_command_for(&draft));
                state.particle_applied = true;
                state.particle_draft = Some(draft);
            } else {
                state.particle_draft = Some(draft);
            }
        });
    });
}

fn transform_tab(
    ui: &mut egui::Ui,
    state: &mut EntityPropertiesState,
    locale: Locale,
    commands: &mut Vec<Value>,
) {
    let id = state
        .entity
        .as_ref()
        .map(|e| e.id)
        .or_else(|| state.particle_draft.as_ref().map(|d| d.id))
        .unwrap_or(0);
    let is_particle = state.particle_draft.is_some();
    let is_editor_camera = state.is_editor_camera && state.is_3d;
    let is_2d = !state.is_3d;
    let is_play_character =
        state.is_player && !state.is_collider && !state.is_execution_area && !is_2d;

    section_label(ui, &t(locale, "Posición", "Position"));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        let w = axis_width(ui, 3);
        if is_particle {
            // La posición del emisor vive en su propia fila: al editarla se
            // reenvía el comando de upsert del emisor con la nueva posición.
            let mut row = state.particle_draft.clone().expect("is_particle");
            let mut changed = false;
            for axis in 0..3 {
                ui.allocate_ui_with_layout(
                    Vec2::new(w, 26.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        axis_label(ui, axis);
                        let mut text = row.position[axis].text.clone();
                        if transform_number_field(ui, &mut text, w - 22.0).changed()
                            && let Some(value) = parse_f32(&text)
                        {
                            row.position[axis].force(value);
                            changed = true;
                        } else {
                            row.position[axis].text = text;
                        }
                    },
                );
            }
            if changed {
                commands.push(emitter_command_for(&row));
                state.particle_applied = true;
            }
            state.particle_draft = Some(row);
        } else {
            for (axis, draft_value) in state.pos_draft.iter_mut().enumerate() {
                ui.allocate_ui_with_layout(
                    Vec2::new(w, 26.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        axis_label(ui, axis);
                        let mut draft = draft_value.clone();
                        if transform_number_field(ui, &mut draft, w - 22.0).changed()
                            && let Some(value) = parse_f32(&draft)
                        {
                            commands.push(json!({
                                "cmd": "set_transform", "id": id,
                                "position_axis": { "axis": axis, "value": value }
                            }));
                        }
                        *draft_value = draft;
                    },
                );
            }
        }
    });
    ui.add_space(8.0);

    // Los emisores no tienen rotación: se omite por completo en partículas.
    if !is_editor_camera && !is_particle {
        if is_2d {
            section_label(
                ui,
                &t(locale, "Rotación (cuaternión)", "Rotation (quaternion)"),
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                let w = axis_width(ui, 4);
                for axis in 0..4 {
                    ui.allocate_ui_with_layout(Vec2::new(w, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        axis_label(ui, axis);
                        let mut draft = state.rot_draft[axis].clone();
                        if transform_number_field(ui, &mut draft, w - 22.0).changed() {
                            let mut drafts = state.rot_draft.clone();
                            drafts[axis] = draft.clone();
                            let values: Option<Vec<f32>> = drafts.iter().map(|v| parse_f32(v)).collect();
                            if let Some(values) = values
                                && !is_particle
                            {
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
                let w = axis_width(ui, 3);
                for axis in 0..3 {
                    ui.allocate_ui_with_layout(Vec2::new(w, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        axis_label(ui, axis);
                        let mut draft = state.rot_draft[axis].clone();
                        if transform_number_field(ui, &mut draft, w - 22.0).changed()
                            && let Some(value) = parse_f32(&draft)
                        {
                            let rounded = value.round();
                            let base = state.rot_focus_deg[axis];
                            let delta = shortest_delta(base, rounded);
                            if delta.abs() > 1e-6 {
                                if !is_particle {
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
            let w = axis_width(ui, 3);
            for axis in 0..3 {
                ui.allocate_ui_with_layout(Vec2::new(w, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    axis_label(ui, axis);
                    let mut draft = state.scl_draft[axis].clone();
                    if transform_number_field(ui, &mut draft, w - 22.0).changed()
                        && let Some(value) = parse_f32(&draft)
                    {
                        if is_particle {
                            // El emisor no tiene transform: su «escala» es el tamaño
                            // de sus partículas. Uniforme en los tres ejes.
                            for axis_draft in state.scl_draft.iter_mut() {
                                *axis_draft = format_number(value);
                            }
                            if let Some(row) = state.particle_draft.as_mut() {
                                row.size.force(value);
                                commands.push(emitter_command_for(row));
                                state.particle_applied = true;
                            }
                        } else if state.lock_proportions {
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
            if is_3d && anim.embedded_in_model {
                let mut root_motion = anim.root_motion;
                if ui
                    .checkbox(
                        &mut root_motion,
                        t(
                            locale,
                            "Esta animación controla el movimiento",
                            "This animation controls movement",
                        ),
                    )
                    .clicked()
                {
                    if let Some(entity) = state.entity.as_mut()
                        && let Some(a) = entity.animations.get_mut(index)
                    {
                        a.root_motion = root_motion;
                    }
                    commands.push(json!({
                        "cmd": "set_animation_root_motion",
                        "id": id,
                        "name": anim.name.clone(),
                        "enabled": root_motion,
                    }));
                }
            }
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
    tooltips: &[Value],
) {
    let id = state.entity.as_ref().map(|e| e.id).unwrap_or(0);
    let mut enabled = state.interaction_enabled_draft;
    // Tanto el checkbox como su etiqueta alternan el estado. `ui.checkbox` ya
    // invierte `enabled` al pulsarlo; la etiqueta lo invierte explícitamente.
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui.checkbox(&mut enabled, "").clicked() {
            changed = true;
        }
        let label = ui
            .label(body(&t(locale, "Interactuable", "Interactable"), 12.0))
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if label.clicked() {
            enabled = !enabled;
            changed = true;
        }
    });
    if changed {
        state.interaction_enabled_draft = enabled;
        commit_interaction(state, id, commands);
    }
    ui.add_space(8.0);

    // Tooltip con nombre creado en el SidebarLeft. Solo se puede elegir cuando
    // la entidad es interactuable; si no, el select queda deshabilitado en «—».
    section_label(ui, &t(locale, "Tooltip", "Tooltip"));
    let tooltip_name = |selected: &str| {
        tooltips
            .iter()
            .find(|tooltip| tooltip.get("id").and_then(Value::as_str) == Some(selected))
            .and_then(|tooltip| tooltip.get("name").and_then(Value::as_str))
            .unwrap_or("—")
    };
    let selected_name = if enabled {
        state
            .interaction_tooltip_id_draft
            .as_deref()
            .map(tooltip_name)
            .unwrap_or("—")
    } else {
        "—"
    };
    ui.add_enabled_ui(enabled, |ui| {
        egui::ComboBox::from_id_salt("interaction_tooltip_select")
            .selected_text(selected_name)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_value(&mut state.interaction_tooltip_id_draft, None, "—")
                    .changed()
                {
                    commit_interaction(state, id, commands);
                }
                for tooltip in tooltips {
                    let tooltip_id = tooltip
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let name = tooltip
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("Tooltip")
                        .to_string();
                    let selected =
                        state.interaction_tooltip_id_draft.as_deref() == Some(&tooltip_id);
                    if ui.selectable_label(selected, &name).clicked() {
                        state.interaction_tooltip_id_draft = Some(tooltip_id);
                        commit_interaction(state, id, commands);
                    }
                }
            });
    });
    // Los tooltips se gestionan en el accordion del SidebarLeft, no aquí.
    ui.add_space(6.0);
    egui::Frame::new()
        .fill(theme::ACCORDION_BG)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(icons::INFO)
                        .size(14.0)
                        .color(theme::INFO),
                );
                ui.label(muted(
                    &t(
                        locale,
                        "Crea y edita los tooltips en el acordeón «Tooltips de interacción» del panel izquierdo.",
                        "Create and edit tooltips in the «Interaction tooltips» accordion of the left sidebar.",
                    ),
                    11.0,
                ));
            });
        });
}

fn commit_interaction(state: &EntityPropertiesState, id: u64, commands: &mut Vec<Value>) {
    commands.push(json!({
        "cmd": "set_interactable",
        "id": id,
        "enabled": state.interaction_enabled_draft,
        "tooltip_id": state.interaction_tooltip_id_draft,
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
                        ui.toggle_value(loop_, t(locale, "Bucle", "Loop"));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn model(path: &str, name: &str, category: Option<&str>, scene_id: Option<u32>) -> AssetEntry {
        AssetEntry {
            name: name.to_string(),
            path: path.to_string(),
            model_id: None,
            asset: None,
            category: category.map(str::to_string),
            state: Some("ready".to_string()),
            scene_id,
        }
    }

    fn model_with_state(
        path: &str,
        name: &str,
        category: Option<&str>,
        state: &str,
    ) -> AssetEntry {
        let mut entry = model(path, name, category, None);
        entry.state = Some(state.to_string());
        entry
    }

    fn paths(picker: &crate::editor::entity_properties::ModelPicker) -> Vec<&str> {
        picker.choices.iter().map(|c| c.path.as_str()).collect()
    }

    #[test]
    fn replacing_player_model_lists_only_character_models() {
        let models = vec![
            model("model_hero", "Hero", Some("character"), None),
            model("model_rock", "Rock", Some("object"), None),
            model("model_old", "Legacy", None, None),
        ];

        let for_player = model_picker_for(&models, 1, "character", "");
        assert_eq!(paths(&for_player), ["model_hero"]);
        assert_eq!(for_player.selected, None);

        let for_object = model_picker_for(&models, 1, "object", "");
        assert_eq!(paths(&for_object), ["model_old", "model_rock"]);
    }

    #[test]
    fn current_model_is_excluded_and_shown_above_the_select() {
        let models = vec![
            model("model_hero", "Hero", Some("character"), None),
            model("model_other", "Other", Some("character"), Some(7)),
            model("model_old_hero", "Old Hero", Some("character"), None),
        ];

        let picker = model_picker_for(&models, 1, "character", "model_old_hero");
        // El actual se excluye y se muestra arriba; el de otra escena se oculta.
        assert_eq!(paths(&picker), ["model_hero"]);
        assert_eq!(picker.current_label, "Old Hero");
        assert_eq!(picker.selected, None);
    }

    #[test]
    fn weapons_and_projectiles_only_list_their_own_category() {
        let models = vec![
            model("model_sword", "Sword", Some("weapon"), None),
            model("model_bullet", "Bullet", Some("projectile"), None),
            model("model_hero", "Hero", Some("character"), None),
        ];

        let weapon = model_picker_for(&models, 1, "weapon", "");
        assert_eq!(paths(&weapon), ["model_sword"]);
        let projectile = model_picker_for(&models, 1, "projectile", "");
        assert_eq!(paths(&projectile), ["model_bullet"]);
    }

    #[test]
    fn animals_and_creatures_only_list_their_own_category() {
        let models = vec![
            model("model_wolf", "Wolf", Some("animal"), None),
            model("model_dragon", "Dragon", Some("creature"), None),
            model("model_hero", "Hero", Some("character"), None),
        ];

        let animal = model_picker_for(&models, 1, "animal", "");
        assert_eq!(paths(&animal), ["model_wolf"]);
        let creature = model_picker_for(&models, 1, "creature", "");
        assert_eq!(paths(&creature), ["model_dragon"]);
    }

    #[test]
    fn loading_models_are_listed_with_their_state() {
        let models = vec![
            model("model_hero", "Hero", Some("character"), None),
            model_with_state("model_loading", "Loading", Some("character"), "importing"),
            model_with_state("model_failed", "Failed", Some("character"), "failed"),
        ];

        let picker = model_picker_for(&models, 1, "character", "");
        // Siguen en la lista (el combo los pinta con spinner / deshabilitados).
        assert_eq!(
            paths(&picker),
            ["model_failed", "model_hero", "model_loading"]
        );
        let states: Vec<&str> = picker
            .choices
            .iter()
            .map(|choice| choice.state.as_str())
            .collect();
        assert_eq!(states, ["failed", "ready", "importing"]);
    }
}
