use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::ipc::{
    send_load_progress, send_project_load_3d_complete_event, send_project_loaded_3d_event,
    AnimScriptData, AnimationFrameData, ControlBindingsData, ControlScriptData, EngineCommand,
    EntityRestorePhysics, EntityRestoreTransform, ImportSceneSprite, ProjectLoaded3dEvent,
    ProjectLoaded3dSceneTab, ProjectLoaded3dWorld,
};

use super::State;

const SCRIPT_FILE_PREFIX: &str = "@file:";
const DEFAULT_LIGHT_AMBIENT: f32 = 0.06;
const DEFAULT_LIGHT_INTENSITY: f32 = 1.0;
const DEFAULT_SHADOW_DARKNESS: f32 = 0.22;
const FIRST_PERSON_PLAYER_BODY_SCALE: [f32; 3] = [0.8, 1.7, 0.8];

const ENTITY_MARKERS: &[&str] = &[
    "[EditorBox]",
    "[Ground]",
    "[Player]",
    "[EditorCamera]",
    "[Sun]",
    "[Colisionador]",
    "[ExecutionArea]",
];

// ── Manifest: nombres de campo = claves JSON en `src/shared-types/types.ts`. ─

#[allow(non_snake_case, dead_code)]
#[derive(Debug, Deserialize)]
struct ProjectSaveData {
    #[serde(default)]
    r#type: String,
    gameStyle: String,
    #[serde(default)]
    activeSceneId: Option<u32>,
    #[serde(default)]
    world: Option<SavedWorldConfig>,
    #[serde(default)]
    backgroundPath: Option<String>,
    #[serde(default)]
    entities: Vec<SavedEntity>,
    #[serde(default)]
    playerTransform: Option<SavedPlayerTransform>,
    #[serde(default)]
    sprites: Vec<NamedPath>,
    #[serde(default)]
    models: Vec<NamedPath>,
    #[serde(default)]
    sounds: Vec<NamedPath>,
    #[serde(default)]
    backgrounds: Vec<NamedPath>,
    #[serde(default)]
    scenes: Vec<SavedScene>,
    #[serde(default)]
    blueprints: Vec<SavedBlueprint>,
    #[serde(default)]
    language: Option<String>,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone)]
struct SavedWorldConfig {
    worldWidth: f32,
    worldHeight: f32,
    #[serde(default)]
    worldDepth: Option<f32>,
    #[serde(default)]
    gridVisible: bool,
    gridCellSize: f32,
    #[serde(default)]
    gravity: Option<f32>,
    targetFps: f64,
    #[serde(default)]
    lightAmbient: Option<f32>,
    #[serde(default)]
    lightIntensity: Option<f32>,
    #[serde(default)]
    shadowDarkness: Option<f32>,
}

#[derive(Debug, Deserialize, Clone)]
struct NamedPath {
    name: String,
    path: String,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone, Serialize)]
struct SavedPlayerTransform {
    position: [f32; 3],
    #[serde(default)]
    camera_eye_position: Option<[f32; 3]>,
    #[serde(default)]
    fps_camera_yaw: Option<f32>,
    #[serde(default)]
    fps_camera_pitch: Option<f32>,
    scale: [f32; 3],
    #[serde(default)]
    yaw: Option<f32>,
    #[serde(default)]
    pitch: Option<f32>,
    #[serde(default)]
    visual_model_path: Option<String>,
    #[serde(default)]
    fov_y: Option<f32>,
    #[serde(default)]
    frustum_distance: Option<f32>,
    #[serde(default)]
    camera_follow_mode: Option<String>,
    #[serde(default)]
    control_bindings: Option<SavedControlBindings>,
    #[serde(default)]
    body_rotation: Option<[f32; 4]>,
    #[serde(default)]
    body_scale: Option<[f32; 3]>,
}

