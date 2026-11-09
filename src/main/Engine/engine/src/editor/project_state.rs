//! Estado de proyecto que el editor necesita para guardar.
//!
//! Mientras la UI React sigue siendo la fuente de edición, el editor pide un
//! snapshot del proyecto justo antes de guardar. Así el ensamblado del
//! `manifest.json` y el empaquetado `.save` viven en Rust y no dependen del
//! empaquetado de Electron.

use std::collections::BTreeMap;

use serde_json::Value;

/// Datos del proyecto que no viven en el motor (escenas inactivas, blueprints,
/// metadatos de entidad, pantallas UI e idioma).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProjectState {
    pub project_type: Option<String>,
    pub game_style: Option<String>,
    pub locale: Option<String>,
    pub scenes: Vec<Value>,
    pub active_scene_id: Option<u64>,
    pub blueprints: Vec<Value>,
    pub entity_meta: BTreeMap<String, Value>,
    pub sounds: Vec<Value>,
    pub fonts: Vec<Value>,
    pub hud_images: Vec<Value>,
    pub models: Vec<Value>,
    pub backgrounds: Vec<Value>,
    pub player_ui_screens: Vec<Value>,
    pub menu_ui_screens: Vec<Value>,
}

impl ProjectState {
    /// Construye el estado a partir del JSON que responde la UI React.
    pub fn from_value(value: &Value) -> Self {
        Self {
            project_type: string_field(value, "projectType"),
            game_style: string_field(value, "gameStyle"),
            locale: string_field(value, "locale"),
            scenes: array_field(value, "scenes"),
            active_scene_id: value.get("activeSceneId").and_then(Value::as_u64),
            blueprints: array_field(value, "blueprints"),
            entity_meta: entity_meta_field(value),
            sounds: array_field(value, "sounds"),
            fonts: array_field(value, "fonts"),
            hud_images: array_field(value, "hudImages"),
            models: array_field(value, "models"),
            backgrounds: array_field(value, "backgrounds"),
            player_ui_screens: array_field(value, "playerUiScreens"),
            menu_ui_screens: array_field(value, "menuUiScreens"),
        }
    }

    /// Metadatos de editor de una entidad (`entityMeta[id]`).
    pub fn entity_meta_for(&self, entity_id: u64) -> Option<&Value> {
        self.entity_meta.get(&entity_id.to_string())
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn array_field(value: &Value, key: &str) -> Vec<Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn entity_meta_field(value: &Value) -> BTreeMap<String, Value> {
    value
        .get("entityMeta")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(key, meta)| (key.clone(), meta.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Estado mínimo válido para poder guardar.
pub fn is_saveable(state: &ProjectState) -> bool {
    !state.scenes.is_empty() || !state.entity_meta.is_empty()
}

// ---------------------------------------------------------------------------
// Timestamp ISO-8601 (UTC) sin dependencias externas.
// ---------------------------------------------------------------------------

pub fn now_iso8601() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    iso8601_from_unix(seconds)
}

fn iso8601_from_unix(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rem = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

/// Algoritmo `civil_from_days` de Howard Hinnant (días desde 1970-01-01).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_project_state() {
        let value = json!({
            "projectType": "3D",
            "gameStyle": "third-person",
            "locale": "es",
            "activeSceneId": 2,
            "scenes": [{ "id": 1 }, { "id": 2 }],
            "blueprints": [{ "id": "bp_1" }],
            "entityMeta": { "7": { "kind": "character" } },
            "sounds": [{ "name": "a", "path": "C:/a.wav" }],
            "playerUiScreens": [{ "id": "hud_main", "name": "HUD" }]
        });

        let state = ProjectState::from_value(&value);
        assert_eq!(state.project_type.as_deref(), Some("3D"));
        assert_eq!(state.game_style.as_deref(), Some("third-person"));
        assert_eq!(state.locale.as_deref(), Some("es"));
        assert_eq!(state.active_scene_id, Some(2));
        assert_eq!(state.scenes.len(), 2);
        assert_eq!(state.blueprints.len(), 1);
        assert_eq!(state.sounds.len(), 1);
        assert_eq!(state.player_ui_screens.len(), 1);
        assert_eq!(
            state.entity_meta_for(7).and_then(|m| m.get("kind")),
            Some(&json!("character"))
        );
    }

    #[test]
    fn blank_strings_are_none() {
        let state = ProjectState::from_value(&json!({
            "projectType": "  ",
            "locale": ""
        }));
        assert_eq!(state.project_type, None);
        assert_eq!(state.locale, None);
    }

    #[test]
    fn iso_timestamp_matches_known_dates() {
        assert_eq!(iso8601_from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601_from_unix(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(iso8601_from_unix(951_782_400), "2000-02-29T00:00:00Z");
    }
}
