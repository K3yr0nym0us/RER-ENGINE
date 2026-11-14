//! Ensamblado del `manifest.json` de un `.save`.
//!
//! Es el port de `buildProjectSaveFromEngineSnapshot` (`src/renderer`): combina
//! el snapshot de la escena activa que exporta el motor con el estado de editor
//! que responde la UI React (escenas inactivas, blueprints, `entityMeta`, UI e
//! idioma).

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use crate::editor::project_state::{ProjectState, now_iso8601};

const ENTITY_MARKERS: [&str; 8] = [
    "[EditorBox]",
    "[Ground]",
    "[Player]",
    "[EditorCamera]",
    "[Sun]",
    "[Ball]",
    "[Colisionador]",
    "[ExecutionArea]",
];

const RERASSET_IMPORTER_VERSION: u32 = 2;

pub struct ManifestInput<'a> {
    pub engine_scene: &'a Value,
    pub state: &'a ProjectState,
    pub project_type: &'a str,
    pub game_style: &'a str,
}

/// Construye el manifiesto completo listo para empaquetar.
pub fn build_manifest(input: &ManifestInput<'_>) -> Value {
    let state = input.state;
    let engine_scene = input.engine_scene;
    let active_scene_id = state.active_scene_id.unwrap_or(1);
    let active_scene_name = state
        .scenes
        .iter()
        .find(|scene| scene_id(scene) == Some(active_scene_id))
        .and_then(|scene| scene.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("Scene-{active_scene_id:02}"));

    let active_scene =
        engine_scene_to_saved_scene(engine_scene, active_scene_id, &active_scene_name, state);

    let mut scenes = if state.scenes.is_empty() {
        vec![active_scene]
    } else {
        state
            .scenes
            .iter()
            .map(|tab| merge_scene(tab, &active_scene, active_scene_id))
            .collect()
    };

    let merged_models = merged_models(&state.models, engine_scene);

    for scene in &mut scenes {
        attach_model_ids_to_scene(scene, &merged_models);
    }

    let merged_fonts = merge_library_assets(&state.fonts, engine_scene.get("fonts"));
    let merged_hud_images = merge_library_assets(&state.hud_images, engine_scene.get("hud_images"));

    let mut blueprints = state.blueprints.clone();
    if input.project_type == "3D" {
        attach_model_ids(&mut blueprints, &merged_models);
        rewrite_imported_paths(&mut blueprints);
    }

    for scene in &mut scenes {
        if let Some(entities) = scene.get_mut("entities").and_then(Value::as_array_mut) {
            attach_model_ids(entities, &merged_models);
            rewrite_imported_paths(entities);
        }
        if let Some(player) = scene.get_mut("player")
            && player.is_object()
        {
            let mut one = vec![std::mem::take(player)];
            attach_model_ids(&mut one, &merged_models);
            rewrite_imported_paths(&mut one);
            *player = one.pop().unwrap_or(Value::Null);
        }
    }

    let is_3d = input.project_type == "3D";
    let resources_models = if is_3d {
        build_resources_models(engine_scene.get("models"), &merged_models)
    } else {
        Vec::new()
    };

    let mut root = Map::new();
    root.insert("version".into(), json!(1));
    root.insert("type".into(), json!(input.project_type));
    root.insert("gameStyle".into(), json!(input.game_style));
    root.insert("scenes".into(), Value::Array(scenes));
    root.insert("activeSceneId".into(), json!(active_scene_id));
    // Con `scenes[]` el mundo/entidades viven por escena (mismo criterio que React).
    root.insert("backgroundPath".into(), Value::Null);
    root.insert("entities".into(), json!([]));
    root.insert("player".into(), Value::Null);
    root.insert("config_camera".into(), Value::Null);
    root.insert("config_editor_camera".into(), Value::Null);
    root.insert("camera2d".into(), Value::Null);
    root.insert("savedAt".into(), json!(now_iso8601()));
    root.insert("sprites".into(), json!([]));
    if is_3d {
        root.insert(
            "resources".into(),
            json!({ "models": Value::Array(resources_models) }),
        );
    }
    root.insert("sounds".into(), Value::Array(state.sounds.clone()));
    root.insert("fonts".into(), Value::Array(merged_fonts));
    if !merged_hud_images.is_empty() {
        root.insert("hudImages".into(), Value::Array(merged_hud_images));
    }
    root.insert(
        "backgrounds".into(),
        Value::Array(state.backgrounds.clone()),
    );
    root.insert("blueprints".into(), Value::Array(blueprints));
    // Sección del plugin worldgen (persistencia F8): se pasa tal cual del
    // snapshot del engine; sin mundo generado no aparece.
    if let Some(world_gen_3d) = engine_scene.get("world_gen_3d") {
        root.insert("worldGen3d".into(), world_gen_3d.clone());
    }
    root.insert(
        "language".into(),
        json!(state.locale.as_deref().unwrap_or("en")),
    );
    root.insert(
        "playerUiScreens".into(),
        Value::Array(state.player_ui_screens.clone()),
    );
    root.insert(
        "menuUiScreens".into(),
        Value::Array(state.menu_ui_screens.clone()),
    );

    copy_engine_list(
        &mut root,
        engine_scene,
        "player_ui_text_boxes",
        "playerUiTextBoxes",
    );
    copy_engine_list(
        &mut root,
        engine_scene,
        "player_ui_buttons",
        "playerUiButtons",
    );
    copy_engine_list(
        &mut root,
        engine_scene,
        "player_ui_images",
        "playerUiImages",
    );
    copy_engine_list(
        &mut root,
        engine_scene,
        "player_ui_objects",
        "playerUiObjects",
    );

    Value::Object(root)
}