#[allow(non_snake_case, dead_code)]
#[derive(Debug, Deserialize, Clone)]
struct SavedScene {
    id: u32,
    #[serde(default)]
    name: String,
    world: SavedWorldConfig,
    #[serde(default)]
    backgroundPath: Option<String>,
    #[serde(default)]
    entities: Vec<SavedEntity>,
    #[serde(default)]
    playerTransform: Option<SavedPlayerTransform>,
    #[serde(default)]
    sprites: Vec<NamedPath>,
    #[serde(default)]
    models: Vec<NamedPath>,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
struct SavedEntity {
    id: u32,
    #[serde(default)]
    name: Option<String>,
    path: String,
    kind: String,
    position: [f32; 3],
    #[serde(default)]
    rotation: Option<[f32; 4]>,
    scale: [f32; 3],
    #[serde(default)]
    physics_enabled: Option<bool>,
    #[serde(default)]
    physics_type: Option<String>,
    #[serde(default)]
    animations: Option<Vec<SavedAnimation>>,
    #[serde(default)]
    scripts: Option<Vec<SavedScript>>,
    #[serde(default)]
    control_bindings: Option<SavedControlBindings>,
    #[serde(default)]
    blueprint_id: Option<String>,
    #[serde(default)]
    visual_model_path: Option<String>,
    #[serde(default)]
    entity_category: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
struct SavedAnimation {
    name: String,
    fps: u32,
    r#loop: bool,
    #[serde(default)]
    is_default: Option<bool>,
    #[serde(default)]
    facing_right: Option<bool>,
    logical_w: u32,
    logical_h: u32,
    #[serde(default)]
    audio_path: Option<String>,
    frames: Vec<SavedAnimationFrame>,
    #[serde(default)]
    scripts: Option<Vec<SavedScript>>,
    #[serde(default)]
    is_cancelable: Option<bool>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
struct SavedAnimationFrame {
    path: String,
    pivot_x: f32,
    pivot_y: f32,
    #[serde(default)]
    src_x: Option<u32>,
    #[serde(default)]
    src_y: Option<u32>,
    #[serde(default)]
    src_w: Option<u32>,
    #[serde(default)]
    src_h: Option<u32>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
struct SavedScript {
    name: String,
    source: String,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
struct SavedControlBindings {
    #[serde(default)]
    keyboard_mouse: HashMap<String, SavedScript>,
    #[serde(default)]
    gamepad: HashMap<String, SavedScript>,
}

/// `BluePrintEntry` en types.ts.
#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone, Serialize)]
struct SavedBlueprint {
    id: String,
    name: String,
    category: String,
    kind: String,
    path: String,
    scale: [f32; 3],
    #[serde(default)]
    rotation: Option<[f32; 4]>,
    #[serde(default)]
    physics_enabled: Option<bool>,
    #[serde(default)]
    physics_type: Option<String>,
    #[serde(default)]
    animations: Option<Vec<SavedAnimation>>,
    #[serde(default)]
    scripts: Option<Vec<SavedScript>>,
    #[serde(default)]
    control_bindings: Option<SavedControlBindings>,
    #[serde(default)]
    visualModelPath: Option<String>,
    #[serde(default)]
    entity_category: Option<String>,
}

#[allow(non_snake_case, dead_code)]
struct ActiveSaveView {
    world: SavedWorldConfig,
    entities: Vec<SavedEntity>,
    models: Vec<NamedPath>,
    playerTransform: Option<SavedPlayerTransform>,
    sceneId: u32,
    sceneName: String,
}

#[derive(Clone)]
struct PendingRestore {
    transform: EntityRestoreTransform,
    name: Option<String>,
    physics_enabled: bool,
    physics_type: String,
    animations: Option<Vec<SavedAnimation>>,
    scripts: Option<Vec<SavedScript>>,
    control_bindings: Option<ControlBindingsData>,
    blueprint_id: Option<String>,
    entity_category: Option<String>,
    visual_model_path: Option<String>,
}

impl State {
    /// Carga proyecto 3D desde la carpeta ya extraída por Electron (manifest + assets).
    pub(crate) fn load_proyect_from_save_path(&mut self, extract_path: &str) {
        let path = if extract_path.trim().is_empty() {
            std::env::var("RER_PROJECT_EXTRACT_DIR").unwrap_or_default()
        } else {
            extract_path.to_string()
        };

        match load_project_from_extract_dir(&path) {
            Ok(mut project) => {
                if project.r#type != "3D" {
                    log::warn!(
                        "tipo '{}' ignorado en binario 3D",
                        project.r#type
                    );
                    return;
                }
                let extract_dir = PathBuf::from(&path);
                resolve_loaded_paths(&mut project, &extract_dir);
                match apply_loaded_proyect_3d(self, &project) {
                    Ok(view) => {
                        send_project_loaded_3d(&project, &view);
                        send_project_load_3d_complete_event();
                    }
                    Err(err) => log::error!("error al aplicar proyecto: {err}"),
                }
            }
            Err(err) => log::error!("error al abrir '{path}': {err}"),
        }
    }
}

fn load_project_from_extract_dir(extract_path: &str) -> Result<ProjectSaveData, String> {
    let extract_dir = Path::new(extract_path);
    if extract_dir.is_file() {
        return Err(
            "se esperaba directorio extraído, no archivo .save (Electron ya descomprimió)".to_string(),
        );
    }
    if !extract_dir.is_dir() {
        return Err("directorio de proyecto no encontrado".to_string());
    }

    let manifest_path = extract_dir.join("manifest.json");
    if !manifest_path.is_file() {
        return Err("manifest.json no encontrado en directorio extraído".to_string());
    }

    let raw = fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
    let project: ProjectSaveData = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if project.r#type.is_empty() || project.gameStyle.is_empty() {
        return Err("manifest.json inválido (falta type o gameStyle)".to_string());
    }

