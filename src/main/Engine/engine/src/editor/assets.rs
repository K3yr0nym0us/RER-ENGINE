//! Listas de recursos del proyecto (modelos, sprites, sonidos, fuentes, etc.).
//!
//! El motor es la fuente de verdad: al arrancar/cargar proyecto se piden las
//! listas y se mantienen al día con los eventos `*_loaded` / `*_removed`.

use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetEntry {
    pub name: String,
    pub path: String,
    pub model_id: Option<String>,
    pub asset: Option<String>,
    pub category: Option<String>,
    /// Estado de carga (`importing` mientras el modelo aún no está listo).
    /// `None` para recursos que no exponen estado (sprites, sonidos, etc.).
    pub state: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssetLists {
    pub sprites: Vec<AssetEntry>,
    pub sounds: Vec<AssetEntry>,
    pub fonts: Vec<AssetEntry>,
    pub hud_images: Vec<AssetEntry>,
    pub backgrounds: Vec<AssetEntry>,
    pub models: Vec<AssetEntry>,
}

impl AssetLists {
    /// Comandos para pedir todas las listas del tipo de proyecto.
    pub fn request_commands(is_3d: bool) -> Vec<Value> {
        let mut commands = vec![
            json!({ "cmd": "get_sounds_list" }),
            json!({ "cmd": "get_fonts_list" }),
            json!({ "cmd": "get_hud_images_list" }),
        ];
        if is_3d {
            commands.push(json!({ "cmd": "get_models_list" }));
        } else {
            commands.push(json!({ "cmd": "get_sprites_list" }));
            commands.push(json!({ "cmd": "get_backgrounds_list" }));
        }
        commands
    }

    pub fn apply_event(&mut self, event: &Value) {
        let name = event.get("event").and_then(Value::as_str).unwrap_or("");
        match name {
            "sprites_list" => self.sprites = parse_list(event.get("sprites")),
            "sounds_list" => self.sounds = parse_list(event.get("sounds")),
            "fonts_list" => self.fonts = parse_list(event.get("fonts")),
            "hud_images_list" => self.hud_images = parse_list(event.get("images")),
            "backgrounds_list" => self.backgrounds = parse_list(event.get("backgrounds")),
            "models_list" => self.models = parse_list(event.get("models")),
            "sprite_loaded" => upsert(&mut self.sprites, entry_from_event(event)),
            "sound_loaded" => upsert(&mut self.sounds, entry_from_event(event)),
            "font_loaded" => upsert(&mut self.fonts, entry_from_event(event)),
            "hud_image_loaded" => upsert(&mut self.hud_images, entry_from_event(event)),
            "background_asset_loaded" => upsert(&mut self.backgrounds, entry_from_event(event)),
            "model_asset_importing" => {
                upsert_state(&mut self.models, entry_from_event(event), Some("importing"))
            }
            "model_asset_imported" => {
                upsert_state(&mut self.models, entry_from_event(event), Some("importing"))
            }
            "model_asset_preload_started" => {
                upsert_state(&mut self.models, entry_from_event(event), Some("importing"))
            }
            "model_asset_loaded" => {
                upsert_state(&mut self.models, entry_from_event(event), Some("ready"))
            }
            "model_asset_load_failed" => {
                upsert_state(&mut self.models, entry_from_event(event), Some("failed"))
            }
            "sprite_removed" => remove_path(&mut self.sprites, event),
            "sound_removed" => remove_path(&mut self.sounds, event),
            "font_removed" => remove_path(&mut self.fonts, event),
            "hud_image_removed" => remove_path(&mut self.hud_images, event),
            "background_asset_removed" => remove_path(&mut self.backgrounds, event),
            "model_asset_removed" => remove_path(&mut self.models, event),
            _ => {}
        }
    }

    pub fn assets(&self, kind: AssetKind) -> &[AssetEntry] {
        match kind {
            AssetKind::Model => &self.models,
            AssetKind::Sprite => &self.sprites,
            AssetKind::Sound => &self.sounds,
            AssetKind::Font => &self.fonts,
            AssetKind::HudImage => &self.hud_images,
            AssetKind::Background => &self.backgrounds,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Model,
    Sprite,
    Sound,
    Font,
    HudImage,
    Background,
}

impl AssetKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Model => "res_models",
            Self::Sprite => "res_sprites",
            Self::Sound => "res_sounds",
            Self::Font => "res_fonts",
            Self::HudImage => "res_images",
            Self::Background => "res_backgrounds",
        }
    }

    pub fn load_command(self) -> &'static str {
        match self {
            Self::Model => "load_model_asset",
            Self::Sprite => "load_sprite",
            Self::Sound => "load_sound",
            Self::Font => "load_font",
            Self::HudImage => "load_hud_image",
            Self::Background => "load_background_asset",
        }
    }

    /// Identificador estable para las modales legacy de Electron.
    pub fn legacy_id(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Sprite => "sprite",
            Self::Sound => "sound",
            Self::Font => "font",
            Self::HudImage => "hud_image",
            Self::Background => "background",
        }
    }

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Model => &["glb", "gltf", "fbx"],
            Self::Sprite | Self::HudImage | Self::Background => &["png", "jpg", "jpeg", "webp"],
            Self::Sound => &["wav", "ogg", "mp3", "flac", "aac", "m4a"],
            Self::Font => &["ttf", "otf"],
        }
    }
}

fn parse_list(value: Option<&Value>) -> Vec<AssetEntry> {
    value
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(entry_from_value).collect())
        .unwrap_or_default()
}

fn entry_from_event(event: &Value) -> AssetEntry {
    entry_from_value(event).unwrap_or(AssetEntry {
        name: String::new(),
        path: String::new(),
        model_id: None,
        asset: None,
        category: None,
        state: None,
    })
}

