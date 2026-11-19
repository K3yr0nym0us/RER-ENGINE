//! Estado y datos de la ventana nativa de configuración de controles.
//!
//! Sustituye al `useControlBindings` del renderer. El catálogo de personajes y
//! sus bindings salen de `project_state.entity_meta` (fuente de verdad del editor)
//! y los cambios se envían al motor con `set_control_bindings`.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::editor::app::EditorApp;
use crate::editor::cli::Locale;

/// Dispositivo cuya configuración se está editando.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ControlDeviceMode {
    #[default]
    KeyboardMouse,
    Gamepad,
}

impl ControlDeviceMode {
    pub fn parse(mode: &str) -> Self {
        if mode == "gamepad" {
            Self::Gamepad
        } else {
            Self::KeyboardMouse
        }
    }

    pub fn json_key(self) -> &'static str {
        match self {
            Self::KeyboardMouse => "keyboard_mouse",
            Self::Gamepad => "gamepad",
        }
    }
}

/// Borrador del editor de script anidado (doble click sobre una tecla).
#[derive(Debug, Clone, Default)]
pub struct ScriptDraft {
    pub control_key: String,
    pub name: String,
    pub source: String,
}

/// Estado de la ventana nativa de controles.
#[derive(Debug, Clone, Default)]
pub struct ControlBindingsState {
    pub open: bool,
    pub mode: ControlDeviceMode,
    /// Personaje efectivo (el mismo criterio que el hook de React: jugador primero).
    pub character_id: Option<u64>,
    /// Editor de script abierto sobre la ventana de controles.
    pub editor: Option<ScriptDraft>,
}

/// Opción de personaje mostrada en la cabecera.
#[derive(Debug, Clone)]
pub struct CharacterOption {
    pub id: u64,
    pub label: String,
}

fn path_marker(path: &str) -> Option<&str> {
    let last = path.rsplit(['/', '\\']).next().unwrap_or(path);
    matches!(
        last,
        "[Player]" | "[EditorCamera]" | "[Sun]" | "[Ground]" | "[EditorBox]"
    )
    .then_some(last)
}

fn path_label(path: &str) -> String {
    if path_marker(path) == Some("[Player]") {
        return "Player principal".to_string();
    }
    let last = path.rsplit(['/', '\\']).next().unwrap_or(path);
    last.rsplit_once('.')
        .map(|(name, _)| name)
        .unwrap_or(last)
        .to_string()
}

/// Personajes configurables, con el jugador principal primero (igual que React).
pub fn characters(app: &EditorApp) -> Vec<CharacterOption> {
    let mut options = Vec::new();
    for (id, meta) in &app.project_state.entity_meta {
        let Ok(entity_id) = id.parse::<u64>() else {
            continue;
        };
        let path = meta.get("path").and_then(Value::as_str).unwrap_or("");
        if path_marker(path) == Some("[EditorCamera]") {
            continue;
        }
        let kind = meta.get("kind").and_then(Value::as_str).unwrap_or("");
        let is_player = path_marker(path) == Some("[Player]");
        if kind != "character" && !is_player {
            continue;
        }
        let label = meta
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| path_label(path));
        options.push((
            u8::from(!is_player),
            CharacterOption {
                id: entity_id,
                label,
            },
        ));
    }
    options.sort_by_key(|(priority, option)| (*priority, option.id));
    options.into_iter().map(|(_, option)| option).collect()
}

/// Asegura que haya un personaje válido seleccionado. El `project_state` puede
/// llegar después de abrir la ventana, así que se reintenta en cada frame.
pub fn ensure_character(app: &mut EditorApp) {
    let options = characters(app);
    if app
        .control_bindings
        .character_id
        .is_none_or(|id| !options.iter().any(|option| option.id == id))
    {
        app.control_bindings.character_id = options.first().map(|option| option.id);
    }
}

/// Etiqueta del personaje efectivo.
pub fn character_label(app: &EditorApp) -> String {
    let options = characters(app);
    options
        .iter()
        .find(|option| Some(option.id) == app.control_bindings.character_id)
        .or_else(|| options.first())
        .map(|option| option.label.clone())
        .unwrap_or_else(|| "Sin personaje".to_string())
}

/// Bindings del modo indicado para el personaje efectivo: tecla → (nombre, código).
pub fn bindings(app: &EditorApp, mode: ControlDeviceMode) -> BTreeMap<String, (String, String)> {
    let mut result = BTreeMap::new();
    let Some(id) = app.control_bindings.character_id else {
        return result;
    };
    let Some(meta) = app.project_state.entity_meta_for(id) else {
        return result;
    };
    let Some(store) = meta.get("controlBindings").or_else(|| meta.get("controls")) else {
        return result;
    };
    let Some(map) = store.get(mode.json_key()).and_then(Value::as_object) else {
        return result;
    };
    for (key, script) in map {
        result.insert(
            key.clone(),
            (
                script
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                script
                    .get("source")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ),
        );
    }
    result
}