    Ok(project)
}

fn entity_path_marker(p: &str) -> Option<&'static str> {
    let marker = p.split(['/', '\\']).next_back().unwrap_or(p);
    ENTITY_MARKERS
        .iter()
        .copied()
        .find(|m| *m == marker)
}

fn is_player_path(p: &str) -> bool {
    entity_path_marker(p) == Some("[Player]")
}

fn is_sun_path(p: &str) -> bool {
    entity_path_marker(p) == Some("[Sun]")
}

fn is_ground_path(p: &str) -> bool {
    entity_path_marker(p) == Some("[Ground]")
}

fn is_editor_box_path(p: &str) -> bool {
    entity_path_marker(p) == Some("[EditorBox]")
}

fn is_model_3d_path(p: &str) -> bool {
    let lower = p.to_ascii_lowercase();
    lower.ends_with(".glb") || lower.ends_with(".gltf") || lower.ends_with(".fbx")
}

fn is_3d_model_file_entity(entity: &SavedEntity) -> bool {
    if !is_model_3d_path(&entity.path) {
        return false;
    }
    !is_player_path(&entity.path)
        && !is_sun_path(&entity.path)
        && !is_ground_path(&entity.path)
        && !is_editor_box_path(&entity.path)
}

fn path_basename_lower(p: &str) -> String {
    p.split(['/', '\\'])
        .next_back()
        .unwrap_or(p)
        .to_ascii_lowercase()
}

fn paths_match_for_burst(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    path_basename_lower(a) == path_basename_lower(b)
}

fn resolve_path(p: &str, extracted_dir: &Path) -> String {
    if p.is_empty() {
        return p.to_string();
    }
    if entity_path_marker(p).is_some() {
        return entity_path_marker(p).unwrap().to_string();
    }
    let path = Path::new(p);
    if path.is_absolute() {
        return p.replace('/', std::path::MAIN_SEPARATOR_STR);
    }
    let normalized = p.replace('/', std::path::MAIN_SEPARATOR_STR);
    extracted_dir
        .join(normalized)
        .to_string_lossy()
        .into_owned()
}

fn resolve_optional_path(p: &Option<String>, extracted_dir: &Path) -> Option<String> {
    p.as_ref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| resolve_path(s, extracted_dir))
}

fn resolve_script_source(source: &str, extracted_dir: &Path) -> String {
    if source.is_empty() {
        return String::new();
    }
    // Compatibilidad con saves legacy/main: scripts inline (no @file:) se usan tal cual.
    if !source.starts_with(SCRIPT_FILE_PREFIX) {
        return source.to_string();
    }
    let rel = &source[SCRIPT_FILE_PREFIX.len()..];
    let normalized = rel.replace('/', std::path::MAIN_SEPARATOR_STR);
    let abs = extracted_dir.join(normalized);
    if !abs.is_file() {
        log::warn!("script referenciado no encontrado en save: {rel}");
        return String::new();
    }
    fs::read_to_string(&abs).unwrap_or_default()
}

fn resolve_scripts(scripts: &Option<Vec<SavedScript>>, extracted_dir: &Path) -> Option<Vec<SavedScript>> {
    scripts.as_ref().map(|list| {
        list.iter()
            .map(|s| SavedScript {
                name: s.name.clone(),
                source: resolve_script_source(&s.source, extracted_dir),
            })
            .collect()
    })
}

fn resolve_control_bindings(
    bindings: &Option<SavedControlBindings>,
    extracted_dir: &Path,
) -> Option<SavedControlBindings> {
    bindings.as_ref().map(|b| SavedControlBindings {
        keyboard_mouse: b
            .keyboard_mouse
            .iter()
            .map(|(k, s)| {
                (
                    k.clone(),
                    SavedScript {
                        name: s.name.clone(),
                        source: resolve_script_source(&s.source, extracted_dir),
                    },
                )
            })
            .collect(),
        gamepad: b
            .gamepad
            .iter()
            .map(|(k, s)| {
                (
                    k.clone(),
                    SavedScript {
                        name: s.name.clone(),
                        source: resolve_script_source(&s.source, extracted_dir),
                    },
                )
            })
            .collect(),
    })
}

fn resolve_player_transform(
    pt: &Option<SavedPlayerTransform>,
    extracted_dir: &Path,
) -> Option<SavedPlayerTransform> {
    pt.as_ref().map(|view| SavedPlayerTransform {
        visual_model_path: resolve_optional_path(&view.visual_model_path, extracted_dir),
        control_bindings: resolve_control_bindings(&view.control_bindings, extracted_dir),
        ..view.clone()
    })
}

fn resolve_entity(entity: &SavedEntity, extracted_dir: &Path) -> SavedEntity {
    SavedEntity {
        path: resolve_path(&entity.path, extracted_dir),
        scripts: resolve_scripts(&entity.scripts, extracted_dir),
        control_bindings: resolve_control_bindings(&entity.control_bindings, extracted_dir),
        visual_model_path: resolve_optional_path(&entity.visual_model_path, extracted_dir),
        ..entity.clone()
    }
}

fn resolve_loaded_paths(project: &mut ProjectSaveData, extracted_dir: &Path) {
    let has_scenes = !project.scenes.is_empty();

    project.sounds = project
        .sounds
        .iter()
        .map(|s| NamedPath {
            name: s.name.clone(),
            path: resolve_path(&s.path, extracted_dir),
        })
        .collect();
    project.backgrounds = project
        .backgrounds
        .iter()
        .map(|b| NamedPath {
            name: b.name.clone(),
            path: resolve_path(&b.path, extracted_dir),
        })
        .collect();
    project.models = project
        .models
        .iter()
        .map(|m| NamedPath {
            name: m.name.clone(),
            path: resolve_path(&m.path, extracted_dir),
        })
        .collect();
    project.playerTransform = resolve_player_transform(&project.playerTransform, extracted_dir);

    if !has_scenes {
        project.entities = project
            .entities
            .iter()
            .map(|e| resolve_entity(e, extracted_dir))
            .collect();
    } else {
        project.entities.clear();
    }

    project.scenes = project
        .scenes
        .iter()
        .map(|scene| SavedScene {
            backgroundPath: resolve_optional_path(&scene.backgroundPath, extracted_dir),
            models: scene
                .models
                .iter()
                .map(|m| NamedPath {
                    name: m.name.clone(),
                    path: resolve_path(&m.path, extracted_dir),
                })
                .collect(),
            entities: scene
                .entities
                .iter()
                .map(|e| resolve_entity(e, extracted_dir))
                .collect(),
            playerTransform: resolve_player_transform(&scene.playerTransform, extracted_dir),
            ..scene.clone()
        })
        .collect();

    project.blueprints = project
        .blueprints
        .iter()
        .map(|bp| SavedBlueprint {
            path: resolve_path(&bp.path, extracted_dir),
            visualModelPath: resolve_optional_path(&bp.visualModelPath, extracted_dir),
            scripts: resolve_scripts(&bp.scripts, extracted_dir),
            control_bindings: resolve_control_bindings(&bp.control_bindings, extracted_dir),
            ..bp.clone()
        })
        .collect();
}

fn pick_active_save_view(project: &ProjectSaveData) -> Result<ActiveSaveView, String> {
    if !project.scenes.is_empty() {
        let active = project
            .activeSceneId
            .and_then(|id| project.scenes.iter().find(|s| s.id == id))
            .or_else(|| project.scenes.first())
            .ok_or_else(|| "escenas vacías en manifest".to_string())?;
        return Ok(ActiveSaveView {
            world: active.world.clone(),
            entities: active.entities.clone(),
            models: active.models.clone(),
            playerTransform: active
                .playerTransform
                .clone()
                .or_else(|| project.playerTransform.clone()),
            sceneId: active.id,
            sceneName: active.name.clone(),
        });
    }

    let world = project
        .world
        .clone()
        .ok_or_else(|| "manifest sin world ni scenes".to_string())?;

    Ok(ActiveSaveView {
        world,
        entities: project.entities.clone(),
        models: project.models.clone(),
        playerTransform: project.playerTransform.clone(),
        sceneId: 1,
        sceneName: String::new(),
    })
}

fn needs_scene_burst_load(game_style: &str, view: &ActiveSaveView) -> bool {
    if !view.entities.is_empty() {
        return true;
    }
    let saved_player = view.playerTransform.as_ref();
    let player_in_entities = view
        .entities
        .iter()
        .any(|e| e.kind == "character" && is_player_path(&e.path));
    game_style == "first-person" && saved_player.is_some() && !player_in_entities
}

fn find_blueprint<'a>(id: &str, blueprints: &'a [SavedBlueprint]) -> Option<&'a SavedBlueprint> {
    blueprints.iter().find(|bp| bp.id == id)
}

