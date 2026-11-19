//! Estado del editor de HUD (pantallas Player UI) en ventana nativa.
//!
//! Sustituye al `PlayerUiEditorElectronHost`, sus sesiones IPC y su panel React.
//! Los elementos se leen del evento `player_ui_text_boxes_list` del motor y
//! todas las acciones se envían como comandos del motor desde Rust.

use serde_json::{Value, json};

use crate::editor::app::EditorApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HudElementKind {
    Text,
    Button,
    Image,
    Object,
}

impl HudElementKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Button => "button",
            Self::Image => "image",
            Self::Object => "object",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HudElement {
    pub kind: HudElementKind,
    pub id: u32,
    pub label: String,
    pub z_index: i32,
    pub locked: bool,
    pub fill_color: Option<[f32; 4]>,
    #[allow(dead_code)]
    pub texture_path: Option<String>,
}

/// Submodal abierto sobre el editor de HUD.
#[derive(Debug, Clone)]
pub enum HudNested {
    SelectFont,
    AddImage,
    AssignTexture(u32),
    ConfirmDelete(HudElementKind, u32, String),
}

/// Estado de la ventana nativa del editor de HUD.
#[derive(Debug, Clone, Default)]
pub struct HudEditorState {
    pub open: bool,
    pub screen_id: String,
    pub name_draft: String,
    pub editing_name: bool,
    pub nested: Option<HudNested>,
    pub font_draft: String,
    pub image_draft: String,
    pub open_group: usize,
    pub object_draw_active: bool,
}

/// Lista de elementos del motor para la pantalla que se está editando.
#[derive(Debug, Clone, Default)]
pub struct HudElements {
    pub scope: String,
    pub screen_id: String,
    pub items: Vec<HudElement>,
}

fn number(value: Option<&Value>) -> u32 {
    value.and_then(Value::as_u64).unwrap_or(0) as u32
}

fn integer(value: Option<&Value>) -> i32 {
    value.and_then(Value::as_i64).unwrap_or(0) as i32
}

fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn bool_value(value: Option<&Value>) -> bool {
    value.and_then(Value::as_bool).unwrap_or(false)
}

fn color(value: Option<&Value>) -> Option<[f32; 4]> {
    let values = value?.as_array()?;
    let floats: Vec<f32> = values
        .iter()
        .filter_map(Value::as_f64)
        .map(|value| value as f32)
        .collect();
    <[f32; 4]>::try_from(floats).ok()
}

