//! Estado del editor de HUD (pantallas Player UI) en ventana nativa.
//!
//! Sustituye al `PlayerUiEditorElectronHost`, sus sesiones IPC y su panel React.
//! Los elementos se leen del evento `player_ui_text_boxes_list` del motor y
//! todas las acciones se envían como comandos del motor desde Rust.

use rer_engine_core::i18n::TextValue;
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
    pub text_value: Option<TextValue>,
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

fn text_value(value: Option<&Value>) -> Option<TextValue> {
    serde_json::from_value(value?.clone()).ok()
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
            let text_value = text_value(entry.get("text"));
            items.push(HudElement {
                kind: HudElementKind::Text,
                id: number(entry.get("id")),
                label: text_value
                    .as_ref()
                    .map(|text| text.source_text().to_string())
                    .unwrap_or_default(),
                text_value,
                z_index: integer(entry.get("z_index")),
                locked: bool_value(entry.get("locked")),
                fill_color: None,
                texture_path: None,
            });
        }
        for entry in list("buttons") {
            let text_value = text_value(entry.get("text"));
            items.push(HudElement {
                kind: HudElementKind::Button,
                id: number(entry.get("id")),
                label: text_value
                    .as_ref()
                    .map(|text| text.source_text().to_string())
                    .unwrap_or_default(),
                text_value,
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
                text_value: None,
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
                text_value: None,
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

/// Pantalla usada por el selector existente de «Añadir texto».
///
/// Mantiene el HUD que ya está en edición; si no hay ninguno, prefiere la
/// pantalla de jugador activa y después la primera disponible.
pub fn new_text_screen(app: &EditorApp) -> Option<&str> {
    if app.hud_editor.open && !app.hud_editor.screen_id.is_empty() {
        return Some(&app.hud_editor.screen_id);
    }
    app.ui_screens
        .player
        .iter()
        .find(|screen| screen.active)
        .or_else(|| app.ui_screens.player.first())
        .map(|screen| screen.id.as_str())
}

/// Abre el selector nativo existente con la fuente del proyecto preseleccionada.
///
/// No crea el texto ni importa la fuente: el botón «Añadir» del selector sigue
/// siendo el encargado de enviar el comando al motor.
pub fn open_add_text_with_font(app: &mut EditorApp, font_path: &str) -> bool {
    if font_path.is_empty() {
        return false;
    }
    let Some(screen_id) = new_text_screen(app).map(str::to_owned) else {
        return false;
    };
    if !app.hud_editor.open || app.hud_editor.screen_id != screen_id {
        begin_edit(app, &screen_id);
    }
    app.hud_editor.font_draft = font_path.to_owned();
    app.hud_editor.nested = Some(HudNested::SelectFont);
    true
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

fn hud_element_props_command(element: &HudElement, props: Value) -> Value {
    let mut command = json!({
        "cmd": "set_player_ui_hud_element_props",
        "element_kind": element.kind.as_str(),
        "id": element.id,
    });
    if let Some(object) = command.as_object_mut() {
        for (key, value) in props.as_object().expect("props es objeto") {
            let mut value = value.clone();
            if key == "text"
                && let Some(source) = value.as_str()
            {
                value = match element
                    .text_value
                    .as_ref()
                    .filter(|text| text.is_localized())
                {
                    Some(localized) => json!(localized),
                    None => json!(TextValue::literal(source)),
                };
            }
            object.insert(key.clone(), value);
        }
    }
    command
}

pub fn set_props(app: &mut EditorApp, element: &super::hud_editor::HudElement, props: Value) {
    app.send_engine(hud_element_props_command(element, props));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::ui_screens::UiScreenEntry;

    #[test]
    fn font_selector_preselects_asset_in_current_hud() {
        let mut app = EditorApp::new(crate::editor::cli::Cli::default());
        app.hud_editor.open = true;
        app.hud_editor.screen_id = "hud-current".to_owned();
        app.hud_editor.font_draft = "old.ttf".to_owned();

        assert!(open_add_text_with_font(&mut app, "fonts/project.ttf"));

        assert_eq!(app.hud_editor.screen_id, "hud-current");
        assert_eq!(app.hud_editor.font_draft, "fonts/project.ttf");
        assert!(
            app.hud_editor
                .nested
                .as_ref()
                .is_some_and(|nested| matches!(nested, HudNested::SelectFont))
        );
        assert!(app.pending_engine.borrow().is_empty());
    }

    #[test]
    fn font_selector_opens_active_hud_when_editor_is_closed() {
        let mut app = EditorApp::new(crate::editor::cli::Cli::default());
        app.ui_screens.player = vec![
            UiScreenEntry {
                id: "hud-first".to_owned(),
                name: "First".to_owned(),
                active: false,
            },
            UiScreenEntry {
                id: "hud-active".to_owned(),
                name: "Active HUD".to_owned(),
                active: true,
            },
        ];

        assert!(open_add_text_with_font(&mut app, "fonts/project.ttf"));

        assert!(app.hud_editor.open);
        assert_eq!(app.hud_editor.screen_id, "hud-active");
        assert_eq!(app.hud_editor.name_draft, "Active HUD");
        assert_eq!(app.hud_editor.font_draft, "fonts/project.ttf");
        assert!(
            app.hud_editor
                .nested
                .as_ref()
                .is_some_and(|nested| matches!(nested, HudNested::SelectFont))
        );
        let pending = app.pending_engine.borrow();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0]["cmd"], "set_player_ui_edit_mode");
        assert_eq!(pending[0]["screen_id"], "hud-active");
    }

    #[test]
    fn font_selector_requires_a_hud_and_a_nonempty_path() {
        let mut app = EditorApp::new(crate::editor::cli::Cli::default());
        app.hud_editor.font_draft = "keep.ttf".to_owned();

        assert!(!open_add_text_with_font(&mut app, "fonts/project.ttf"));
        assert!(!open_add_text_with_font(&mut app, ""));
        assert!(!app.hud_editor.open);
        assert_eq!(app.hud_editor.font_draft, "keep.ttf");
        assert!(app.hud_editor.nested.is_none());
        assert!(app.pending_engine.borrow().is_empty());
    }

    #[test]
    fn hud_elements_preserve_serialized_text_values() {
        let localized = json!({
            "kind": "localized",
            "id": {"namespace": "player_ui", "key": "title"},
            "source": "Título"
        });
        let literal = json!({"kind": "literal", "value": "Jugar"});
        let elements = HudElements::from_event(&json!({
            "scope": "player",
            "screen_id": "main",
            "boxes": [{
                "id": 1,
                "text": localized,
                "z_index": 2,
                "locked": false
            }],
            "buttons": [{
                "id": 2,
                "text": literal,
                "z_index": 3,
                "locked": true
            }]
        }));

        assert_eq!(elements.items.len(), 2);
        assert_eq!(elements.items[0].label, "Título");
        assert!(
            elements.items[0]
                .text_value
                .as_ref()
                .is_some_and(TextValue::is_localized)
        );
        assert_eq!(elements.items[1].label, "Jugar");
        assert_eq!(
            elements.items[1]
                .text_value
                .as_ref()
                .and_then(TextValue::as_literal),
            Some("Jugar")
        );
    }

    #[test]
    fn hud_props_preserve_localized_text_and_wrap_plain_text_for_literals() {
        let element = HudElement {
            kind: HudElementKind::Text,
            id: 7,
            label: "Título".into(),
            text_value: None,
            z_index: 0,
            locked: false,
            fill_color: None,
            texture_path: None,
        };
        let localized = json!({
            "kind": "localized",
            "id": {"namespace": "player_ui", "key": "title"},
            "source": "Título"
        });
        let command =
            hud_element_props_command(&element, json!({"text": localized, "locked": true}));
        assert_eq!(command["text"], localized);
        assert_eq!(command["locked"], true);

        let command = hud_element_props_command(&element, json!({"text": "Texto"}));
        assert_eq!(
            command["text"],
            json!({"kind": "literal", "value": "Texto"})
        );

        let localized_value: TextValue = serde_json::from_value(localized).unwrap();
        let mut localized_element = element.clone();
        localized_element.text_value = Some(localized_value.clone());
        let command = hud_element_props_command(
            &localized_element,
            json!({"text": "Texto plano", "locked": true}),
        );
        assert_eq!(command["text"], json!(localized_value));
        assert_eq!(command["locked"], true);

        let mut literal_element = element;
        literal_element.text_value = Some(TextValue::literal("Jugar"));
        let command = hud_element_props_command(&literal_element, json!({"text": "Texto"}));
        assert_eq!(
            command["text"],
            json!({"kind": "literal", "value": "Texto"})
        );
    }
}