fn copy_engine_list(
    root: &mut Map<String, Value>,
    engine_scene: &Value,
    engine_key: &str,
    manifest_key: &str,
) {
    if let Some(list) = engine_scene.get(engine_key).and_then(Value::as_array)
        && !list.is_empty()
    {
        root.insert(manifest_key.into(), Value::Array(list.clone()));
    }
}

fn scene_id(scene: &Value) -> Option<u64> {
    scene.get("id").and_then(Value::as_u64)
}

/// Sustituye la escena activa por el snapshot del motor conservando id/nombre y
/// dejando `models` vacío (los modelos viven en `resources.models`).
fn merge_scene(tab: &Value, active_scene: &Value, active_scene_id: u64) -> Value {
    if scene_id(tab) == Some(active_scene_id) {
        let mut merged = tab.clone();
        if let (Some(target), Some(source)) = (merged.as_object_mut(), active_scene.as_object()) {
            for (key, value) in source {
                target.insert(key.clone(), value.clone());
            }
        }
        if let Some(obj) = merged.as_object_mut() {
            obj.insert("id".into(), json!(scene_id(tab).unwrap_or(active_scene_id)));
            if let Some(name) = tab.get("name") {
                obj.insert("name".into(), name.clone());
            }
            obj.insert("models".into(), json!([]));
        }
        merged
    } else {
        let mut clone = tab.clone();
        if let Some(obj) = clone.as_object_mut() {
            obj.insert("models".into(), json!([]));
        }
        clone
    }
}

fn engine_scene_to_saved_scene(scene: &Value, id: u64, name: &str, state: &ProjectState) -> Value {
    let entities: Vec<Value> = scene
        .get("entities")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|raw| map_engine_entity(raw, state))
                .collect()
        })
        .unwrap_or_default();

    let player = scene
        .get("player")
        .filter(|value| value.is_object())
        .map(normalize_engine_entity)
        .unwrap_or(Value::Null);

    let camera2d = scene
        .get("camera2d")
        .filter(|value| value.is_object())
        .map(|camera| {
            json!({
                "x": camera.get("x").cloned().unwrap_or(json!(0.0)),
                "y": camera.get("y").cloned().unwrap_or(json!(0.0)),
                "halfH": camera.get("half_h").cloned().unwrap_or(json!(0.0)),
            })
        })
        .unwrap_or(Value::Null);

    json!({
        "id": id,
        "name": name,
        "world": build_world(scene.get("world")),
        "backgroundPath": scene.get("background_path").cloned().unwrap_or(Value::Null),
        "entities": entities,
        "player": player,
        "config_camera": scene.get("config_camera").cloned().unwrap_or(Value::Null),
        "config_editor_camera": scene.get("config_editor_camera").cloned().unwrap_or(Value::Null),
        "camera2d": camera2d,
        "sprites": scene.get("sprites").cloned().unwrap_or(json!([])),
        "models": [],
    })
}