fn map_control_bindings(bindings: Option<&SavedControlBindings>) -> Option<ControlBindingsData> {
    bindings.map(|b| ControlBindingsData {
        keyboard_mouse: b
            .keyboard_mouse
            .iter()
            .map(|(k, s)| {
                (
                    k.clone(),
                    ControlScriptData {
                        name: s.name.clone(),
                        source: s.source.clone(),
                    },
                )
            })
            .collect(),
        gamepad: b
            .gamepad
            .iter()
            .map(|(k, s)| {
                (
                    k.clone(),
                    ControlScriptData {
                        name: s.name.clone(),
                        source: s.source.clone(),
                    },
                )
            })
            .collect(),
    })
}

fn resolve_saved_entity_transform(entity: &SavedEntity) -> EntityRestoreTransform {
    EntityRestoreTransform {
        position: entity.position,
        rotation: entity.rotation.unwrap_or([0.0, 0.0, 0.0, 1.0]),
        scale: entity.scale,
    }
}

fn resolve_entity_transform(
    entity: &SavedEntity,
    blueprints: &[SavedBlueprint],
) -> EntityRestoreTransform {
    let bp = entity
        .blueprint_id
        .as_deref()
        .and_then(|id| find_blueprint(id, blueprints));
    EntityRestoreTransform {
        position: entity.position,
        rotation: bp
            .and_then(|b| b.rotation)
            .or(entity.rotation)
            .unwrap_or([0.0, 0.0, 0.0, 1.0]),
        scale: bp.map(|b| b.scale).unwrap_or(entity.scale),
    }
}

/// Misma resolución que el `else` del `ready` del front (blueprint hereda props).
fn build_generic_pending_restore(
    entity: &SavedEntity,
    transform: EntityRestoreTransform,
    blueprints: &[SavedBlueprint],
) -> PendingRestore {
    let bp = entity
        .blueprint_id
        .as_deref()
        .and_then(|id| find_blueprint(id, blueprints));
    PendingRestore {
        transform,
        name: entity.name.clone(),
        physics_enabled: bp
            .and_then(|b| b.physics_enabled)
            .or(entity.physics_enabled)
            .unwrap_or(false),
        physics_type: bp
            .and_then(|b| b.physics_type.clone())
            .or_else(|| entity.physics_type.clone())
            .unwrap_or_else(|| "static".to_string()),
        animations: bp
            .and_then(|b| b.animations.clone())
            .or_else(|| entity.animations.clone()),
        scripts: bp
            .and_then(|b| b.scripts.clone())
            .or_else(|| entity.scripts.clone()),
        control_bindings: map_control_bindings(
            bp.and_then(|b| b.control_bindings.as_ref())
                .or(entity.control_bindings.as_ref()),
        ),
        blueprint_id: entity.blueprint_id.clone(),
        entity_category: entity.entity_category.clone(),
        visual_model_path: entity.visual_model_path.clone(),
    }
}

fn build_player_pending_restore(
    entity: &SavedEntity,
    transform: EntityRestoreTransform,
    saved_player: Option<&SavedPlayerTransform>,
) -> PendingRestore {
    PendingRestore {
        transform,
        name: entity.name.clone(),
        physics_enabled: true,
        physics_type: "dynamic".to_string(),
        animations: entity.animations.clone(),
        scripts: entity.scripts.clone(),
        control_bindings: map_control_bindings(
            saved_player
                .and_then(|p| p.control_bindings.as_ref())
                .or(entity.control_bindings.as_ref()),
        ),
        blueprint_id: entity.blueprint_id.clone(),
        entity_category: entity.entity_category.clone(),
        visual_model_path: saved_player
            .and_then(|p| p.visual_model_path.clone())
            .or_else(|| entity.visual_model_path.clone()),
    }
}

fn apply_entity_scripts(state: &mut State, id: u32, scripts: Option<&[SavedScript]>) {
    let Some(list) = scripts else { return };
    for script in list {
        state.handle_command(EngineCommand::LoadScript {
            id,
            path: script.name.clone(),
            source: script.source.clone(),
        });
    }
}

