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

/// `true` si una entidad del snapshot/manifiesto es contenido DEMO de la
/// plantilla 3D. Usa el mismo criterio que el motor en «Limpiar escena».
fn is_demo_manifest_entity(entity: &Value) -> bool {
    let path = entity
        .get("model")
        .and_then(Value::as_str)
        .or_else(|| entity.get("path").and_then(Value::as_str))
        .unwrap_or_default();
    crate::engine3d::is_demo_entity_path(path)
}

/// `true` si un emisor serializado pertenece a la fila demo de partículas.
fn is_demo_manifest_emitter(emitter: &Value) -> bool {
    let id = emitter.get("id").and_then(Value::as_u64).unwrap_or(0);
    let material = emitter
        .get("material")
        .and_then(Value::as_str)
        .unwrap_or("");
    let position = emitter.get("position").and_then(Value::as_array);
    let at = |i: usize| {
        position
            .and_then(|p| p.get(i))
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32
    };
    crate::engine3d::particles::is_demo_showcase_wire(material, [at(0), at(1), at(2)], id)
}

/// Blindaje de guardado: el contenido DEMO nunca se escribe en un `.save`,
/// aunque esté presente en la escena del motor (p. ej. guardar la demo sin
/// haber pulsado «Limpiar escena»). Aplica a la escena activa y a las pestañas.
fn strip_demo_from_scenes(scenes: &mut [Value]) -> usize {
    let mut removed = 0usize;
    for scene in scenes {
        if let Some(entities) = scene.get_mut("entities").and_then(Value::as_array_mut) {
            let before = entities.len();
            entities.retain(|entity| !is_demo_manifest_entity(entity));
            removed += before - entities.len();
        }
        if let Some(particles) = scene
            .get_mut("world")
            .and_then(|world| world.get_mut("particles"))
            && let Some(emitters) = particles.get_mut("emitters").and_then(Value::as_array_mut)
        {
            let before = emitters.len();
            emitters.retain(|emitter| !is_demo_manifest_emitter(emitter));
            removed += before - emitters.len();
        }
    }
    removed
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

    if input.project_type == "3D" {
        let removed = strip_demo_from_scenes(&mut scenes);
        if removed > 0 {
            log::info!("[save] {removed} elementos DEMO excluidos del .save");
        }
    }

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
        build_resources_models(engine_scene.get("models"), &merged_models, &scenes, state)
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
    let tooltips = state
        .interaction_tooltips
        .iter()
        .filter(|tooltip| {
            !(input.project_type == "3D"
                && crate::editor::ui::sidebar_left::is_demo_tooltip(tooltip))
        })
        .cloned()
        .collect();
    root.insert("interactionTooltips".into(), Value::Array(tooltips));

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
        ("worldCenterX", "world_center_x"),
        ("worldCenterZ", "world_center_z"),
        ("gridVisible", "grid_visible"),
        ("gridCellSize", "grid_cell_size"),
        ("gravity", "gravity"),
        ("targetFps", "target_fps"),
        ("lightAmbient", "light_ambient"),
        ("lightIntensity", "light_intensity"),
        ("shadowDarkness", "shadow_darkness"),
        ("sunCycleEnabled", "sun_cycle_enabled"),
        ("sunRiseCardinal", "sun_rise_cardinal"),
        ("sunCycleDay", "sun_cycle_day"),
        ("sunCycleNight", "sun_cycle_night"),
        ("sunCyclePhase", "sun_cycle_phase"),
        ("graphicsTextureTier", "graphics_texture_tier"),
        ("textureDetailDistance", "texture_detail_distance_m"),
        ("reflectionTier", "reflection_tier"),
        ("reflectionMode", "reflection_mode"),
        ("rtTier", "rt_tier"),
        ("reflectionRaytracing", "reflection_raytracing"),
        ("reflectionProbes", "reflection_probes"),
        ("shadowTier", "shadow_tier"),
        ("msaaTier", "msaa_tier"),
        ("ssaaTier", "ssaa_tier"),
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
    // Partículas (F5): la sección entera viaja verbatim (clima + emisores con sus
    // campos snake_case); la restauración la deserializa tal cual.
    if let Some(particles) = world.get("particles").filter(|value| value.is_object()) {
        out.insert("particles".into(), particles.clone());
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
    // Los bindings los edita el editor nativo en `entity_meta`; cópielos al
    // entity del manifest para que se persistan sin depender del snapshot.
    if let Some(controls) = meta
        .get("controlBindings")
        .or_else(|| meta.get("controls"))
        .filter(|value| !value.is_null())
    {
        obj.insert("controls".into(), controls.clone());
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
            "root_motion",
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
            copy_first(&mut merged, "global", previous, item);
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

fn imported_model_asset_path(model_id: &str, asset: Option<&str>) -> String {
    asset
        .map(str::to_string)
        .unwrap_or_else(|| format!("imported/models/{model_id}.rerasset"))
}

/// Ruta relativa de un recurso por escena dentro del `.save`.
fn scene_model_asset_path(scene_id: u64, model_id: &str) -> String {
    format!("scene-{scene_id}/assets/{model_id}.rerasset")
}

/// Uso de cada `model_id` por escena: `entities[].model_id` + `player.model_id`.
fn collect_scene_model_usage(scenes: &[Value]) -> HashMap<String, Vec<u64>> {
    let mut usage: HashMap<String, Vec<u64>> = HashMap::new();
    for scene in scenes {
        let Some(scene_id) = scene_id(scene) else {
            continue;
        };
        let mut push = |model_id: Option<&str>| {
            if let Some(id) = model_id.map(str::trim).filter(|id| !id.is_empty()) {
                let list = usage.entry(id.to_string()).or_default();
                if !list.contains(&scene_id) {
                    list.push(scene_id);
                }
            }
        };
        if let Some(entities) = scene.get("entities").and_then(Value::as_array) {
            for entity in entities {
                push(entity.get("model_id").and_then(Value::as_str));
            }
        }
        push(
            scene
                .get("player")
                .and_then(|player| player.get("model_id"))
                .and_then(Value::as_str),
        );
    }
    usage
}

/// Las blueprints son plantillas globales: si usan un modelo, vive en la
/// biblioteca global aunque solo lo referencie una escena.
fn blueprints_use_model(state: &ProjectState, model_id: &str) -> bool {
    state.blueprints.iter().any(|bp| {
        bp.get("model_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id == model_id)
    })
}

/// Decisión de alcance de un modelo: `Some(None)` = global (compartido entre
/// escenas, usado por blueprint o biblioteca sin uso), `Some(Some(id))` =
/// recurso exclusivo de la escena `id` en proyecto multi-escena.
fn model_scene_scope(
    usage: &HashMap<String, Vec<u64>>,
    scene_count: usize,
    state: &ProjectState,
    model_id: &str,
) -> Option<Option<u64>> {
    let owners = usage.get(model_id).map(Vec::as_slice).unwrap_or(&[]);
    if owners.len() >= 2 || blueprints_use_model(state, model_id) {
        // Promoción a biblioteca global (recurso compartido entre escenas).
        return Some(None);
    }
    if owners.len() == 1 && scene_count > 1 {
        return Some(Some(owners[0]));
    }
    None
}

fn make_resource_entry(
    model_id: &str,
    name: &Value,
    model_type: &Value,
    scene_id: Option<u64>,
    asset: String,
    global: bool,
) -> Value {
    let mut entry = Map::new();
    entry.insert("id".into(), json!(model_id));
    entry.insert("name".into(), name.clone());
    entry.insert("type".into(), model_type.clone());
    entry.insert("asset".into(), Value::String(asset));
    entry.insert("importer_version".into(), json!(RERASSET_IMPORTER_VERSION));
    if let Some(scene_id) = scene_id {
        entry.insert("scene_id".into(), json!(scene_id));
    }
    if global {
        // Override explícito del usuario («Globalizar modelo»): se conserva al
        // recargar para que el enrutado automático no vuelva a atarlo a una escena.
        entry.insert("global".into(), json!(true));
    }
    Value::Object(entry)
}

/// Biblioteca 3D `resources.models[]` con rutas por escena:
/// - Compartido entre escenas / blueprint / proyecto de una escena → global
///   (`imported/models/{id}.rerasset`, sin `scene_id`).
/// - Exclusivo de una escena en proyecto multi-escena → `scene-{id}/assets/{id}.rerasset`
///   con `scene_id` (carga lazy y liberación al cambiar de escena).
fn build_resources_models(
    engine_models: Option<&Value>,
    merged_models: &[Value],
    scenes: &[Value],
    state: &ProjectState,
) -> Vec<Value> {
    let scene_count = scenes.len();
    let usage = collect_scene_model_usage(scenes);
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
        let forced_global = item.get("global").and_then(Value::as_bool).unwrap_or(false);
        let (scene_id, asset) = if forced_global {
            (
                None,
                if asset.starts_with("scene-") {
                    imported_model_asset_path(model_id, None)
                } else {
                    asset.to_string()
                },
            )
        } else {
            match model_scene_scope(&usage, scene_count, state, model_id) {
                Some(None) => (
                    None,
                    // Conserva rutas no por escena (global relativa o absoluta de sesión).
                    if asset.starts_with("scene-") {
                        imported_model_asset_path(model_id, None)
                    } else {
                        asset.to_string()
                    },
                ),
                Some(Some(scene_id)) => {
                    (Some(scene_id), scene_model_asset_path(scene_id, model_id))
                }
                None => (None, imported_model_asset_path(model_id, Some(asset))),
            }
        };
        let entry = make_resource_entry(
            model_id,
            &item.get("name").cloned().unwrap_or(json!(model_id)),
            &item.get("category").cloned().unwrap_or(json!("object")),
            scene_id,
            asset,
            forced_global,
        );
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
        let provided_asset = item
            .get("asset")
            .and_then(Value::as_str)
            .filter(|asset| !asset.is_empty());
        let forced_global = item.get("global").and_then(Value::as_bool).unwrap_or(false);
        let (scene_id, asset) = if forced_global {
            (
                None,
                provided_asset
                    .filter(|asset| !asset.starts_with("scene-"))
                    .map(str::to_string)
                    .unwrap_or_else(|| imported_model_asset_path(model_id, None)),
            )
        } else {
            match model_scene_scope(&usage, scene_count, state, model_id) {
                Some(None) => (
                    None,
                    provided_asset
                        .filter(|asset| !asset.starts_with("scene-"))
                        .map(str::to_string)
                        .unwrap_or_else(|| imported_model_asset_path(model_id, None)),
                ),
                Some(Some(scene_id)) => {
                    (Some(scene_id), scene_model_asset_path(scene_id, model_id))
                }
                None => (None, imported_model_asset_path(model_id, provided_asset)),
            }
        };
        let entry = make_resource_entry(
            model_id,
            &item.get("name").cloned().unwrap_or(json!(model_id)),
            &item.get("category").cloned().unwrap_or(json!("object")),
            scene_id,
            asset,
            forced_global,
        );
        upsert(&mut entries, model_id.to_string(), entry);
    }

    entries.into_iter().map(|(_, value)| value).collect()
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
            interaction_tooltips: Vec::new(),
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
    fn manifest_maps_sun_cycle_world_fields() {
        let mut scene = engine_scene();
        scene["world"]["sun_cycle_enabled"] = json!(true);
        scene["world"]["sun_rise_cardinal"] = json!("north");
        scene["world"]["sun_cycle_day"] = json!(1200.0);
        scene["world"]["sun_cycle_night"] = json!(800.0);
        scene["world"]["sun_cycle_phase"] = json!(0.42);
        let manifest = build_manifest(&ManifestInput {
            engine_scene: &scene,
            state: &state(),
            project_type: "3D",
            game_style: "third-person",
        });
        let world = &manifest["scenes"][0]["world"];
        assert_eq!(world["sunCycleEnabled"], json!(true));
        assert_eq!(world["sunRiseCardinal"], json!("north"));
        assert_eq!(world["sunCycleDay"], json!(1200.0));
        assert_eq!(world["sunCycleNight"], json!(800.0));
        assert_eq!(world["sunCyclePhase"], json!(0.42));
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
    fn manifest_keeps_root_motion_from_editor_meta() {
        let mut project = state();
        project
            .entity_meta
            .get_mut("7")
            .expect("meta de la entidad 7")
            .as_object_mut()
            .expect("meta es objeto")
            .insert(
                "animations".into(),
                json!([{ "name": "idle", "root_motion": true }]),
            );
        let manifest = build_manifest(&ManifestInput {
            engine_scene: &engine_scene(),
            state: &project,
            project_type: "3D",
            game_style: "third-person",
        });
        assert_eq!(
            manifest["scenes"][0]["entities"][0]["animations"][0]["root_motion"],
            json!(true)
        );
    }

    #[test]
    fn manifest_keeps_control_bindings_from_editor_meta() {
        let mut project = state();
        project
            .entity_meta
            .get_mut("7")
            .expect("meta de la entidad 7")
            .as_object_mut()
            .expect("meta es objeto")
            .insert(
                "controlBindings".into(),
                json!({
                    "keyboard_mouse": { "W": { "name": "walk", "source": "let x = 1;" } },
                    "gamepad": {}
                }),
            );
        let manifest = build_manifest(&ManifestInput {
            engine_scene: &engine_scene(),
            state: &project,
            project_type: "3D",
            game_style: "third-person",
        });
        let controls = &manifest["scenes"][0]["entities"][0]["controls"];
        assert_eq!(controls["keyboard_mouse"]["W"]["name"], json!("walk"));
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
    fn globalized_model_keeps_global_routing_on_save() {
        // `model_tree` es exclusivo de la escena 2, pero el usuario lo globalizó:
        // debe guardarse como global (sin scene_id) y con el override `global`.
        let scenes = vec![
            json!({ "id": 1, "entities": [], "player": null }),
            json!({
                "id": 2,
                "entities": [{ "model_id": "model_tree" }],
                "player": null
            }),
        ];
        let engine_models = json!([
            {
                "model_id": "model_tree",
                "asset": "imported/models/model_tree.rerasset",
                "global": true
            },
        ]);

        let models = build_resources_models(Some(&engine_models), &[], &scenes, &state());
        let tree = models
            .iter()
            .find(|entry| entry["id"] == json!("model_tree"))
            .expect("falta model_tree");

        assert_eq!(tree["asset"], json!("imported/models/model_tree.rerasset"));
        assert!(tree.get("scene_id").is_none());
        assert_eq!(tree["global"], json!(true));
    }

    #[test]
    fn resources_route_models_per_scene_and_promote_shared() {
        // Multi-escena: `model_hero` compartido (→ global), `model_tree`
        // exclusivo de la escena 2 (→ per-scene), `model_npc` usado por
        // blueprint (→ global aunque solo lo referencie la escena 1).
        let scenes = vec![
            json!({
                "id": 1,
                "entities": [
                    { "model_id": "model_hero" },
                    { "model_id": "model_npc" }
                ],
                "player": null
            }),
            json!({
                "id": 2,
                "entities": [
                    { "model_id": "model_hero" },
                    { "model_id": "model_tree" }
                ],
                "player": { "model_id": "model_tree" }
            }),
        ];
        let engine_models = json!([
            { "model_id": "model_hero", "asset": "imported/models/model_hero.rerasset" },
            { "model_id": "model_tree", "asset": "imported/models/model_tree.rerasset" },
            { "model_id": "model_npc", "asset": "imported/models/model_npc.rerasset" },
        ]);
        let mut project_state = state();
        project_state.blueprints = vec![json!({ "model_id": "model_npc" })];

        let models = build_resources_models(Some(&engine_models), &[], &scenes, &project_state);

        let find = |id: &str| {
            models
                .iter()
                .find(|entry| entry["id"] == json!(id))
                .unwrap_or_else(|| panic!("falta el recurso {id}"))
                .clone()
        };

        let hero = find("model_hero");
        assert_eq!(hero["asset"], json!("imported/models/model_hero.rerasset"));
        assert!(hero.get("scene_id").is_none());

        let tree = find("model_tree");
        assert_eq!(tree["asset"], json!("scene-2/assets/model_tree.rerasset"));
        assert_eq!(tree["scene_id"], json!(2));

        let npc = find("model_npc");
        assert_eq!(npc["asset"], json!("imported/models/model_npc.rerasset"));
        assert!(npc.get("scene_id").is_none());
    }

    #[test]
    fn resources_keep_single_scene_project_global() {
        // Proyecto de una sola escena: todo global (compatible con saves antiguos).
        let scenes = vec![json!({
            "id": 1,
            "entities": [{ "model_id": "model_hero" }],
            "player": null
        })];
        let engine_models = json!([
            { "model_id": "model_hero", "asset": "imported/models/model_hero.rerasset" },
        ]);
        let models = build_resources_models(Some(&engine_models), &[], &scenes, &state());
        assert_eq!(models.len(), 1);
        assert_eq!(
            models[0]["asset"],
            json!("imported/models/model_hero.rerasset")
        );
        assert!(models[0].get("scene_id").is_none());
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

    #[test]
    fn demo_entities_never_reach_the_manifest() {
        let mut scene = engine_scene();
        let demo_paths = [
            "[EditorBox]",
            "[Ball]",
            "[MatComparison]",
            "[MatVal]",
            "[MatValLabel]",
            "[ReflectionProbe]",
            "[ParticleDemoLabel]",
            "[InteractDemo]",
            "[InteractDemoLabel]",
        ];
        let mut entities = Vec::new();
        for (index, path) in demo_paths.iter().enumerate() {
            entities.push(json!({
                "id": 100 + index,
                "name": path,
                "model": path,
                "position": [0.0, 0.0, 0.0],
                "rotation": [0.0, 0.0, 0.0, 1.0],
                "scale": [1.0, 1.0, 1.0],
            }));
        }
        entities.push(json!({
            "id": 200,
            "name": "Roca",
            "model": "model_rock",
            "position": [1.0, 0.0, 2.0],
            "rotation": [0.0, 0.0, 0.0, 1.0],
            "scale": [1.0, 1.0, 1.0],
        }));
        scene["entities"] = json!(entities);

        let manifest = build_manifest(&ManifestInput {
            engine_scene: &scene,
            state: &state(),
            project_type: "3D",
            game_style: "first-person",
        });
        let saved = manifest["scenes"][0]["entities"].as_array().unwrap();
        assert_eq!(
            saved.len(),
            1,
            "solo la entidad del usuario debe persistirse"
        );
        assert_eq!(saved[0]["model"], json!("model_rock"));
    }

    #[test]
    fn demo_particle_emitters_never_reach_the_manifest() {
        let mut scene = engine_scene();
        let mut emitters = Vec::new();
        for index in 0..5 {
            emitters.push(json!({
                "id": crate::engine3d::particles::demo_emitter_id(index),
                "material": "fire",
                "position": [10.0, 0.35, -1.0 + index as f32 * 2.0],
            }));
        }
        // Id de usuario pero pose de la fila demo: también debe descartarse.
        emitters.push(json!({
            "id": 9000,
            "material": "fire",
            "position": [13.8, 0.35, -7.6],
        }));
        emitters.push(json!({
            "id": 9001,
            "material": "smoke",
            "position": [2.0, 0.5, 3.0],
        }));
        scene["world"]["particles"] = json!({
            "weather": { "kind": "rain", "intensity": 0.5, "fall_speed": 1.0, "wind_x": 0.0 },
            "emitters": emitters,
        });

        let manifest = build_manifest(&ManifestInput {
            engine_scene: &scene,
            state: &state(),
            project_type: "3D",
            game_style: "first-person",
        });
        let particles = &manifest["scenes"][0]["world"]["particles"];
        let saved = particles["emitters"].as_array().unwrap();
        assert_eq!(
            saved.len(),
            1,
            "solo el emisor del usuario debe persistirse"
        );
        assert_eq!(saved[0]["id"], json!(9001));
        assert!(particles["weather"].is_object());
    }

    #[test]
    fn demo_entities_in_inactive_scene_tabs_are_stripped() {
        let mut s = state();
        s.scenes.push(json!({
            "id": 2,
            "name": "Nivel 2",
            "world": { "worldWidth": 10 },
            "entities": [
                { "id": 1, "name": "Demo", "model": "[EditorBox]",
                  "position": [0,0,0], "rotation": [0,0,0,1], "scale": [1,1,1] },
                { "id": 2, "name": "Roca", "model": "model_rock",
                  "position": [0,0,0], "rotation": [0,0,0,1], "scale": [1,1,1] },
            ],
            "player": null,
            "sprites": [],
            "models": [],
        }));
        let manifest = build_manifest(&ManifestInput {
            engine_scene: &engine_scene(),
            state: &s,
            project_type: "3D",
            game_style: "third-person",
        });
        let scenes = manifest["scenes"].as_array().unwrap();
        let scene2 = scenes.iter().find(|scene| scene["id"] == 2).unwrap();
        let saved = scene2["entities"].as_array().unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0]["model"], json!("model_rock"));
    }

    #[test]
    fn demo_interaction_tooltips_never_reach_the_manifest() {
        let mut s = state();
        s.interaction_tooltips = crate::editor::ui::sidebar_left::demo_tooltips();
        s.interaction_tooltips
            .push(json!({ "id": "my_tooltip", "name": "Custom" }));

        let manifest = build_manifest(&ManifestInput {
            engine_scene: &engine_scene(),
            state: &s,
            project_type: "3D",
            game_style: "first-person",
        });
        let saved = manifest["interactionTooltips"].as_array().unwrap();
        assert_eq!(
            saved.len(),
            1,
            "solo el tooltip del usuario debe persistirse"
        );
        assert_eq!(saved[0]["id"], json!("my_tooltip"));
    }
}