fn build_world(world: Option<&Value>) -> Value {
    let Some(world) = world else {
        return json!({});
    };
    let mut out = Map::new();
    let fields = [
        ("worldWidth", "world_width"),
        ("worldHeight", "world_height"),
        ("worldDepth", "world_depth"),
        ("worldRadius", "world_radius"),
        ("gridVisible", "grid_visible"),
        ("gridCellSize", "grid_cell_size"),
        ("gravity", "gravity"),
        ("targetFps", "target_fps"),
        ("lightAmbient", "light_ambient"),
        ("lightIntensity", "light_intensity"),
        ("shadowDarkness", "shadow_darkness"),
        ("graphicsTextureTier", "graphics_texture_tier"),
        ("textureDetailDistance", "texture_detail_distance_m"),
        ("reflectionTier", "reflection_tier"),
        ("reflectionRaytracing", "reflection_raytracing"),
        ("reflectionProbes", "reflection_probes"),
        ("shadowTier", "shadow_tier"),
        ("msaaTier", "msaa_tier"),
        ("taaEnabled", "taa_enabled"),
        ("taaBlend", "taa_blend"),
        ("taaJitterScale", "taa_jitter_scale"),
        ("lodEnabled", "model_lod_enabled"),
        ("lod1Distance", "model_lod1_distance_m"),
        ("lod2Distance", "model_lod2_distance_m"),
    ];
    for (target, source) in fields {
        if let Some(value) = world.get(source).filter(|value| !value.is_null()) {
            out.insert(target.into(), value.clone());
        }
    }
    Value::Object(out)
}

fn map_engine_entity(raw: &Value, state: &ProjectState) -> Value {
    let mut entity = normalize_engine_entity(raw);
    let Some(id) = entity.get("id").and_then(Value::as_u64) else {
        return entity;
    };
    let Some(meta) = state.entity_meta_for(id) else {
        return entity;
    };

    let engine_animations = entity
        .get("animations")
        .and_then(Value::as_array)
        .cloned()
        .filter(|list| !list.is_empty());
    let meta_animations = meta
        .get("animations")
        .and_then(Value::as_array)
        .cloned()
        .filter(|list| !list.is_empty());
    let animations = merge_animation_editor_meta(engine_animations, meta_animations);

    let Some(obj) = entity.as_object_mut() else {
        return entity;
    };
    if let Some(animations) = animations {
        obj.insert("animations".into(), Value::Array(animations));
    }
    if let Some(scripts) = meta
        .get("scripts")
        .and_then(Value::as_array)
        .filter(|list| !list.is_empty())
    {
        obj.insert("scripts".into(), Value::Array(scripts.clone()));
    }
    if let Some(graph) = meta.get("visualGraph").filter(|value| !value.is_null()) {
        obj.insert("visualGraph".into(), graph.clone());
    }
    if let Some(source) = meta
        .get("visualScriptRhai")
        .filter(|value| !value.is_null())
    {
        obj.insert("visualScriptRhai".into(), source.clone());
    }
    let blueprint_id = obj
        .get("blueprint_id")
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| {
            meta.get("blueprintId")
                .filter(|value| !value.is_null())
                .cloned()
        });
    if let Some(blueprint_id) = blueprint_id {
        obj.insert("blueprint_id".into(), blueprint_id);
    }

    entity
}

fn merge_animation_editor_meta(
    engine_animations: Option<Vec<Value>>,
    meta_animations: Option<Vec<Value>>,
) -> Option<Vec<Value>> {
    let Some(meta_animations) = meta_animations else {
        return engine_animations;
    };
    let Some(mut engine_animations) = engine_animations else {
        return Some(meta_animations);
    };

    for anim in &mut engine_animations {
        let name = anim.get("name").and_then(Value::as_str);
        let Some(from_meta) = name.and_then(|name| {
            meta_animations
                .iter()
                .find(|meta| meta.get("name").and_then(Value::as_str) == Some(name))
        }) else {
            continue;
        };
        let Some(target) = anim.as_object_mut() else {
            continue;
        };
        for key in [
            "selection_mode",
            "grid_size",
            "cell_offset_x",
            "cell_offset_y",
        ] {
            if let Some(value) = from_meta.get(key) {
                target.insert(key.to_string(), value.clone());
            }
        }
    }

    Some(engine_animations)
}