fn apply_entity_animations(state: &mut State, id: u32, animations: Option<&[SavedAnimation]>) {
    let Some(list) = animations else { return };
    for anim in list {
        let frames: Vec<AnimationFrameData> = anim
            .frames
            .iter()
            .map(|f| AnimationFrameData {
                path: f.path.clone(),
                pivot_x: f.pivot_x,
                pivot_y: f.pivot_y,
                src_x: f.src_x,
                src_y: f.src_y,
                src_w: f.src_w,
                src_h: f.src_h,
            })
            .collect();
        let anim_scripts: Vec<AnimScriptData> = anim
            .scripts
            .as_ref()
            .map(|scripts| {
                scripts
                    .iter()
                    .map(|s| AnimScriptData {
                        name: s.name.clone(),
                        source: s.source.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        state.handle_command(EngineCommand::SetAnimation {
            id,
            name: anim.name.clone(),
            frames,
            fps: anim.fps,
            loop_: anim.r#loop,
            flip_horizontal: !(anim.facing_right.unwrap_or(true)),
            audio_path: anim.audio_path.clone(),
            logical_w: Some(anim.logical_w),
            logical_h: Some(anim.logical_h),
            scripts: anim_scripts,
            is_cancelable: anim.is_cancelable.unwrap_or(true),
        });
        if anim.is_default.unwrap_or(false) {
            state.handle_command(EngineCommand::SetDefaultAnimation {
                id,
                name: anim.name.clone(),
            });
        }
    }
}

fn apply_full_entity_restore(
    state: &mut State,
    id: u32,
    pending: &PendingRestore,
    model_path: &str,
    skip_transform: bool,
    omit_scale: bool,
) {
    let physics = if pending.physics_enabled {
        Some(EntityRestorePhysics {
            enabled: true,
            body_type: pending.physics_type.clone(),
        })
    } else {
        None
    };
    state.apply_entity_restore_inner(
        id,
        pending.name.clone(),
        &pending.transform,
        physics.as_ref(),
        pending.control_bindings.as_ref(),
        omit_scale,
        skip_transform,
    );

    let is_environment = pending.entity_category.as_deref() == Some("environment");
    if is_environment {
        state.handle_command(EngineCommand::SetPhysics {
            id,
            enabled: true,
            body_type: "static".to_string(),
        });
    }

    apply_entity_scripts(state, id, pending.scripts.as_deref());
    apply_entity_animations(state, id, pending.animations.as_deref());

    if let Some(visual) = pending
        .visual_model_path
        .as_ref()
        .filter(|p| !p.is_empty() && !paths_match_for_burst(p, model_path))
    {
        state.replace_entity_model(id, visual);
    }
}

fn ensure_model_cached(state: &mut State, path: &str) -> bool {
    state.poll_and_advance_model_preloads(64);
    match state.ensure_static_model_cached(path) {
        Ok(()) => true,
        Err(err) => {
            log::error!("no se pudo cachear modelo '{path}': {err}");
            false
        }
    }
}

fn collect_uncached_burst_model_paths(
    queued_paths: &[String],
    preloaded_paths: &[String],
) -> Vec<(String, String)> {
    let mut uncached: Vec<(String, String)> = Vec::new();
    for queued_path in queued_paths {
        if preloaded_paths
            .iter()
            .any(|preloaded| paths_match_for_burst(preloaded, queued_path))
        {
            continue;
        }
        if uncached
            .iter()
            .any(|(existing, _)| paths_match_for_burst(existing, queued_path))
        {
            continue;
        }
        uncached.push((queued_path.clone(), path_basename_lower(queued_path)));
    }
    uncached
}

fn push_unique_model_path(paths: &mut Vec<String>, path: &str, key_of: impl Fn(&str) -> String) {
    if path.trim().is_empty() {
        return;
    }
    let key = key_of(path);
    if paths.iter().any(|p| key_of(p) == key) {
        return;
    }
    paths.push(path.to_string());
}

/// GLB/FBX que la escena instancia al abrir el `.save` (sin biblioteca `view.models`).
fn collect_scene_required_model_paths(state: &State, view: &ActiveSaveView) -> Vec<String> {
    let key_of = |p: &str| state.model_path_key(p);
    let mut paths: Vec<String> = Vec::new();
    for entity in &view.entities {
        if is_3d_model_file_entity(entity) {
            push_unique_model_path(&mut paths, &entity.path, key_of);
        }
        if let Some(visual) = entity.visual_model_path.as_ref().filter(|p| !p.trim().is_empty()) {
            push_unique_model_path(&mut paths, visual, key_of);
        }
    }
    if let Some(player) = view.playerTransform.as_ref() {
        if let Some(visual) = player
            .visual_model_path
            .as_ref()
            .filter(|p| !p.trim().is_empty())
        {
            push_unique_model_path(&mut paths, visual, key_of);
        }
    }
    paths
}

/// Lanza precargas en hilos; no bloquea el hilo del motor (el event loop sigue vivo).
fn log_load_step(total: Instant, step: &mut Instant, message: &str) {
    let step_ms = step.elapsed().as_millis() as u64;
    let total_ms = total.elapsed().as_millis() as u64;
    log::info!(
        "{} (+{} ms, total {} ms)",
        message,
        step_ms,
        total_ms
    );
    send_load_progress(message, Some(step_ms), Some(total_ms));
    *step = Instant::now();
}

fn collect_play_character_warm_model_keys(state: &State, view: &ActiveSaveView) -> HashSet<String> {
    let mut keys = HashSet::new();
    let mut add = |p: &str| {
        if !p.trim().is_empty() {
            keys.insert(state.model_path_key(p));
        }
    };
    if let Some(player) = view.playerTransform.as_ref() {
        if let Some(visual) = player.visual_model_path.as_deref() {
            add(visual);
        }
    }
    for entity in &view.entities {
        if entity.kind == "character" && is_player_path(&entity.path) {
            if let Some(visual) = entity.visual_model_path.as_deref() {
                add(visual);
            }
        }
    }
    keys
}

fn kickoff_preload_models_for_save(
    state: &mut State,
    model_paths: &[String],
    warm_play_keys: &HashSet<String>,
) {
    let mut started = 0usize;
    for path in model_paths {
        let key = state.model_path_key(path);
        if state.static_model_cache.contains_key(&key) || state.model_preload_inflight.contains(&key) {
            continue;
        }
        let label = state.model_display_label(path);
        let msg = format!("Cargando modelo en segundo plano: {label}");
        log::info!("{msg}");
        send_load_progress(&msg, None, None);
        let warm_play = warm_play_keys.contains(&key);
        state.start_model_preload(key, label, warm_play);
        started += 1;
    }
    state.poll_and_advance_model_preloads(64);
    if started == 0 {
        log::info!("Modelos 3D ya en caché, sin precarga nueva");
    }
}

fn load_project_asset_stores(state: &mut State, project: &ProjectSaveData) {
    for sound in &project.sounds {
        state.handle_command(EngineCommand::LoadSound {
            path: sound.path.clone(),
            name: sound.name.clone(),
        });
    }
    for bg in &project.backgrounds {
        state.handle_command(EngineCommand::LoadBackgroundAsset {
            path: bg.path.clone(),
            name: bg.name.clone(),
        });
    }
}

/// Alinea cámara orbital + FPS antes de `load_character` (evita `sync_player_rotation_from_look` con defaults).
fn preset_spawn_camera_from_saved(state: &mut State, saved: &SavedPlayerTransform) {
    use crate::config_3d::character_anchor::{
        PLAY_CHARACTER_EDITOR_ORBIT_PITCH, PLAY_CHARACTER_EDITOR_ORBIT_YAW,
    };

    state.camera.target = glam::Vec3::from_array(saved.position);
    let orbit_yaw = saved.yaw.unwrap_or(PLAY_CHARACTER_EDITOR_ORBIT_YAW);
    let orbit_pitch = saved.pitch.unwrap_or(PLAY_CHARACTER_EDITOR_ORBIT_PITCH);
    state.editor_viewport_yaw = orbit_yaw;
    state.editor_viewport_pitch = orbit_pitch;
    let cam_yaw = saved.fps_camera_yaw.unwrap_or(orbit_yaw);
    let cam_pitch = saved
        .fps_camera_pitch
        .unwrap_or(orbit_pitch)
        .clamp(
            -std::f32::consts::FRAC_PI_2 + 0.05,
            std::f32::consts::FRAC_PI_2 - 0.05,
        );
    state.camera.yaw = cam_yaw;
    state.camera.pitch = cam_pitch;
    if let Some(eye) = saved.camera_eye_position {
        state.play_camera_eye_position = glam::Vec3::from_array(eye);
    }
}

/// Misma secuencia que `character_loaded` + `entity_model_replaced` en el front (`main`).
fn spawn_player_from_pending(
    state: &mut State,
    pending: &PendingRestore,
    saved_player: Option<&SavedPlayerTransform>,
) {
    if let Some(saved) = saved_player {
        preset_spawn_camera_from_saved(state, saved);
    }

    state.load_character("[Player]");
    let Some(id) = state.play_character_entity.filter(|&id| id != 0) else {
        return;
    };

    // Front: skipTransform si hay playerTransform; omitScale si no hay body_scale.
    let skip_transform = saved_player.is_some();
    let omit_scale = saved_player.is_none_or(|s| s.body_scale.is_none());

    apply_full_entity_restore(state, id, pending, "[Player]", skip_transform, omit_scale);
    state.ensure_play_character_kinematic_only();

    // Posición/orientación/cámara desde playerTransform (pies), no entity.position del manifest.
    if let Some(saved) = saved_player {
        apply_saved_play_character_view(state, saved);
    }
}

fn spawn_entity_after_load_model_single(
    state: &mut State,
    path: &str,
    entity_category: Option<&str>,
) -> Option<u32> {
    let before = state.scenario_entities.len();
    state.load_model_single(path, entity_category);
    if state.scenario_entities.len() > before {
        return state.scenario_entities.get(before).copied();
    }
    state.scenario_entities.last().copied()
}

/// Equivalente a `applySavedPlayCharacterView` + handler `set_play_character_view` en main.
fn apply_saved_play_character_view(state: &mut State, view: &SavedPlayerTransform) {
    use crate::config_3d::character_anchor::{
        PLAY_CHARACTER_EDITOR_ORBIT_PITCH, PLAY_CHARACTER_EDITOR_ORBIT_YAW,
    };

    let follow_mode = view.camera_follow_mode.as_deref().map(|m| match m {
        "follow_character" => crate::ipc::PlayCameraFollowMode::FollowCharacter,
        _ => crate::ipc::PlayCameraFollowMode::MoveWithCharacter,
    });
    // Mismos defaults que `EngineCommand::SetPlayCharacterView` (no usar 0.0 si el campo falta).
    let yaw = view.yaw.unwrap_or(PLAY_CHARACTER_EDITOR_ORBIT_YAW);
    let pitch = view.pitch.unwrap_or(PLAY_CHARACTER_EDITOR_ORBIT_PITCH);
    state.apply_play_character_view(
        view.position,
        yaw,
        pitch,
        view.fov_y,
        view.frustum_distance,
        follow_mode,
        view.body_rotation,
        view.body_scale,
        view.camera_eye_position,
        view.fps_camera_yaw,
        view.fps_camera_pitch,
    );
    // Tras `replace_entity_model`, placement deja solo yaw; reaplicar rotación del mesh guardada.
    if let (Some(id), Some(rot)) = (state.play_character_entity, view.body_rotation) {
        let rot_q = glam::Quat::from_xyzw(rot[0], rot[1], rot[2], rot[3]);
        state.apply_play_character_transform_editor(id, None, Some(rot_q), None);
        if state.uses_editor_viewport_camera() {
            state.ensure_editor_camera_entity();
            state.sync_editor_camera_entity_from_viewport();
        }
    }
}

fn apply_loaded_proyect_3d(state: &mut State, project: &ProjectSaveData) -> Result<ActiveSaveView, String> {
    let load_started_at = Instant::now();
    let mut step_started = Instant::now();

    let view = pick_active_save_view(project)?;
    let open_msg = format!(
        "Abriendo escena «{}» ({} entidades)…",
        view.sceneName,
        view.entities.len()
    );
    log::info!("{open_msg}");
    send_load_progress(&open_msg, None, None);
    if state.mount_save_on_empty_world {
        state.mount_save_on_empty_world = false;
        log_load_step(load_started_at, &mut step_started, "Montando escena desde .save");
    } else {
        state.clear_scene_entities_for_save_load();
        state.apply_empty_3d_editor_defaults();
        log_load_step(load_started_at, &mut step_started, "Escena anterior vaciada, leyendo mundo");
    }
    let game_style = project.gameStyle.as_str();
    let blueprints = &project.blueprints;
    let burst_load = needs_scene_burst_load(game_style, &view);
    let saved_player = view.playerTransform.as_ref();

    let light_ambient = view
        .world
        .lightAmbient
        .unwrap_or(DEFAULT_LIGHT_AMBIENT);
    let light_intensity = view
        .world
        .lightIntensity
        .unwrap_or(DEFAULT_LIGHT_INTENSITY);
    let shadow_darkness = view
        .world
        .shadowDarkness
        .unwrap_or(DEFAULT_SHADOW_DARKNESS);

    let depth = view.world.worldDepth.unwrap_or(50.0);
    state.handle_command(EngineCommand::SetWorldSize {
        width: view.world.worldWidth,
        height: view.world.worldHeight,
        depth: Some(depth),
    });
    state.handle_command(EngineCommand::SetGridVisible {
        visible: view.world.gridVisible,
    });
    state.handle_command(EngineCommand::SetGridCellSize {
        size: view.world.gridCellSize,
    });
    let target_fps = if view.world.targetFps.is_finite() && view.world.targetFps > 0.0 {
        view.world.targetFps as u64
    } else {
        60
    };
    state.handle_command(EngineCommand::SetTargetFps { fps: target_fps });
    if let Some(gravity) = view.world.gravity {
        state.handle_command(EngineCommand::SetGravity { gravity });
    }
    state.handle_command(EngineCommand::SetDirectionalLight {
        ambient: Some(light_ambient),
        intensity: Some(light_intensity),
        shadow_darkness: Some(shadow_darkness),
    });

    load_project_asset_stores(state, project);
    log_load_step(load_started_at, &mut step_started, "Sonidos y fondos registrados");
    for model in &view.models {
        state
            .model_store
            .insert(state.model_path_key(&model.path), model.name.clone());
    }
    let scene_model_paths = collect_scene_required_model_paths(state, &view);
    let warm_play_keys = collect_play_character_warm_model_keys(state, &view);
    kickoff_preload_models_for_save(state, &scene_model_paths, &warm_play_keys);
    log_load_step(
        load_started_at,
        &mut step_started,
        &format!(
            "Precarga de modelos de escena lanzada ({} archivo/s)",
            scene_model_paths.len()
        ),
    );

    let burst_load_planned = burst_load;
    if !view.models.is_empty() && !burst_load_planned {
        for model in &view.models {
            let _ = ensure_model_cached(state, &model.path);
            state
                .model_store
                .insert(state.model_path_key(&model.path), model.name.clone());
        }
    }

    let mut model_load_queue: Vec<(String, PendingRestore)> = Vec::new();

    for entity in &view.entities {
        state.poll_and_advance_model_preloads(64);
        let transform = if is_3d_model_file_entity(entity) {
            resolve_saved_entity_transform(entity)
        } else {
            resolve_entity_transform(entity, blueprints)
        };

        if entity.kind == "character" && is_player_path(&entity.path) {
            let pending = build_player_pending_restore(entity, transform, saved_player);
            spawn_player_from_pending(state, &pending, saved_player);
        } else if entity.kind == "directional_light" || is_sun_path(&entity.path) {
            let pending = build_generic_pending_restore(entity, transform, blueprints);
            state.spawn_sun(
                entity.name.as_deref().unwrap_or(""),
                entity.position,
                entity.scale,
            );
            if let Some(id) = state.sun_entity {
                apply_full_entity_restore(state, id, &pending, "[Sun]", true, false);
            }
        } else if entity.kind == "model" && is_ground_path(&entity.path) {
            let pending = build_generic_pending_restore(entity, transform, blueprints);
            state.spawn_ground_plane(entity.position, entity.scale);
            if let Some(id) = state.ground_entity_id() {
                apply_full_entity_restore(state, id, &pending, "[Ground]", true, false);
            }
        } else if entity.kind == "model" && is_editor_box_path(&entity.path) {
            let pending = build_generic_pending_restore(entity, transform, blueprints);
            let box_label = entity.name.as_deref().unwrap_or("Caja");
            log::info!(
                "Colocando bloque «{box_label}» en [{:.1}, {:.1}, {:.1}]",
                entity.position[0],
                entity.position[1],
                entity.position[2]
            );
            state.spawn_editor_box(
                box_label,
                entity.position,
                entity.scale,
            );
            if let Some(id) = state.scenario_entities.last().copied() {
                apply_full_entity_restore(state, id, &pending, "[EditorBox]", true, false);
            }
        } else if entity.kind == "scenario" && !is_model_3d_path(&entity.path) {
            // Escenarios 2D legacy sin archivo 3D.
        } else {
            let pending = build_generic_pending_restore(entity, transform, blueprints);

            if entity.kind == "scenario" {
                state.load_scenario(&entity.path);
            }

            if is_3d_model_file_entity(entity) {
                model_load_queue.push((entity.path.clone(), pending.clone()));

                if !burst_load_planned {
                    if !ensure_model_cached(state, &entity.path) {
                        continue;
                    }
                    let category = entity.entity_category.as_deref();
                    if let Some(id) = spawn_entity_after_load_model_single(state, &entity.path, category)
                    {
                        apply_full_entity_restore(state, id, &pending, &entity.path, false, false);
                    }
                }
            }
        }
    }

    log_load_step(
        load_started_at,
        &mut step_started,
        &format!("Entidades del manifest procesadas ({})", view.entities.len()),
    );

    if burst_load_planned {
        log::info!("Instanciando modelos 3D (carga por lotes)…");
        state.poll_and_advance_model_preloads(64);
        for model in &view.models {
            if !model.path.trim().is_empty() {
                state
                    .model_store
                    .insert(state.model_path_key(&model.path), model.name.clone());
            }
        }

        let queued_paths: Vec<String> = model_load_queue.iter().map(|(p, _)| p.clone()).collect();
        for (path, name) in collect_uncached_burst_model_paths(&queued_paths, &[]) {
            if ensure_model_cached(state, &path) {
                state
                    .model_store
                    .insert(state.model_path_key(&path), name);
            }
        }

        for (path, pending) in model_load_queue {
            state.poll_and_advance_model_preloads(64);
            if !ensure_model_cached(state, &path) {
                continue;
            }
            if let Ok(id) = state.spawn_cached_model_from_save(
                &path,
                pending.transform.position,
                pending.transform.rotation,
                pending.transform.scale,
                pending.name.as_deref(),
                pending.entity_category.clone(),
                pending.blueprint_id.clone(),
                pending.physics_enabled,
                &pending.physics_type,
            ) {
                apply_full_entity_restore(state, id, &pending, &path, false, false);
            }
        }
    }

    if game_style == "first-person" {
        let player_in_entities = view
            .entities
            .iter()
            .any(|e| e.kind == "character" && is_player_path(&e.path));
        if !player_in_entities {
            if let Some(saved) = saved_player {
                let pending = PendingRestore {
                    transform: EntityRestoreTransform {
                        position: [0.0, FIRST_PERSON_PLAYER_BODY_SCALE[1] * 0.5, 0.0],
                        rotation: [0.0, 0.0, 0.0, 1.0],
                        scale: FIRST_PERSON_PLAYER_BODY_SCALE,
                    },
                    name: Some("Player".to_string()),
                    physics_enabled: true,
                    physics_type: "dynamic".to_string(),
                    animations: None,
                    scripts: None,
                    control_bindings: map_control_bindings(saved.control_bindings.as_ref()),
                    blueprint_id: None,
                    entity_category: None,
                    visual_model_path: saved.visual_model_path.clone(),
                };
                spawn_player_from_pending(state, &pending, saved_player);
            }
        }
    }

    if burst_load_planned {
        log_load_step(load_started_at, &mut step_started, "Modelos 3D instanciados");
    }

    if let Some(saved) = saved_player {
        apply_saved_play_character_view(state, saved);
        log::info!(
            "Jugador colocado en [{:.2}, {:.2}, {:.2}]",
            saved.position[0],
            saved.position[1],
            saved.position[2]
        );
    } else {
        log::warn!("El manifest no trae playerTransform; spawn por defecto");
    }

    let done_msg = format!(
        "Carga terminada — escena «{}» ({} entidades, {} ms)",
        view.sceneName,
        view.entities.len(),
        load_started_at.elapsed().as_millis()
    );
    log::info!("{done_msg}");
    send_load_progress(
        &done_msg,
        None,
        Some(load_started_at.elapsed().as_millis() as u64),
    );
    Ok(view)
}

fn send_project_loaded_3d(project: &ProjectSaveData, view: &ActiveSaveView) {
    let scenes: Vec<ProjectLoaded3dSceneTab> = if project.scenes.is_empty() {
        vec![ProjectLoaded3dSceneTab {
            id: view.sceneId,
            name: view.sceneName.clone(),
        }]
    } else {
        project
            .scenes
            .iter()
            .map(|s| ProjectLoaded3dSceneTab {
                id: s.id,
                name: s.name.clone(),
            })
            .collect()
    };

    let world = ProjectLoaded3dWorld {
        worldWidth: view.world.worldWidth,
        worldHeight: view.world.worldHeight,
        worldDepth: view.world.worldDepth,
        gridVisible: view.world.gridVisible,
        gridCellSize: view.world.gridCellSize,
        gravity: view.world.gravity,
        targetFps: view.world.targetFps,
        lightAmbient: view.world.lightAmbient,
        lightIntensity: view.world.lightIntensity,
        shadowDarkness: view.world.shadowDarkness,
    };

    let models: Vec<ImportSceneSprite> = view
        .models
        .iter()
        .map(|m| ImportSceneSprite {
            path: m.path.clone(),
            name: m.name.clone(),
        })
        .collect();

    let blueprints =
        serde_json::to_value(&project.blueprints).unwrap_or(serde_json::Value::Array(vec![]));

    let player_transform = view
        .playerTransform
        .as_ref()
        .and_then(|pt| serde_json::to_value(pt).ok());

    send_project_loaded_3d_event(&ProjectLoaded3dEvent {
        event: "project_loaded_3d",
        activeSceneId: view.sceneId,
        sceneName: view.sceneName.clone(),
        entityCount: view.entities.len() as u32,
        scenes,
        language: project.language.clone(),
        models,
        sounds: project
            .sounds
            .iter()
            .map(|s| ImportSceneSprite {
                path: s.path.clone(),
                name: s.name.clone(),
            })
            .collect(),
        backgrounds: project
            .backgrounds
            .iter()
            .map(|b| ImportSceneSprite {
                path: b.path.clone(),
                name: b.name.clone(),
            })
            .collect(),
        blueprints,
        world,
        playerTransform: player_transform,
    });
}