fn entry_from_value(value: &Value) -> Option<AssetEntry> {
    let path = value.get("path")?.as_str()?.to_string();
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(&path)
        .to_string();
    Some(AssetEntry {
        name,
        path,
        model_id: value
            .get("model_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        asset: value
            .get("asset")
            .and_then(Value::as_str)
            .map(str::to_string),
        category: value
            .get("category")
            .and_then(Value::as_str)
            .map(str::to_string),
        state: value
            .get("state")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn upsert(list: &mut Vec<AssetEntry>, entry: AssetEntry) {
    upsert_state(list, entry, None);
}

/// Inserta o actualiza un recurso. Los eventos de modelo alternan entre el
/// path fuente (`model_asset_importing/imported`) y el `model_id`
/// (`model_asset_loaded/preload_started`): se emparejan también por `model_id`
/// para no duplicar filas ni dejar el spinner encendido.
fn upsert_state(list: &mut Vec<AssetEntry>, entry: AssetEntry, state: Option<&str>) {
    if entry.path.is_empty() {
        return;
    }
    let existing = list.iter_mut().find(|item| {
        item.path == entry.path || (entry.model_id.is_some() && item.model_id == entry.model_id)
    });
    if let Some(existing) = existing {
        if !entry.name.is_empty() {
            existing.name = entry.name;
        }
        if entry.model_id.is_some() {
            existing.model_id = entry.model_id;
        }
        if entry.asset.is_some() {
            existing.asset = entry.asset;
        }
        if entry.category.is_some() {
            existing.category = entry.category;
        }
        if entry.state.is_some() {
            existing.state = entry.state;
        }
        if let Some(state) = state {
            existing.state = Some(state.to_string());
        }
    } else {
        let mut entry = entry;
        if let Some(state) = state {
            entry.state = Some(state.to_string());
        }
        list.push(entry);
    }
}

fn remove_path(list: &mut Vec<AssetEntry>, event: &Value) {
    let Some(path) = event.get("path").and_then(Value::as_str) else {
        return;
    };
    list.retain(|item| item.path != path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_model_loading_until_loaded() {
        let mut assets = AssetLists::default();
        assets.apply_event(&json!({
            "event": "model_asset_importing",
            "model_id": "model_hero",
            "name": "Hero",
            "path": "C:/models/hero.glb",
            "category": "character"
        }));
        assert_eq!(assets.models[0].state.as_deref(), Some("importing"));
        assert_eq!(assets.models[0].category.as_deref(), Some("character"));

        assets.apply_event(&json!({
            "event": "model_asset_imported",
            "model_id": "model_hero",
            "name": "Hero",
            "path": "C:/models/hero.glb",
            "asset": "imported/models/model_hero.rerasset",
            "category": "character"
        }));
        assert_eq!(assets.models[0].state.as_deref(), Some("importing"));

        // El evento de GPU llega con el `model_id` como path: no debe duplicar
        // la fila ni dejar el spinner encendido.
        assets.apply_event(&json!({
            "event": "model_asset_loaded",
            "path": "model_hero",
            "name": "Hero",
            "model_id": "model_hero"
        }));
        assert_eq!(assets.models.len(), 1);
        assert_eq!(assets.models[0].path, "C:/models/hero.glb");
        assert_eq!(assets.models[0].state.as_deref(), Some("ready"));
        assert_eq!(assets.models[0].category.as_deref(), Some("character"));
    }

    #[test]
    fn reads_sprite_list_and_updates() {
        let mut assets = AssetLists::default();
        assets.apply_event(&json!({
            "event": "sprites_list",
            "sprites": [
                { "name": "Hero", "path": "C:/sprites/hero.png" },
                { "name": "Enemy", "path": "C:/sprites/enemy.png" }
            ]
        }));
        assert_eq!(assets.sprites.len(), 2);

        assets.apply_event(&json!({
            "event": "sprite_loaded",
            "name": "Boss",
            "path": "C:/sprites/boss.png"
        }));
        assert_eq!(assets.sprites.len(), 3);

        assets.apply_event(&json!({
            "event": "sprite_removed",
            "path": "C:/sprites/enemy.png"
        }));
        assert_eq!(assets.sprites.len(), 2);
        assert!(!assets.sprites.iter().any(|item| item.name == "Enemy"));
    }

    #[test]
    fn upserts_models_by_path() {
        let mut assets = AssetLists::default();
        assets.apply_event(&json!({
            "event": "model_asset_importing",
            "model_id": "model_hero",
            "name": "Hero",
            "path": "C:/models/hero.glb"
        }));
        assets.apply_event(&json!({
            "event": "model_asset_imported",
            "model_id": "model_hero",
            "name": "Hero",
            "path": "C:/models/hero.glb",
            "asset": "imported/models/model_hero.rerasset"
        }));
        assert_eq!(assets.models.len(), 1);
        assert_eq!(
            assets.models[0].asset.as_deref(),
            Some("imported/models/model_hero.rerasset")
        );
    }

    #[test]
    fn request_commands_depend_on_project_type() {
        let two_d = AssetLists::request_commands(false);
        assert!(
            two_d
                .iter()
                .any(|cmd| cmd["cmd"] == json!("get_sprites_list"))
        );
        let three_d = AssetLists::request_commands(true);
        assert!(
            three_d
                .iter()
                .any(|cmd| cmd["cmd"] == json!("get_models_list"))
        );
        assert!(
            !three_d
                .iter()
                .any(|cmd| cmd["cmd"] == json!("get_sprites_list"))
        );
    }
}