/// Normaliza entidades 2D (`kind`, `path`, `control_bindings`, `physics_enabled`)
/// al contrato `Entity3D` del manifest.
fn normalize_engine_entity(raw: &Value) -> Value {
    let mut entity = raw.clone();
    let Some(obj) = entity.as_object_mut() else {
        return entity;
    };

    let model = obj
        .get("model")
        .and_then(Value::as_str)
        .or_else(|| obj.get("path").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();
    let category = obj
        .get("category")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| kind_to_category(obj.get("kind").and_then(Value::as_str)))
        .unwrap_or_else(|| "object".to_string());
    let colision = obj
        .get("colision")
        .and_then(Value::as_bool)
        .or_else(|| obj.get("physics_enabled").and_then(Value::as_bool))
        .unwrap_or(false);

    let controls = obj
        .get("controls")
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| {
            obj.get("control_bindings")
                .filter(|value| !value.is_null())
                .cloned()
        });

    obj.insert("model".into(), Value::String(model));
    obj.insert("category".into(), Value::String(category));
    obj.insert("colision".into(), Value::Bool(colision));
    match controls {
        Some(controls) => {
            obj.insert("controls".into(), controls);
        }
        None => {
            obj.remove("controls");
        }
    }

    if let Some(animations) = obj.get_mut("animations") {
        normalize_loop_flag(animations);
    }

    entity
}

fn normalize_loop_flag(animations: &mut Value) {
    let Some(list) = animations.as_array_mut() else {
        return;
    };
    for anim in list {
        let Some(obj) = anim.as_object_mut() else {
            continue;
        };
        let loop_value = obj.remove("loop_");
        if let Some(loop_value) = loop_value {
            obj.insert("loop".into(), loop_value);
        } else if !obj.contains_key("loop") {
            obj.insert("loop".into(), json!(false));
        }
    }
}

fn kind_to_category(kind: Option<&str>) -> Option<String> {
    let category = match kind? {
        "character" => "character",
        "scenario" => "environment",
        "projectile" => "projectile",
        "collider" | "execution_area" | "model" => "object",
        _ => return None,
    };
    Some(category.to_string())
}

fn merge_library_assets(from_editor: &[Value], from_engine: Option<&Value>) -> Vec<Value> {
    let mut entries: Vec<(String, Value)> = Vec::new();
    for item in from_editor {
        let key = item
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        upsert(&mut entries, key, item.clone());
    }
    if let Some(from_engine) = from_engine.and_then(Value::as_array) {
        for item in from_engine {
            let key = item
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            upsert(&mut entries, key, item.clone());
        }
    }
    entries.into_iter().map(|(_, value)| value).collect()
}

fn merged_models(from_editor: &[Value], engine_scene: &Value) -> Vec<Value> {
    let mut entries: Vec<(String, Value)> = Vec::new();
    for item in from_editor {
        let key = item
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        upsert(&mut entries, key, item.clone());
    }
    if let Some(from_engine) = engine_scene.get("models").and_then(Value::as_array) {
        for item in from_engine {
            let path = item
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let previous = entries
                .iter()
                .find(|(key, _)| key == &path)
                .map(|(_, value)| value);
            let mut merged = Map::new();
            merged.insert(
                "name".into(),
                item.get("name").cloned().unwrap_or(json!(path)),
            );
            merged.insert("path".into(), json!(path));
            copy_first(&mut merged, "category", previous, item);
            copy_first(&mut merged, "model_id", previous, item);
            copy_first(&mut merged, "asset", previous, item);
            if let Some(state) = previous.and_then(|prev| prev.get("state")) {
                merged.insert("state".into(), state.clone());
            }
            upsert(&mut entries, path, Value::Object(merged));
        }
    }
    entries.into_iter().map(|(_, value)| value).collect()
}