/// Persiste un binding y lo envía al motor sin pasar por React.
pub fn set_binding(
    app: &mut EditorApp,
    mode: ControlDeviceMode,
    key: &str,
    name: &str,
    source: &str,
) {
    let Some(id) = app.control_bindings.character_id else {
        return;
    };
    let id_key = id.to_string();
    let Some(meta) = app.project_state.entity_meta.get_mut(&id_key) else {
        return;
    };

    let store_key = if meta.get("controlBindings").is_some() {
        "controlBindings"
    } else {
        "controls"
    };
    let store = meta
        .as_object_mut()
        .expect("meta de entidad es objeto")
        .entry(store_key.to_string())
        .or_insert_with(|| json!({ "keyboard_mouse": {}, "gamepad": {} }));
    if !store.is_object() {
        *store = json!({ "keyboard_mouse": {}, "gamepad": {} });
    }
    let map = store
        .as_object_mut()
        .expect("store de bindings es objeto")
        .entry(mode.json_key())
        .or_insert_with(|| json!({}));
    if !map.is_object() {
        *map = json!({});
    }
    map.as_object_mut()
        .expect("mapa de bindings es objeto")
        .insert(key.to_string(), json!({ "name": name, "source": source }));

    let payload = store.clone();
    app.send_engine(json!({ "cmd": "set_control_bindings", "id": id, "bindings": payload }));
    app.push_log(
        format!(
            "[Controles] Script «{name}» asignado a {key} ({})",
            mode.json_key()
        ),
        false,
    );
}

/// Plantillo inicial, espejo de `getDefaultControlScript` del renderer.
pub fn default_script(locale: Locale, control_key: &str, is_3d: bool) -> String {
    if control_key == "MOUSE_LEFT" {
        return match (locale, is_3d) {
            (Locale::Es, true) => MOUSE_LEFT_3D_ES.to_string(),
            (Locale::Es, false) => MOUSE_LEFT_2D_ES.to_string(),
            (Locale::En, true) => MOUSE_LEFT_3D_EN.to_string(),
            (Locale::En, false) => MOUSE_LEFT_2D_EN.to_string(),
        };
    }
    match locale {
        Locale::Es => CONTROL_SCRIPT_ES.to_string(),
        Locale::En => CONTROL_SCRIPT_EN.to_string(),
    }
}

const CONTROL_SCRIPT_EN: &str = "\
// Control script — runs each frame while the key is held\n\
// 3D play character: customize walk speed, sprint, jump. The engine applies the bound key.\n\
// 2D: use fn on_keep(entity, control_key) { engine.move_control(entity.id, 7.0); }\n\
// ─────────────────────────────────────────────────────────────────────────────\n\
\n\
engine.fp_set_walk_speed(4.0);\n";

const CONTROL_SCRIPT_ES: &str = "\
// Script de control — se ejecuta cada frame mientras la tecla está pulsada\n\
// 3D play character: personaliza velocidad, sprint, salto. El motor aplica la tecla del binding.\n\
// 2D: fn on_keep(entity, control_key) { engine.move_control(entity.id, 7.0); }\n\
// ─────────────────────────────────────────────────────────────────────────────\n\
\n\
engine.fp_set_walk_speed(4.0);\n";

const MOUSE_LEFT_2D_EN: &str = "\
// MOUSE LEFT — fire projectile toward the mouse pointer (play mode only).\n\
// Use the numeric entity id (Properties → Copy entity ID), not the display name.\n\
//\n\
// fn on_press(entity, control_key) {\n\
//     if engine.is_play_mode() {\n\
//         let template_id = 42;\n\
//         let mx = engine.mouse_world_x();\n\
//         let my = engine.mouse_world_y();\n\
//         let dx = mx - entity.x;\n\
//         let dy = my - entity.y;\n\
//         let len_sq = dx * dx + dy * dy;\n\
//         if len_sq > 0.000001 {\n\
//             let len = len_sq.sqrt();\n\
//             engine.fire_projectile(template_id, entity.id, dx / len, dy / len, 0.0);\n\
//         }\n\
//     }\n\
// }\n";

const MOUSE_LEFT_2D_ES: &str = "\
// MOUSE LEFT — disparar proyectil hacia el puntero del ratón (solo en modo play).\n\
// Usa el id numérico de la entidad (Propiedades → Copiar ID de la entidad), no el nombre visible.\n\
//\n\
// fn on_press(entity, control_key) {\n\
//     if engine.is_play_mode() {\n\
//         let template_id = 42;\n\
//         let mx = engine.mouse_world_x();\n\
//         let my = engine.mouse_world_y();\n\
//         let dx = mx - entity.x;\n\
//         let dy = my - entity.y;\n\
//         let len_sq = dx * dx + dy * dy;\n\
//         if len_sq > 0.000001 {\n\
//             let len = len_sq.sqrt();\n\
//             engine.fire_projectile(template_id, entity.id, dx / len, dy / len, 0.0);\n\
//         }\n\
//     }\n\
// }\n";

const MOUSE_LEFT_3D_EN: &str = "\
// MOUSE LEFT — fire projectile toward the screen crosshair (FPS play mode).\n\
// The cursor is hidden in first-person; aim uses the camera forward vector.\n\
// Use the numeric entity id (Properties → Copy entity ID), not the display name.\n\
//\n\
// fn on_press(entity, control_key) {\n\
//     if engine.is_play_mode() {\n\
//         let template_id = 42;\n\
//         engine.fire_projectile(template_id, entity.id, engine.play_aim_dir_x(), engine.play_aim_dir_y(), engine.play_aim_dir_z());\n\
//     }\n\
// }\n";

const MOUSE_LEFT_3D_ES: &str = "\
// MOUSE LEFT — disparar proyectil hacia la cruceta central (modo play FPS).\n\
// El cursor está oculto en primera persona; la dirección sale de la cámara.\n\
// Usa el id numérico de la entidad (Propiedades → Copiar ID de la entidad), no el nombre visible.\n\
//\n\
// fn on_press(entity, control_key) {\n\
//     if engine.is_play_mode() {\n\
//         let template_id = 42;\n\
//         engine.fire_projectile(template_id, entity.id, engine.play_aim_dir_x(), engine.play_aim_dir_y(), engine.play_aim_dir_z());\n\
//     }\n\
// }\n";