impl HudElements {
    /// Parsea el evento `player_ui_text_boxes_list` del motor.
    pub fn from_event(event: &Value) -> Self {
        let list = |key: &str| {
            event
                .get(key)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        let mut items = Vec::new();

        for entry in list("boxes") {
            items.push(HudElement {
                kind: HudElementKind::Text,
                id: number(entry.get("id")),
                label: text(entry.get("text")),
                z_index: integer(entry.get("z_index")),
                locked: bool_value(entry.get("locked")),
                fill_color: None,
                texture_path: None,
            });
        }
        for entry in list("buttons") {
            items.push(HudElement {
                kind: HudElementKind::Button,
                id: number(entry.get("id")),
                label: text(entry.get("text")),
                z_index: integer(entry.get("z_index")),
                locked: bool_value(entry.get("locked")),
                fill_color: None,
                texture_path: None,
            });
        }
        for entry in list("images") {
            items.push(HudElement {
                kind: HudElementKind::Image,
                id: number(entry.get("id")),
                label: text(entry.get("image_name")),
                z_index: integer(entry.get("z_index")),
                locked: bool_value(entry.get("locked")),
                fill_color: None,
                texture_path: None,
            });
        }
        for entry in list("objects") {
            let id = number(entry.get("id"));
            items.push(HudElement {
                kind: HudElementKind::Object,
                id,
                label: format!(
                    "{} #{}",
                    if text(entry.get("texture_name")).is_empty() {
                        "Object".to_string()
                    } else {
                        text(entry.get("texture_name"))
                    },
                    id
                ),
                z_index: integer(entry.get("z_index")),
                locked: bool_value(entry.get("locked")),
                fill_color: color(entry.get("fill_color")),
                texture_path: entry
                    .get("texture_path")
                    .and_then(Value::as_str)
                    .filter(|path| !path.is_empty())
                    .map(str::to_string),
            });
        }

        Self {
            scope: text(event.get("scope")),
            screen_id: text(event.get("screen_id")),
            items,
        }
    }
}

/// Entra en modo edición de la pantalla (avisa al motor).
pub fn begin_edit(app: &mut EditorApp, screen_id: &str) {
    app.hud_editor.open = true;
    app.hud_editor.screen_id = screen_id.to_string();
    app.hud_editor.name_draft = app
        .ui_screens
        .player
        .iter()
        .find(|screen| screen.id == screen_id)
        .map(|screen| screen.name.clone())
        .unwrap_or_default();
    app.hud_editor.editing_name = false;
    app.hud_editor.nested = None;
    app.hud_editor.open_group = 0;
    app.send_engine(json!({
        "cmd": "set_player_ui_edit_mode",
        "active": true,
        "scope": "player",
        "screen_id": screen_id,
    }));
}

/// Sale del modo edición (X de la ventana o Guardar).
pub fn end_edit(app: &mut EditorApp) {
    app.hud_editor.open = false;
    app.hud_editor.nested = None;
    app.hud_editor.object_draw_active = false;
    app.send_engine(json!({
        "cmd": "set_player_ui_edit_mode",
        "active": false,
    }));
    app.send_engine(json!({ "cmd": "set_player_ui_object_draw", "active": false }));
}

pub fn rename(app: &mut EditorApp, name: &str) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Some(screen) = app
        .ui_screens
        .player
        .iter_mut()
        .find(|screen| screen.id == app.hud_editor.screen_id)
    {
        screen.name = trimmed.to_string();
    }
    app.hud_editor.name_draft = trimmed.to_string();
    app.sync_player_ui_screens();
    app.push_log(format!("[HUD] Pantalla renombrada a «{trimmed}»"), false);
}

pub fn toggle_draw(app: &mut EditorApp, active: bool) {
    app.hud_editor.object_draw_active = active;
    app.send_engine(json!({ "cmd": "set_player_ui_object_draw", "active": active }));
}

pub fn set_props(app: &mut EditorApp, element: &super::hud_editor::HudElement, props: Value) {
    let mut command = json!({
        "cmd": "set_player_ui_hud_element_props",
        "element_kind": element.kind.as_str(),
        "id": element.id,
    });
    if let Some(object) = command.as_object_mut() {
        for (key, value) in props.as_object().expect("props es objeto") {
            object.insert(key.clone(), value.clone());
        }
    }
    app.send_engine(command);
}

pub fn set_style(app: &mut EditorApp, id: u32, style: Value) {
    let mut command = json!({
        "cmd": "set_player_ui_object_style",
        "id": id,
        "fill_color": style.get("fill_color").cloned().unwrap_or(Value::Null),
        "clear_texture": style.get("clear_texture").and_then(Value::as_bool).unwrap_or(false),
        "live": style.get("live").and_then(Value::as_bool).unwrap_or(false),
        "skip_undo": style.get("skip_undo").and_then(Value::as_bool).unwrap_or(false),
    });
    if let Some(texture) = style.get("texture_path").and_then(Value::as_str) {
        command["texture_path"] = json!(texture);
    }
    app.send_engine(command);
}

pub fn add_from_asset(app: &mut EditorApp, is_font: bool, path: &str) {
    if path.is_empty() {
        return;
    }
    let command = if is_font {
        json!({ "cmd": "add_player_ui_text_box", "font_path": path })
    } else {
        json!({ "cmd": "add_player_ui_image", "image_path": path })
    };
    app.send_engine(command);
    app.hud_editor.nested = None;
}