fn copy_first(target: &mut Map<String, Value>, key: &str, previous: Option<&Value>, item: &Value) {
    let value = previous
        .and_then(|prev| prev.get(key))
        .filter(|value| !value.is_null())
        .or_else(|| item.get(key).filter(|value| !value.is_null()));
    if let Some(value) = value {
        target.insert(key.to_string(), value.clone());
    }
}

fn build_resources_models(engine_models: Option<&Value>, merged_models: &[Value]) -> Vec<Value> {
    let mut entries: Vec<(String, Value)> = Vec::new();

    for item in engine_models
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(model_id) = item
            .get("model_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        let Some(asset) = item
            .get("asset")
            .and_then(Value::as_str)
            .filter(|asset| !asset.is_empty())
        else {
            continue;
        };
        let entry = json!({
            "id": model_id,
            "name": item.get("name").cloned().unwrap_or(json!(model_id)),
            "type": item.get("category").cloned().unwrap_or(json!("object")),
            "asset": asset,
            "importer_version": RERASSET_IMPORTER_VERSION,
        });
        upsert(&mut entries, model_id.to_string(), entry);
    }

    for item in merged_models {
        let Some(model_id) = item
            .get("model_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        if matches!(
            item.get("state").and_then(Value::as_str),
            Some("importing" | "failed")
        ) {
            continue;
        }
        if entries.iter().any(|(key, _)| key == model_id) {
            continue;
        }
        let asset = item
            .get("asset")
            .and_then(Value::as_str)
            .filter(|asset| !asset.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| imported_model_asset_path(model_id, None));
        let entry = json!({
            "id": model_id,
            "name": item.get("name").cloned().unwrap_or(json!(model_id)),
            "type": item.get("category").cloned().unwrap_or(json!("object")),
            "asset": asset,
            "importer_version": RERASSET_IMPORTER_VERSION,
        });
        upsert(&mut entries, model_id.to_string(), entry);
    }

    entries.into_iter().map(|(_, value)| value).collect()
}

fn imported_model_asset_path(model_id: &str, asset: Option<&str>) -> String {
    asset
        .map(str::to_string)
        .unwrap_or_else(|| format!("imported/models/{model_id}.rerasset"))
}

fn attach_model_ids_to_scene(scene: &mut Value, models: &[Value]) {
    if let Some(entities) = scene.get_mut("entities").and_then(Value::as_array_mut) {
        attach_model_ids(entities, models);
    }
    if let Some(player) = scene.get_mut("player")
        && player.is_object()
    {
        let mut one = vec![std::mem::take(player)];
        attach_model_ids(&mut one, models);
        *player = one.pop().unwrap_or(Value::Null);
    }
}

fn attach_model_ids(entities: &mut [Value], models: &[Value]) {
    let mut by_path: HashMap<String, String> = HashMap::new();
    let mut by_id: HashMap<String, String> = HashMap::new();
    let mut by_basename: HashMap<String, String> = HashMap::new();

    for model in models {
        let Some(model_id) = model
            .get("model_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        by_id.insert(model_id.to_string(), model_id.to_string());
        if let Some(path) = model.get("path").and_then(Value::as_str) {
            by_path.insert(path.to_string(), model_id.to_string());
            by_basename.insert(model_basename(path).to_lowercase(), model_id.to_string());
        }
    }

    for entity in entities {
        let Some(obj) = entity.as_object_mut() else {
            continue;
        };
        if obj.get("model_id").is_some_and(|value| !value.is_null()) {
            continue;
        }
        let Some(model) = obj.get("model").and_then(Value::as_str) else {
            continue;
        };
        let resolved = by_id
            .get(model)
            .or_else(|| by_path.get(model))
            .or_else(|| by_basename.get(&model_basename(model).to_lowercase()));
        if let Some(model_id) = resolved {
            obj.insert("model_id".into(), Value::String(model_id.clone()));
        }
    }
}

fn rewrite_imported_paths(entities: &mut [Value]) {
    for entity in entities {
        let Some(obj) = entity.as_object_mut() else {
            continue;
        };
        let model_id = obj
            .get("model_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if model_id.is_empty() {
            continue;
        }
        let model = obj.get("model").and_then(Value::as_str).unwrap_or("");
        if is_entity_marker(model) {
            continue;
        }
        obj.insert("model".into(), Value::String(model_id.to_string()));
    }
}

fn is_entity_marker(path: &str) -> bool {
    ENTITY_MARKERS.contains(&model_basename(path))
}

fn model_basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn upsert(entries: &mut Vec<(String, Value)>, key: String, value: Value) {
    if let Some(position) = entries.iter().position(|(entry, _)| entry == &key) {
        entries[position].1 = value;
    } else {
        entries.push((key, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn scene_state() -> Value {
        json!({
            "id": 1,
            "name": "Nivel 1",
            "world": { "worldWidth": 10 },
            "entities": [],
            "player": null,
            "sprites": [],
            "models": [{ "path": "C:/old.glb" }]
        })
    }

    fn state() -> ProjectState {
        ProjectState {
            project_type: Some("3D".into()),
            game_style: Some("third-person".into()),
            locale: Some("es".into()),
            scenes: vec![scene_state()],
            active_scene_id: Some(1),
            blueprints: vec![json!({ "id": "bp_1", "model": "C:/models/hero.glb" })],
            entity_meta: BTreeMap::from([(
                "7".to_string(),
                json!({ "scripts": [{ "name": "a", "source": "let x = 1;" }] }),
            )]),
            models: vec![json!({
                "name": "Hero",
                "path": "C:/models/hero.glb",
                "model_id": "model_hero",
                "state": "ready"
            })],
            sounds: vec![json!({ "name": "hit", "path": "C:/sfx/hit.wav" })],
            fonts: vec![json!({ "name": "ui", "path": "C:/fonts/ui.ttf" })],
            hud_images: Vec::new(),
            backgrounds: Vec::new(),
            player_ui_screens: vec![json!({ "id": "main", "name": "Main" })],
            menu_ui_screens: Vec::new(),
        }
    }

    fn engine_scene() -> Value {
        json!({
            "world": {
                "world_width": 20.0,
                "world_height": 20.0,
                "world_depth": 20.0,
                "world_radius": 10.0,
                "grid_visible": true,
                "grid_cell_size": 1.0,
                "gravity": 9.81,
                "target_fps": 60,
                "light_ambient": 0.06,
                "msaa_tier": "high"
            },
            "background_path": null,
            "entities": [{
                "id": 7,
                "name": "Player",
                "category": "character",
                "model": "model_hero",
                "model_id": "model_hero",
                "position": [0.0, 0.0, 0.0],
                "rotation": [0.0, 0.0, 0.0, 1.0],
                "scale": [1.0, 1.0, 1.0],
                "colision": true,
                "animations": [{ "name": "idle", "loop_": true }]
            }],
            "camera2d": null,
            "sprites": [],
            "models": [{
                "name": "Hero",
                "path": "C:/models/hero.glb",
                "category": "character",
                "model_id": "model_hero",
                "asset": "imported/models/model_hero.rerasset"
            }],
            "fonts": [{ "name": "ui", "path": "C:/fonts/ui.ttf" }],
            "player_ui_text_boxes": [{ "scope": "player", "screen_id": "main", "id": 1 }]
        })
    }

    #[test]
    fn manifest_keeps_scene_metadata_and_engine_scene() {
        let manifest = build_manifest(&ManifestInput {
            engine_scene: &engine_scene(),
            state: &state(),
            project_type: "3D",
            game_style: "third-person",
        });

        assert_eq!(manifest["version"], json!(1));
        assert_eq!(manifest["type"], json!("3D"));
        assert_eq!(manifest["language"], json!("es"));
        let scenes = manifest["scenes"].as_array().unwrap();
        assert_eq!(scenes.len(), 1);
        let scene = &scenes[0];
        assert_eq!(scene["name"], json!("Nivel 1"));
        assert_eq!(scene["world"]["worldWidth"], json!(20.0));
        assert_eq!(scene["models"], json!([]));
        assert_eq!(manifest["world"], Value::Null);
        assert_eq!(manifest["playerUiScreens"][0]["id"], json!("main"));
        assert_eq!(manifest["playerUiTextBoxes"][0]["id"], json!(1));
    }

    #[test]
    fn manifest_maps_model_lod_world_fields() {
        let mut scene = engine_scene();
        scene["world"]["model_lod_enabled"] = json!(true);
        scene["world"]["model_lod1_distance_m"] = json!(35.0);
        scene["world"]["model_lod2_distance_m"] = json!(90.0);
        let manifest = build_manifest(&ManifestInput {
            engine_scene: &scene,
            state: &state(),
            project_type: "3D",
            game_style: "third-person",
        });
        let world = &manifest["scenes"][0]["world"];
        assert_eq!(world["lodEnabled"], json!(true));
        assert_eq!(world["lod1Distance"], json!(35.0));
        assert_eq!(world["lod2Distance"], json!(90.0));
    }

    #[test]
    fn merged_entity_gets_scripts_and_loop_flag() {
        let manifest = build_manifest(&ManifestInput {
            engine_scene: &engine_scene(),
            state: &state(),
            project_type: "3D",
            game_style: "third-person",
        });
        let entity = &manifest["scenes"][0]["entities"][0];
        assert_eq!(entity["scripts"][0]["name"], json!("a"));
        assert_eq!(entity["animations"][0]["loop"], json!(true));
        assert!(entity["animations"][0].get("loop_").is_none());
    }

    #[test]
    fn resources_use_engine_asset_and_ready_models() {
        let manifest = build_manifest(&ManifestInput {
            engine_scene: &engine_scene(),
            state: &state(),
            project_type: "3D",
            game_style: "third-person",
        });
        let models = manifest["resources"]["models"].as_array().unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0]["id"], json!("model_hero"));
        assert_eq!(
            models[0]["asset"],
            json!("imported/models/model_hero.rerasset")
        );
        assert_eq!(models[0]["importer_version"], json!(2));
    }

    #[test]
    fn two_d_blueprints_pass_through_unchanged() {
        let blueprint = json!({
            "id": "bp_1",
            "name": "Hero",
            "category": "character",
            "kind": "character",
            "path": "C:/sprites/hero.png",
            "scale": [1.0, 1.0, 1.0]
        });
        let state = ProjectState {
            project_type: Some("2D".into()),
            game_style: Some("top-down".into()),
            scenes: vec![json!({ "id": 1, "name": "Nivel" })],
            active_scene_id: Some(1),
            blueprints: vec![blueprint.clone()],
            ..ProjectState::default()
        };
        let manifest = build_manifest(&ManifestInput {
            engine_scene: &json!({}),
            state: &state,
            project_type: "2D",
            game_style: "top-down",
        });
        assert_eq!(manifest["blueprints"], json!([blueprint]));
    }

    #[test]
    fn two_d_entities_are_normalized() {
        let engine = json!({
            "world": { "world_width": 100.0, "world_height": 100.0, "world_depth": 0.0,
                       "grid_visible": true, "grid_cell_size": 32.0, "gravity": 0.0,
                       "target_fps": 60 },
            "entities": [{
                "id": 3,
                "name": "Hero",
                "kind": "character",
                "path": "C:/sprites/hero.png",
                "position": [0.0, 0.0, 0.0],
                "rotation": [0.0, 0.0, 0.0, 1.0],
                "scale": [1.0, 1.0, 1.0],
                "physics_enabled": true,
                "control_bindings": { "keyboard_mouse": {} }
            }],
            "sprites": [],
            "models": [],
            "sounds": [],
            "backgrounds": []
        });
        let manifest = build_manifest(&ManifestInput {
            engine_scene: &engine,
            state: &ProjectState {
                project_type: Some("2D".into()),
                game_style: Some("top-down".into()),
                ..ProjectState::default()
            },
            project_type: "2D",
            game_style: "top-down",
        });
        let entity = &manifest["scenes"][0]["entities"][0];
        assert_eq!(entity["model"], json!("C:/sprites/hero.png"));
        assert_eq!(entity["category"], json!("character"));
        assert_eq!(entity["colision"], json!(true));
        assert_eq!(entity["controls"]["keyboard_mouse"], json!({}));
        assert!(manifest.get("resources").is_none());
    }
}
