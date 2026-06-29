use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::ipc::{send_event, send_load_progress, send_project_load_3d_complete_event, send_project_loaded_3d_event, AnimScriptData, AnimationFrameData, ControlBindingsData, ControlScriptData, EngineCommand, EngineCommand3dOnly, EngineCommandCommon, EngineEvent, EntityRestorePhysics, EntityRestoreTransform, ImportSceneSprite, ProjectLoaded3dEvent, ProjectLoaded3dSceneTab, ProjectLoaded3dWorld};

use super::State;
use crate::assets::registry::{relative_rerasset_manifest_path, resolve_manifest_asset_path};
use crate::config_3d::static_model_cache::MODEL_GPU_PARTS_DURING_SAVE_LOAD;
use rer_engine_shared::assets::AssetState;

const SCRIPT_FILE_PREFIX: &str = "@file:";
const DEFAULT_LIGHT_AMBIENT: f32 = 0.06;
const DEFAULT_LIGHT_INTENSITY: f32 = 1.0;
const DEFAULT_SHADOW_DARKNESS: f32 = 0.22;
const ENTITY_MARKERS: &[&str] = &[
    "[EditorBox]",
    "[Ground]",
    "[Player]",
    "[EditorCamera]",
    "[Sun]",
    "[Ball]",
    "[Colisionador]",
    "[ExecutionArea]",
];

// ── Manifest: nombres de campo = claves JSON en `src/shared-types/types.ts`. ─

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone)]
pub(crate) struct ProjectSaveData {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    r#type: String,
    pub gameStyle: String,
    #[serde(default)]
    activeSceneId: Option<u32>,
    #[serde(default)]
    world: Option<SavedWorldConfig>,
    #[serde(default)]
    #[allow(dead_code)]
    backgroundPath: Option<String>,
    #[serde(default)]
    entities: Vec<SavedEntity3D>,
    #[serde(default)]
    player: Option<SavedEntity3D>,
    #[serde(default)]
    config_camera: Option<SavedConfigCamera>,
    #[serde(default)]
    #[allow(dead_code)]
    config_editor_camera: Option<SavedConfigEditorCamera>,
    #[serde(default)]
    #[allow(dead_code)]
    sprites: Vec<NamedPath>,
    #[serde(default)]
    sounds: Vec<NamedPath>,
    #[serde(default)]
    fonts: Vec<NamedPath>,
    #[serde(default)]
    backgrounds: Vec<NamedPath>,
    #[serde(default)]
    pub scenes: Vec<SavedScene>,
    #[serde(default)]
    blueprints: Vec<SavedBlueprint>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    playerUiScreens: Vec<crate::ipc::SaveUiScreenSnapshot>,
    #[serde(default)]
    menuUiScreens: Vec<crate::ipc::SaveUiScreenSnapshot>,
    #[serde(default, rename = "playerUiTextBoxes")]
    player_ui_text_boxes: Vec<crate::ipc::SavePlayerUiTextBoxSnapshot>,
    #[serde(default, rename = "playerUiButtons")]
    player_ui_buttons: Vec<crate::ipc::SavePlayerUiButtonSnapshot>,
    #[serde(default, rename = "playerUiImages")]
    player_ui_images: Vec<crate::ipc::SavePlayerUiImageSnapshot>,
    #[serde(default, rename = "playerUiObjects")]
    player_ui_objects: Vec<crate::ipc::SavePlayerUiObjectSnapshot>,
    #[serde(default, rename = "hudImages")]
    hud_images: Vec<NamedPath>,
    #[serde(default)]
    resources: Option<SavedResources>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedResources {
    #[serde(default)]
    models: Vec<SavedResourceModel>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedResourceModel {
    id: String,
    name: String,
    #[serde(rename = "type")]
    model_type: String,
    asset: String,
    importer_version: u16,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedWorldConfig {
    worldWidth: f32,
    worldHeight: f32,
    #[serde(default)]
    worldDepth: Option<f32>,
    #[serde(default)]
    worldRadius: Option<f32>,
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
    #[serde(default)]
    graphicsTextureTier: Option<String>,
    #[serde(default)]
    textureDetailDistance: Option<f32>,
    #[serde(default)]
    reflectionTier: Option<String>,
    #[serde(default)]
    reflectionRaytracing: Option<bool>,
    #[serde(default)]
    reflectionProbes: Option<bool>,
    #[serde(default)]
    shadowTier: Option<String>,
}

#[derive(Debug, Default, Deserialize, Clone, Serialize)]
pub(crate) struct NamedPath {
    name: String,
    path: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    model_id: Option<String>,
    #[serde(default)]
    asset: Option<String>,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedConfigCamera {
    #[serde(default)]
    camera_eye_position: Option<[f32; 3]>,
    #[serde(default)]
    fps_camera_yaw: Option<f32>,
    #[serde(default)]
    fps_camera_pitch: Option<f32>,
    #[serde(default)]
    yaw: Option<f32>,
    #[serde(default)]
    pitch: Option<f32>,
    #[serde(default)]
    fov_y: Option<f32>,
    #[serde(default)]
    frustum_distance: Option<f32>,
    #[serde(default)]
    camera_follow_mode: Option<String>,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedConfigEditorCamera {
    position: [f32; 3],
    #[serde(default)]
    rotation: Option<[f32; 4]>,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedScene {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub world: SavedWorldConfig,
    #[serde(default)]
    pub backgroundPath: Option<String>,
    #[serde(default)]
    pub entities: Vec<SavedEntity3D>,
    #[serde(default)]
    pub player: Option<SavedEntity3D>,
    #[serde(default)]
    pub config_camera: Option<SavedConfigCamera>,
    #[serde(default)]
    pub config_editor_camera: Option<SavedConfigEditorCamera>,
    #[serde(default)]
    pub sprites: Vec<NamedPath>,
    #[serde(default)]
    pub models: Vec<NamedPath>,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedEntity3D {
    pub id: u32,
    pub name: String,
    pub category: String,
    pub model: String,
    #[serde(default)]
    pub model_id: Option<String>,
    pub position: [f32; 3],
    #[serde(default)]
    pub rotation: Option<[f32; 4]>,
    pub scale: [f32; 3],
    #[serde(default)]
    pub physics_type: Option<String>,
    #[serde(default = "default_colision_on")]
    pub colision: bool,
    #[serde(default)]
    pub animations: Option<Vec<SavedAnimation>>,
    #[serde(default)]
    pub scripts: Option<Vec<SavedScript>>,
    #[serde(default, alias = "control_bindings")]
    pub controls: Option<SavedControlBindings>,
    #[serde(default)]
    pub blueprint_id: Option<String>,
    #[serde(default)]
    pub texture_lod: Option<crate::ipc::SaveEntityTextureLodSnapshot>,
    #[serde(default, alias = "attachParentId")]
    pub attach_parent_id: Option<u32>,
    #[serde(default, alias = "attachLocalPosition")]
    pub attach_local_position: Option<[f32; 3]>,
    #[serde(default, alias = "attachLocalRotation")]
    pub attach_local_rotation: Option<[f32; 4]>,
    #[serde(default, alias = "attachLocalScale")]
    pub attach_local_scale: Option<[f32; 3]>,
    #[serde(default, alias = "attachSocketHostId")]
    pub attach_socket_host_id: Option<u32>,
    #[serde(default, alias = "attachSocketName")]
    pub attach_socket_name: Option<String>,
    #[serde(default)]
    pub sockets: Vec<crate::config_3d::entity_sockets::EntitySocketSnapshot>,
    #[serde(default)]
    pub bone_physics: Vec<crate::config_3d::bone_physics::BonePhysicsSnapshot>,
}

fn default_colision_on() -> bool {
    true
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedAnimation {
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
pub(crate) struct SavedScript {
    name: String,
    source: String,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedControlBindings {
    #[serde(default)]
    keyboard_mouse: HashMap<String, SavedScript>,
    #[serde(default)]
    gamepad: HashMap<String, SavedScript>,
}

/// `Blueprint3D` en types.ts.
#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone, Serialize)]
struct SavedBlueprint {
    id: String,
    name: String,
    category: String,
    model: String,
    #[serde(default)]
    model_id: Option<String>,
    #[serde(default)]
    physics_type: Option<String>,
    #[serde(default = "default_colision_on")]
    colision: bool,
    #[serde(default)]
    animations: Option<Vec<SavedAnimation>>,
    #[serde(default)]
    scripts: Option<Vec<SavedScript>>,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone)]
pub(crate) struct ActiveSaveView {
    pub world: SavedWorldConfig,
    pub entities: Vec<SavedEntity3D>,
    pub player: Option<SavedEntity3D>,
    pub config_camera: Option<SavedConfigCamera>,
    pub sceneId: u32,
    pub sceneName: String,
}

#[derive(Clone)]
struct PendingRestore {
    transform: EntityRestoreTransform,
    name: Option<String>,
    physics_enabled: bool,
    physics_type: String,
    colision: bool,
    animations: Option<Vec<SavedAnimation>>,
    scripts: Option<Vec<SavedScript>>,
    control_bindings: Option<ControlBindingsData>,
    blueprint_id: Option<String>,
    entity_category: Option<String>,
    visual_model_path: Option<String>,
    /// Id estable del manifest; debe coincidir al spawnear para restaurar fusiones.
    saved_entity_id: u32,
}

fn file_size_label(path: &Path) -> String {
    fs::metadata(path)
        .map(|m| format!("{} bytes", m.len()))
        .unwrap_or_else(|_| "tamaño desconocido".to_string())
}

fn log_load_manifest_summary(project: &ProjectSaveData, extract_dir: &Path) {
    let model_count = project.resources.as_ref().map(|r| r.models.len()).unwrap_or(0);
    log::info!(
        "[load-pack] v{} {} {} | {} modelos, {} escenas, {} entidades | {}",
        project.version,
        project.r#type,
        project.gameStyle,
        model_count,
        project.scenes.len(),
        project.entities.len(),
        extract_dir.display()
    );
    if let Some(resources) = &project.resources {
        for res in &resources.models {
            let disk = resolve_manifest_asset_path(&res.asset, extract_dir, &res.id);
            if disk.is_file() {
                log::info!(
                    "[load-pack]   «{}» {} → {} ({})",
                    res.name,
                    res.id,
                    disk.display(),
                    file_size_label(&disk)
                );
            } else {
                log::error!(
                    "[load-pack]   «{}» {} .rerasset FALTANTE: {}",
                    res.name,
                    res.id,
                    disk.display()
                );
            }
        }
    }
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
                log_load_manifest_summary(&project, &extract_dir);
                resolve_loaded_paths(&mut project, &extract_dir);
                let extract_path = path.clone();
                match apply_loaded_proyect_3d(self, &project) {
                    Ok(view) => {
                        send_project_loaded_3d(&project, &view, None);
                        self.editor_scenes_init_from_project(&project, &extract_path, &view);
                        self.sync_active_editor_scene_committed();
                        self.clear_editor_undo_redo();
                        self.emit_editor_scenes_updated("project_loaded");
                        send_project_load_3d_complete_event();
                        log::info!("[load-pack] carga .save completada");
                    }
                    Err(err) => {
                        log::error!("[load-pack] error al aplicar proyecto: {err}");
                    }
                }
            }
            Err(err) => log::error!("[load-pack] error al abrir '{path}': {err}"),
        }
    }
}

pub(crate) fn load_project_from_extract_dir(extract_path: &str) -> Result<ProjectSaveData, String> {
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

fn is_editor_box_path(p: &str) -> bool {
    entity_path_marker(p) == Some("[EditorBox]")
}

fn is_ball_path(p: &str) -> bool {
    entity_path_marker(p) == Some("[Ball]")
}

fn is_collider_path(p: &str) -> bool {
    entity_path_marker(p) == Some("[Colisionador]")
}

fn is_execution_area_path(p: &str) -> bool {
    entity_path_marker(p) == Some("[ExecutionArea]")
}

fn is_model_3d_path(p: &str) -> bool {
    let lower = p.to_ascii_lowercase();
    lower.ends_with(".glb") || lower.ends_with(".gltf") || lower.ends_with(".fbx")
}

fn is_3d_model_file_entity(entity: &SavedEntity3D) -> bool {
    if entity
        .model_id
        .as_deref()
        .is_some_and(|id| !id.is_empty())
    {
        return entity_path_marker(&entity.model).is_none()
            && !matches!(entity.category.as_str(), "sun" | "ground" | "player");
    }
    if !is_model_3d_path(&entity.model) {
        return false;
    }
    entity_path_marker(&entity.model).is_none()
        && !matches!(entity.category.as_str(), "sun" | "ground" | "player")
}

/// Clave GPU (`model_id`) para spawn / caché al abrir `.save`.
fn entity_model_cache_lookup(state: &State, entity: &SavedEntity3D) -> Result<String, String> {
    if let Some(id) = entity.model_id.as_deref().filter(|s| !s.is_empty()) {
        if state
            .imported_model_registry
            .get(id)
            .is_some_and(|e| e.state == AssetState::Ready)
        {
            return Ok(id.to_string());
        }
    }
    if let Some(id) = state.imported_model_registry.model_id_for_path(&entity.model) {
        if state
            .imported_model_registry
            .get(&id)
            .is_some_and(|e| e.state == AssetState::Ready)
        {
            return Ok(id);
        }
    }
    let basename = path_basename_lower(&entity.model);
    for entry in state.imported_model_registry.iter() {
        if entry.state != AssetState::Ready {
            continue;
        }
        if path_basename_lower(&entry.source_path) == basename {
            return Ok(entry.model_id.clone());
        }
    }
    Err(format!(
        "Sin asset importado para «{}» (requiere model_id + .rerasset en resources.models)",
        entity.model
    ))
}

fn entity_library_category(category: &str) -> Option<String> {
    match category {
        "environment" | "object" | "character" | "weapon" | "projectile" => Some(category.to_string()),
        _ => None,
    }
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

/// `model_*` del manifest v2 no es una ruta en disco bajo extract_dir.
fn is_imported_model_id(p: &str) -> bool {
    p.starts_with("model_")
}

/// Resuelve paths de assets en el `.save`; preserva marcadores y `model_id`.
fn resolve_asset_path(p: &str, extracted_dir: &Path) -> String {
    if p.is_empty() {
        return p.to_string();
    }
    if entity_path_marker(p).is_some() || is_imported_model_id(p) {
        return p.to_string();
    }
    resolve_path(p, extracted_dir)
}

/// Misma referencia de asset importado (model_id o alias en registro).
fn models_refer_to_same_asset(state: &State, a: &str, b: &str) -> bool {
    if paths_match_for_burst(a, b) {
        return true;
    }
    state.model_cache_key(a) == state.model_cache_key(b)
}

fn source_path_for_imported_model(
    project: &ProjectSaveData,
    model_id: &str,
    extract_dir: &Path,
) -> String {
    for scene in &project.scenes {
        for entity in scene
            .entities
            .iter()
            .chain(scene.player.iter())
        {
            if entity.model_id.as_deref() == Some(model_id) && !entity.model.is_empty() {
                if is_imported_model_id(&entity.model) {
                    continue;
                }
                if is_model_3d_path(&entity.model) || entity.model.contains('/') {
                    return resolve_asset_path(&entity.model, extract_dir);
                }
            }
        }
    }
    for entity in project.entities.iter().chain(project.player.iter()) {
        if entity.model_id.as_deref() == Some(model_id) && !entity.model.is_empty() {
            if is_imported_model_id(&entity.model) {
                continue;
            }
            if is_model_3d_path(&entity.model) || entity.model.contains('/') {
                return resolve_asset_path(&entity.model, extract_dir);
            }
        }
    }
    model_id.to_string()
}

fn resolve_path(p: &str, extracted_dir: &Path) -> String {
    if p.is_empty() {
        return p.to_string();
    }
    if entity_path_marker(p).is_some() {
        return entity_path_marker(p).unwrap().to_string();
    }
    if Path::new(p).is_absolute() {
        log::error!("[load] manifest path must be relative to .save extract dir: {p}");
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
    // Rhai embebido en manifest (sin referencia @file:scripting/...).
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

fn resolve_entity_3d(entity: &SavedEntity3D, extracted_dir: &Path) -> SavedEntity3D {
    SavedEntity3D {
        model: resolve_asset_path(&entity.model, extracted_dir),
        scripts: resolve_scripts(&entity.scripts, extracted_dir),
        controls: resolve_control_bindings(&entity.controls, extracted_dir),
        ..entity.clone()
    }
}

fn resolve_player_entity(
    player: &Option<SavedEntity3D>,
    extracted_dir: &Path,
) -> Option<SavedEntity3D> {
    player
        .as_ref()
        .map(|p| resolve_entity_3d(p, extracted_dir))
}

fn resolve_loaded_paths(project: &mut ProjectSaveData, extracted_dir: &Path) {
    let has_scenes = !project.scenes.is_empty();

    project.sounds = project
        .sounds
        .iter()
        .map(|s| NamedPath {
            name: s.name.clone(),
            path: resolve_path(&s.path, extracted_dir),
            category: None,
            model_id: None,
            asset: None,
        })
        .collect();
    project.fonts = project
        .fonts
        .iter()
        .map(|f| NamedPath {
            name: f.name.clone(),
            path: resolve_path(&f.path, extracted_dir),
            category: None,
            model_id: None,
            asset: None,
        })
        .collect();
    project.backgrounds = project
        .backgrounds
        .iter()
        .map(|b| NamedPath {
            name: b.name.clone(),
            path: resolve_path(&b.path, extracted_dir),
            category: None,
            model_id: None,
            asset: None,
        })
        .collect();
    project.hud_images = project
        .hud_images
        .iter()
        .map(|h| NamedPath {
            name: h.name.clone(),
            path: resolve_path(&h.path, extracted_dir),
            category: None,
            model_id: None,
            asset: None,
        })
        .collect();
    project.player_ui_text_boxes = project
        .player_ui_text_boxes
        .iter()
        .map(|b| crate::ipc::SavePlayerUiTextBoxSnapshot {
            font_path: resolve_path(&b.font_path, extracted_dir),
            ..b.clone()
        })
        .collect();
    project.player_ui_buttons = project
        .player_ui_buttons
        .iter()
        .map(|b| crate::ipc::SavePlayerUiButtonSnapshot {
            font_path: resolve_path(&b.font_path, extracted_dir),
            texture_path: resolve_optional_path(&b.texture_path, extracted_dir),
            ..b.clone()
        })
        .collect();
    project.player_ui_images = project
        .player_ui_images
        .iter()
        .map(|img| crate::ipc::SavePlayerUiImageSnapshot {
            image_path: resolve_path(&img.image_path, extracted_dir),
            ..img.clone()
        })
        .collect();
    project.player_ui_objects = project
        .player_ui_objects
        .iter()
        .map(|obj| crate::ipc::SavePlayerUiObjectSnapshot {
            texture_path: resolve_optional_path(&obj.texture_path, extracted_dir),
            ..obj.clone()
        })
        .collect();

    if !has_scenes {
        project.player = resolve_player_entity(&project.player, extracted_dir);
        project.entities = project
            .entities
            .iter()
            .map(|e| resolve_entity_3d(e, extracted_dir))
            .collect();
    } else {
        project.entities.clear();
        project.player = None;
        project.config_camera = None;
        project.config_editor_camera = None;
        project.backgroundPath = None;
        project.sprites.clear();
    }

    project.scenes = project
        .scenes
        .iter()
        .map(|scene| SavedScene {
            backgroundPath: resolve_optional_path(&scene.backgroundPath, extracted_dir),
            models: Vec::new(),
            entities: scene
                .entities
                .iter()
                .map(|e| resolve_entity_3d(e, extracted_dir))
                .collect(),
            player: resolve_player_entity(&scene.player, extracted_dir),
            ..scene.clone()
        })
        .collect();

    project.blueprints = project
        .blueprints
        .iter()
        .map(|bp| SavedBlueprint {
            model: resolve_asset_path(&bp.model, extracted_dir),
            scripts: resolve_scripts(&bp.scripts, extracted_dir),
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
            player: active.player.clone(),
            config_camera: active.config_camera.clone(),
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
        player: project.player.clone(),
        config_camera: project.config_camera.clone(),
        sceneId: 1,
        sceneName: String::new(),
    })
}

fn needs_scene_burst_load(_game_style: &str, view: &ActiveSaveView) -> bool {
    !view.entities.is_empty() || view.player.is_some()
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

fn resolve_saved_entity_transform(entity: &SavedEntity3D) -> EntityRestoreTransform {
    EntityRestoreTransform {
        position: entity.position,
        rotation: entity.rotation.unwrap_or([0.0, 0.0, 0.0, 1.0]),
        scale: entity.scale,
    }
}

/// Herencia de blueprint: model/physics/scripts/animations/colision; transform solo de la instancia.
fn build_generic_pending_restore(
    entity: &SavedEntity3D,
    transform: EntityRestoreTransform,
    blueprints: &[SavedBlueprint],
) -> PendingRestore {
    let bp = entity
        .blueprint_id
        .as_deref()
        .and_then(|id| find_blueprint(id, blueprints));
    let physics_type = bp
        .and_then(|b| b.physics_type.clone())
        .or_else(|| entity.physics_type.clone())
        .unwrap_or_else(|| "static".to_string());
    let visual = {
        let marker_model = bp
            .map(|b| b.model.as_str())
            .unwrap_or(entity.model.as_str());
        let marker_model_id = bp
            .and_then(|b| b.model_id.as_deref())
            .filter(|s| !s.is_empty());
        if entity_path_marker(marker_model).is_some() || marker_model == "[Player]" {
            None
        } else if let Some(id) = entity.model_id.as_ref().filter(|s| !s.is_empty()) {
            Some(id.clone())
        } else if let Some(id) = marker_model_id {
            Some(id.to_string())
        } else if is_imported_model_id(&entity.model) {
            Some(entity.model.clone())
        } else if is_imported_model_id(marker_model) {
            Some(marker_model.to_string())
        } else {
            Some(marker_model.to_string())
        }
    };
    PendingRestore {
        transform,
        name: Some(entity.name.clone()),
        physics_enabled: bp
            .and_then(|b| b.physics_type.as_ref())
            .or(entity.physics_type.as_ref())
            .is_some(),
        physics_type,
        colision: bp.map(|b| b.colision).unwrap_or(entity.colision),
        animations: bp
            .and_then(|b| b.animations.clone())
            .or_else(|| entity.animations.clone()),
        scripts: bp
            .and_then(|b| b.scripts.clone())
            .or_else(|| entity.scripts.clone()),
        control_bindings: map_control_bindings(
            entity.controls.as_ref(),
        ),
        blueprint_id: entity.blueprint_id.clone(),
        entity_category: entity_library_category(&entity.category),
        visual_model_path: visual,
        saved_entity_id: entity.id,
    }
}

fn build_player_pending_restore_from_entity(entity: &SavedEntity3D) -> PendingRestore {
    let visual = if entity_path_marker(&entity.model).is_some() || entity.model == "[Player]" {
        None
    } else if let Some(id) = entity.model_id.as_ref().filter(|s| !s.is_empty()) {
        Some(id.clone())
    } else if !entity.model.is_empty() {
        Some(entity.model.clone())
    } else {
        None
    };
    PendingRestore {
        transform: EntityRestoreTransform {
            position: entity.position,
            rotation: entity.rotation.unwrap_or([0.0, 0.0, 0.0, 1.0]),
            scale: entity.scale,
        },
        name: Some(entity.name.clone()),
        physics_enabled: entity.physics_type.is_some(),
        physics_type: entity
            .physics_type
            .clone()
            .unwrap_or_else(|| "dynamic".to_string()),
        colision: entity.colision,
        animations: entity.animations.clone(),
        scripts: entity.scripts.clone(),
        control_bindings: map_control_bindings(entity.controls.as_ref()),
        blueprint_id: entity.blueprint_id.clone(),
        entity_category: None,
        visual_model_path: visual,
        saved_entity_id: entity.id,
    }
}

fn apply_entity_scripts(state: &mut State, id: u32, scripts: Option<&[SavedScript]>) {
    let Some(list) = scripts else { return };
    for script in list {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::LoadScript {
            id,
            path: script.name.clone(),
            source: script.source.clone(),
        }));
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
                pivot_x: Some(f.pivot_x),
                pivot_y: Some(f.pivot_y),
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
        state.handle_command(EngineCommand::Common(EngineCommandCommon::SetAnimation {
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
        }));
        if anim.is_default.unwrap_or(false) {
            state.handle_command(EngineCommand::Common(EngineCommandCommon::SetDefaultAnimation {
                id,
                name: anim.name.clone(),
            }));
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

    apply_entity_scripts(state, id, pending.scripts.as_deref());
    apply_entity_animations(state, id, pending.animations.as_deref());

    if let Some(visual) = pending
        .visual_model_path
        .as_ref()
        .filter(|p| !p.is_empty() && !models_refer_to_same_asset(state, p, model_path))
    {
        state.replace_entity_model(id, visual);
    }

    state.entity_colision.insert(id, pending.colision);
    state.reconcile_entity_physics_with_mesh(id);
}

fn ensure_model_cached(state: &mut State, path: &str) -> bool {
    state.poll_and_advance_model_preloads(MODEL_GPU_PARTS_DURING_SAVE_LOAD);
    match state.ensure_static_model_cached(path) {
        Ok(()) => true,
        Err(err) => {
            log::error!("no se pudo cachear modelo '{path}': {err}");
            false
        }
    }
}

/// Modelos 3D que la escena instancia al abrir el `.save` (vía `resources.models`).
fn collect_scene_required_model_ids(
    state: &State,
    view: &ActiveSaveView,
) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut push = |entity: &SavedEntity3D| {
        if !is_3d_model_file_entity(entity) {
            return;
        }
        if let Ok(id) = entity_model_cache_lookup(state, entity) {
            if !ids.iter().any(|existing| existing == &id) {
                ids.push(id);
            }
        }
    };
    for entity in &view.entities {
        push(entity);
    }
    if let Some(player) = view.player.as_ref() {
        push(player);
    }
    ids
}

fn log_load_step(total: Instant, step: &mut Instant, message: &str) {
    let step_ms = step.elapsed().as_millis() as u64;
    let total_ms = total.elapsed().as_millis() as u64;
    
    send_load_progress(message, Some(step_ms), Some(total_ms));
    *step = Instant::now();
}

fn load_project_asset_stores(state: &mut State, project: &ProjectSaveData) {
    for sound in &project.sounds {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::LoadSound {
            path: sound.path.clone(),
            name: sound.name.clone(),
        }));
    }
    for font in &project.fonts {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::LoadFont {
            path: font.path.clone(),
            name: font.name.clone(),
        }));
    }
    for bg in &project.backgrounds {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::LoadBackgroundAsset {
            path: bg.path.clone(),
            name: bg.name.clone(),
        }));
    }
    for img in &project.hud_images {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::LoadHudImage {
            path: img.path.clone(),
            name: img.name.clone(),
        }));
    }
}

/// Restaura jugador 3D desde manifest: mesh sin auto-escala, transform y cámara del save.
fn restore_player_from_manifest(
    state: &mut State,
    player: &SavedEntity3D,
    cam: &SavedConfigCamera,
) {
    use crate::config_3d::character_anchor::{
        PLAY_CHARACTER_EDITOR_ORBIT_PITCH, PLAY_CHARACTER_EDITOR_ORBIT_YAW,
    };

    let pending = build_player_pending_restore_from_entity(player);
    let cache_key = entity_model_cache_lookup(state, player).ok();

    if let Some(ref id) = cache_key {
        if !ensure_model_cached(state, id) {
            log::warn!("[restore] no se pudo precargar modelo jugador: {id}");
        }
        state.poll_and_advance_model_preloads(MODEL_GPU_PARTS_DURING_SAVE_LOAD);
    }

    let id = state.ensure_play_character_shell(
        &player.name,
        "[Player]",
        cache_key.as_deref(),
        Some(player.id),
    );

    if let Some(ref model_id) = cache_key {
        match state.install_play_character_visual_from_path(id, model_id) {
            Ok(asset_key) => {
                
                state.emit_entity_model_replaced_for_play_character(id, &asset_key);
            }
            Err(e) => log::error!("[restore] mesh jugador: {e}"),
        }
    }

    let feet = player_manifest_position_as_feet(state, player);
    let feet_v = glam::Vec3::from_array(feet);
    let rot_q = player
        .rotation
        .map(|r| glam::Quat::from_xyzw(r[0], r[1], r[2], r[3]));
    let scale = glam::Vec3::from_array(player.scale);
    state.apply_play_character_transform_editor(id, None, rot_q, Some(scale));
    if let Some(ref model_id) = cache_key {
        if let Some(bounds) = state.play_character_visual_local_bounds(model_id) {
            state.place_play_character_at_world_feet_with_bounds(id, bounds, feet_v, false);
        } else {
            state.place_play_character_at_world_feet(id, model_id, feet_v, false);
        }
    } else {
        state.set_play_character_feet_position(feet_v);
    }

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
        true,
        true,
    );
    apply_entity_scripts(state, id, pending.scripts.as_deref());
    apply_entity_animations(state, id, pending.animations.as_deref());
    state.entity_colision.insert(id, pending.colision);
    state.ensure_play_character_kinematic_only();
    state.reconcile_entity_physics_with_mesh(id);

    if !state.model_animation_bindings.contains_key(&id) {
        log::warn!(
            "[SHADER_MAT] ent={id} jugador sin animation binding tras restore — personaje puede verse sin texturas correctas"
        );
    }

    let follow_mode = cam.camera_follow_mode.as_deref().map(|m| match m {
        "follow_character" => crate::ipc::PlayCameraFollowMode::FollowCharacter,
        _ => crate::ipc::PlayCameraFollowMode::MoveWithCharacter,
    });
    let yaw = cam.yaw.unwrap_or(PLAY_CHARACTER_EDITOR_ORBIT_YAW);
    let pitch = cam.pitch.unwrap_or(PLAY_CHARACTER_EDITOR_ORBIT_PITCH);
    state.apply_play_character_view(
        feet,
        yaw,
        pitch,
        cam.fov_y,
        cam.frustum_distance,
        follow_mode,
        player.rotation,
        Some(player.scale),
        cam.camera_eye_position,
        cam.fps_camera_yaw,
        cam.fps_camera_pitch,
    );
    if let Some(eye) = cam.camera_eye_position {
        state.play_camera_eye_position = glam::Vec3::from_array(eye);
        state.capture_play_camera_follow_offset();
    }
    if let Some(rot) = player.rotation {
        let rot_q = glam::Quat::from_xyzw(rot[0], rot[1], rot[2], rot[3]);
        state.apply_play_character_transform_editor(id, None, Some(rot_q), None);
        if state.uses_editor_viewport_camera() {
            state.ensure_editor_camera_entity();
            state.sync_editor_camera_entity_from_viewport();
        }
    }

    let char_path = cache_key
        .as_deref()
        .map(|id| state.model_library_path_for(id))
        .unwrap_or_else(|| "[Player]".to_string());
    send_event(&EngineEvent::CharacterLoaded {
        id,
        path: char_path,
    });
    state.emit_play_character_view_changed(true);
}

/// `player.position` en el manifest = pies en mundo (`build_player_snapshot`).
fn player_manifest_position_as_feet(_state: &State, player: &SavedEntity3D) -> [f32; 3] {
    player.position
}

pub(crate) fn apply_loaded_proyect_3d(
    state: &mut State,
    project: &ProjectSaveData,
) -> Result<ActiveSaveView, String> {
    apply_loaded_proyect_3d_with_scene(state, project, None)
}

pub(crate) fn apply_editor_scene_switch(
    state: &mut State,
    project: &ProjectSaveData,
    scene: &SavedScene,
) -> Result<ActiveSaveView, String> {
    apply_loaded_proyect_3d_with_scene(state, project, Some(scene))
}

fn apply_loaded_proyect_3d_with_scene(
    state: &mut State,
    project: &ProjectSaveData,
    forced_scene: Option<&SavedScene>,
) -> Result<ActiveSaveView, String> {
    let load_started_at = Instant::now();
    let mut step_started = Instant::now();
    state.restoring_save_manifest = true;
    let view = match forced_scene {
        Some(scene) => active_view_from_saved_scene(scene),
        None => match pick_active_save_view(project) {
            Ok(v) => v,
            Err(e) => {
                state.restoring_save_manifest = false;
                return Err(e);
            }
        },
    };
    let open_msg = format!(
        "Abriendo escena «{}» ({} entidades)…",
        view.sceneName,
        view.entities.len()
    );
    
    send_load_progress(&open_msg, None, None);
    if forced_scene.is_none() && state.mount_save_on_empty_world {
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
    let saved_player = view.player.clone();
    let saved_config_camera = view.config_camera.clone();

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
    let radius = view
        .world
        .worldRadius
        .unwrap_or_else(|| view.world.worldWidth.min(view.world.worldHeight).min(depth) * 0.5);
    state.handle_command(EngineCommand::Only3d(EngineCommand3dOnly::SetWorldRadius { radius }));
    state.handle_command(EngineCommand::Common(EngineCommandCommon::SetGridVisible {
        visible: view.world.gridVisible,
    }));
    state.handle_command(EngineCommand::Common(EngineCommandCommon::SetGridCellSize {
        size: view.world.gridCellSize,
    }));
    let target_fps = if view.world.targetFps.is_finite() && view.world.targetFps > 0.0 {
        view.world.targetFps as u64
    } else {
        60
    };
    state.handle_command(EngineCommand::Common(EngineCommandCommon::SetTargetFps { fps: target_fps }));
    if let Some(gravity) = view.world.gravity {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::SetGravity { gravity }));
    }
    state.handle_command(EngineCommand::Only3d(EngineCommand3dOnly::SetDirectionalLight {
        ambient: Some(light_ambient),
        intensity: Some(light_intensity),
        shadow_darkness: Some(shadow_darkness),
    }));
    crate::config_3d::entity_textures::apply_graphics_settings_from_world_wire(
        state,
        view.world.graphicsTextureTier.as_deref(),
        view.world.textureDetailDistance,
    );
    crate::config_3d::reflection_settings::apply_reflection_settings_from_world_wire(
        state,
        view.world.reflectionTier.as_deref(),
        view.world.reflectionRaytracing,
        view.world.reflectionProbes,
    );
    crate::config_3d::shadow_settings::apply_shadow_settings_from_world_wire(
        state,
        view.world.shadowTier.as_deref(),
    );
    state.ensure_material_validation_demo();
    send_event(&EngineEvent::GraphicsTextureTierChanged {
        tier: state.graphics_texture_tier.wire().to_string(),
    });
    send_event(&EngineEvent::TextureDetailDistanceChanged {
        distance_m: state.texture_detail_near_m,
    });
    send_event(&EngineEvent::ReflectionTierChanged {
        tier: state.reflection_tier.wire().to_string(),
    });
    send_event(&EngineEvent::ReflectionProbesChanged {
        enabled: state.reflection_probes_enabled,
    });
    send_event(&EngineEvent::ShadowTierChanged {
        tier: state.shadow_tier.wire().to_string(),
    });
    send_event(&EngineEvent::TaaChanged {
        enabled: state.taa.enabled,
        blend: Some(state.taa.blend),
        jitter_scale: Some(state.taa.jitter_scale),
    });

    load_project_asset_stores(state, project);
    log_load_step(load_started_at, &mut step_started, "Sonidos y fondos registrados");
    if let Some(resources) = &project.resources {
        let extract_dir = std::env::var("RER_PROJECT_EXTRACT_DIR").unwrap_or_default();
        let extract_path = std::path::Path::new(extract_dir.as_str());
        for res in &resources.models {
            let asset_disk =
                resolve_manifest_asset_path(&res.asset, extract_path, &res.id);
            if !asset_disk.is_file() {
                log::error!(
                    "[load] .rerasset no encontrado: {} (model_id={})",
                    asset_disk.display(),
                    res.id
                );
                send_event(&EngineEvent::Error {
                    message: format!(
                        "Modelo importado faltante en .save: {} ({})",
                        res.name,
                        asset_disk.display()
                    ),
                });
                continue;
            }
            log::info!(
                "[load-pack] GPU «{}» desde {}",
                res.name,
                asset_disk.display()
            );
            let source_path = source_path_for_imported_model(project, &res.id, extract_path);
            state.cache_rerasset_material_tex_map(&res.id, &asset_disk);
            state.imported_model_registry.insert(
                crate::assets::ImportedModelEntry {
                    model_id: res.id.clone(),
                    name: res.name.clone(),
                    category: crate::ipc::normalize_model_library_category(Some(
                        res.model_type.as_str(),
                    )),
                    state: rer_engine_shared::assets::AssetState::Ready,
                    rerasset_path: asset_disk.clone(),
                    source_path: source_path.clone(),
                    source_size: 0,
                    source_mtime_secs: 0,
                    importer_version: res.importer_version,
                },
            );
            state
                .imported_model_registry
                .link_imported_model_aliases(&res.id, &source_path);
            let asset_rel = relative_rerasset_manifest_path(&res.id);
            let key = if is_imported_model_id(&source_path) {
                source_path.clone()
            } else {
                state.model_path_key(&source_path)
            };
            state.model_store.insert(
                key,
                crate::ipc::ModelStoreEntry {
                    name: res.name.clone(),
                    category: crate::ipc::normalize_model_library_category(Some(
                        res.model_type.as_str(),
                    )),
                    model_id: Some(res.id.clone()),
                    rerasset_path: Some(asset_rel),
                },
            );
        }
        for entity in view
            .entities
            .iter()
            .chain(view.player.iter())
        {
            if let Ok(model_id) = entity_model_cache_lookup(state, entity) {
                
                state
                    .imported_model_registry
                    .link_imported_model_aliases(&model_id, &entity.model);
            }
        }
    } else if !collect_scene_required_model_ids(state, &view).is_empty() {
        state.restoring_save_manifest = false;
        return Err(
            "Manifest sin resources.models pero la escena referencia modelos 3D importados."
                .to_string(),
        );
    }
    let burst_load_planned = burst_load;
    log_load_step(
        load_started_at,
        &mut step_started,
        "Modelos importados: carga lazy al instanciar entidades",
    );

    let mut model_load_queue: Vec<(String, PendingRestore)> = Vec::new();

    for entity in &view.entities {
        state.poll_and_advance_model_preloads(MODEL_GPU_PARTS_DURING_SAVE_LOAD);
        let transform = resolve_saved_entity_transform(entity);
        let pending = build_generic_pending_restore(entity, transform, blueprints);

        match entity.category.as_str() {
            "player" => {
                log::warn!(
                    "manifest: entidad player en entities ignorada; usar objeto player del manifest"
                );
            }
            _ if is_collider_path(&entity.model) => {
                
                if let Some(id) = state.restore_collider_plane_from_save(
                    &entity.name,
                    entity.position,
                    entity.scale,
                    entity.rotation,
                    None,
                ) {
                    apply_full_entity_restore(state, id, &pending, "[Colisionador]", false, false);
                }
            }
            _ if is_execution_area_path(&entity.model) => {
                
                if let Some(id) = state.restore_trigger_plane_from_save(
                    &entity.name,
                    entity.position,
                    entity.scale,
                    entity.rotation,
                    None,
                ) {
                    apply_full_entity_restore(state, id, &pending, "[ExecutionArea]", false, false);
                }
            }
            _ if is_editor_box_path(&entity.model) => {
                
                state.spawn_editor_box(&entity.name, entity.position, entity.scale);
                if let Some(id) = state.scenario_entities.last().copied() {
                    apply_full_entity_restore(state, id, &pending, "[EditorBox]", true, false);
                }
            }
            "sun" => {
                state.spawn_sun(&entity.name, entity.position, entity.scale);
                if let Some(id) = state.sun_entity {
                    apply_full_entity_restore(state, id, &pending, "[Sun]", true, false);
                }
            }
            "ground" => {
                state.spawn_ground_plane(entity.position, entity.scale);
                if let Some(id) = state.ground_entity_id() {
                    apply_full_entity_restore(state, id, &pending, "[Ground]", true, false);
                }
            }
            _ if is_ball_path(&entity.model) => {
                let diameter = entity
                    .scale
                    .into_iter()
                    .fold(f32::INFINITY, f32::min)
                    .max(0.15);
                let physics_type = entity
                    .physics_type
                    .as_deref()
                    .unwrap_or("dynamic");
                let id = state.spawn_physics_ball(
                    &entity.name,
                    entity.position,
                    [diameter, diameter, diameter],
                    physics_type,
                );
                apply_full_entity_restore(state, id, &pending, "[Ball]", true, false);
            }
            "environment" | "object" | "character" | "weapon" | "projectile"
                if is_3d_model_file_entity(entity) => {
                let model_key = match entity_model_cache_lookup(state, entity) {
                    Ok(id) => id,
                    Err(e) => {
                        log::error!("{e}");
                        continue;
                    }
                };
                model_load_queue.push((model_key.clone(), pending.clone()));

                if !burst_load_planned {
                    if !ensure_model_cached(state, &model_key) {
                        continue;
                    }
                    if let Ok(id) = state.spawn_cached_model_from_save(
                        &model_key,
                        pending.transform.position,
                        pending.transform.rotation,
                        pending.transform.scale,
                        pending.name.as_deref(),
                        pending.entity_category.clone(),
                        pending.blueprint_id.clone(),
                        pending.physics_enabled,
                        &pending.physics_type,
                        Some(pending.saved_entity_id),
                    ) {
                        apply_full_entity_restore(state, id, &pending, &model_key, false, false);
                    }
                }
            }
            other => {
                log::warn!(
                    "entidad category='{other}' model='{}' ignorada en carga 3D",
                    entity.model
                );
            }
        }
    }

    log_load_step(
        load_started_at,
        &mut step_started,
        &format!("Entidades del manifest procesadas ({})", view.entities.len()),
    );

    if burst_load_planned {
        
        state.poll_and_advance_model_preloads(MODEL_GPU_PARTS_DURING_SAVE_LOAD);

        let queued_ids: Vec<String> = model_load_queue.iter().map(|(p, _)| p.clone()).collect();
        for model_id in &queued_ids {
            if !ensure_model_cached(state, model_id) {
                log::error!("no se pudo cachear modelo '{model_id}' para burst load");
            }
        }

        for (path, pending) in model_load_queue {
            state.poll_and_advance_model_preloads(MODEL_GPU_PARTS_DURING_SAVE_LOAD);
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
                Some(pending.saved_entity_id),
            ) {
                apply_full_entity_restore(state, id, &pending, &path, false, false);
            }
        }
    }

    match (saved_player.as_ref(), saved_config_camera.as_ref()) {
        (Some(player_entity), Some(cam)) => {
            restore_player_from_manifest(state, player_entity, cam);
        }
        (Some(_), None) => {
            log::error!("[load_proyect] proyecto 3D requiere config_camera en manifest");
        }
        (None, _) => {
            if view.entities.is_empty() {
                state.apply_3d_placeholder_sun_and_player();
            } else {
                log::error!("[load_proyect] proyecto 3D requiere player en manifest");
            }
        }
    }

    if burst_load_planned {
        log_load_step(load_started_at, &mut step_started, "Modelos 3D instanciados");
    }

    if let (Some(player_entity), Some(id)) = (saved_player.as_ref(), state.play_character_entity) {
        if let Some(bindings) = map_control_bindings(player_entity.controls.as_ref()) {
            state.handle_command(crate::ipc::EngineCommand::Common(EngineCommandCommon::SetControlBindings { id, bindings }));
        }
    }

    for id in state.world.entities().to_vec() {
        state.reconcile_entity_physics_with_mesh(id);
    }

    let done_msg = format!(
        "Carga terminada — escena «{}» ({} entidades, {} ms)",
        view.sceneName,
        view.entities.len(),
        load_started_at.elapsed().as_millis()
    );
    log::info!("[load-pack] {done_msg}");
    send_load_progress(
        &done_msg,
        None,
        Some(load_started_at.elapsed().as_millis() as u64),
    );
    state.import_player_ui_text_boxes_from_save(&project.player_ui_text_boxes);
    state.import_player_ui_buttons_from_save(&project.player_ui_buttons);
    state.import_player_ui_images_from_save(&project.player_ui_images);
    state.import_player_ui_objects_from_save(&project.player_ui_objects);
    state.ensure_default_3d_player_ui();
    let player_screens: Vec<crate::ipc::PlayerUiScreenInfo> = project
        .playerUiScreens
        .iter()
        .map(|s| crate::ipc::PlayerUiScreenInfo {
            id: s.id.clone(),
            name: s.name.clone(),
            active: s.active,
        })
        .collect();
    state.sync_player_ui_screens(&player_screens);
    if let Some(scene) = forced_scene {
        if is_3d_placeholder_saved_scene(scene) {
            state.finalize_3d_placeholder_editor_scene();
        }
    }
    restore_entity_sockets_after_scene_load(state, &view);
    restore_entity_bone_physics_after_scene_load(state, &view);
    restore_entity_attachments_after_scene_load(state, &view);
    state.sanitize_reflection_probe_entities();
    state.restoring_save_manifest = false;
    Ok(view)
}

fn collect_saved_entity_attachments(
    view: &ActiveSaveView,
) -> Vec<crate::config_3d::entity_attachments::SavedEntityAttachment> {
    let mut out = Vec::new();
    let mut push = |entity: &SavedEntity3D| {
        let (Some(local_position), Some(local_rotation), Some(child_world_scale)) = (
            entity.attach_local_position,
            entity.attach_local_rotation,
            entity.attach_local_scale,
        ) else {
            return;
        };

        let is_socket = entity.attach_socket_host_id.is_some()
            && entity
                .attach_socket_name
                .as_ref()
                .is_some_and(|n| !n.trim().is_empty());
        let is_entity = entity.attach_parent_id.is_some();

        if !is_socket && !is_entity {
            return;
        }

        out.push(crate::config_3d::entity_attachments::SavedEntityAttachment {
            entity_id: entity.id,
            parent_id: if is_socket {
                None
            } else {
                entity.attach_parent_id
            },
            attach_socket_host_id: if is_socket {
                entity.attach_socket_host_id
            } else {
                None
            },
            attach_socket_name: if is_socket {
                entity.attach_socket_name.clone()
            } else {
                None
            },
            local_position,
            local_rotation,
            child_world_scale,
        });
    };
    for entity in &view.entities {
        push(entity);
    }
    if let Some(player) = &view.player {
        push(player);
    }
    out
}

fn restore_entity_sockets_after_scene_load(state: &mut State, view: &ActiveSaveView) {
    let mut restore = |entity: &SavedEntity3D| {
        if !entity.sockets.is_empty() {
            state.restore_entity_sockets_from_saved(entity.id, &entity.sockets);
            state.emit_entity_sockets_if_any(entity.id);
        }
    };
    for entity in &view.entities {
        restore(entity);
    }
    if let Some(player) = &view.player {
        restore(player);
    }
}

fn restore_entity_bone_physics_after_scene_load(state: &mut State, view: &ActiveSaveView) {
    let mut restore = |entity: &SavedEntity3D| {
        if !entity.bone_physics.is_empty() {
            state.restore_entity_bone_physics_from_saved(entity.id, &entity.bone_physics);
            state.emit_entity_bone_physics_if_any(entity.id);
        }
    };
    for entity in &view.entities {
        restore(entity);
    }
    if let Some(player) = &view.player {
        restore(player);
    }
}

fn restore_entity_attachments_after_scene_load(state: &mut State, view: &ActiveSaveView) {
    let saved = collect_saved_entity_attachments(view);
    if !saved.is_empty() {
        state.restore_entity_attachments_from_saved(&saved);
    }
}

fn is_3d_placeholder_saved_scene(scene: &SavedScene) -> bool {
    scene.player.is_some()
        && scene.models.is_empty()
        && scene
            .entities
            .iter()
            .any(|e| e.category == "sun")
        && scene
            .entities
            .iter()
            .any(|e| e.category == "ground")
}

/// Lista de biblioteca para el editor desde `resources.models` (`path` = `model_id`).
fn editor_models_from_manifest(project: &ProjectSaveData) -> Vec<ImportSceneSprite> {
    project
        .resources
        .as_ref()
        .map(|resources| {
            resources
                .models
                .iter()
                .map(|res| ImportSceneSprite {
                    path: res.id.clone(),
                    name: res.name.clone(),
                    category: crate::ipc::normalize_model_library_category(Some(
                        res.model_type.as_str(),
                    )),
                    model_id: Some(res.id.clone()),
                    asset: Some(res.asset.clone()),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Tras `switch_editor_scene`: lista de escenas del registro del editor (incluye creadas sin guardar).
pub(crate) fn send_project_loaded_3d_with_editor_scenes(
    project: &ProjectSaveData,
    view: &ActiveSaveView,
    editor_scenes: &[crate::ipc::EditorSceneListItem],
) {
    send_project_loaded_3d(project, view, Some(editor_scenes));
}

fn send_project_loaded_3d(
    project: &ProjectSaveData,
    view: &ActiveSaveView,
    editor_scenes: Option<&[crate::ipc::EditorSceneListItem]>,
) {
    let scenes: Vec<ProjectLoaded3dSceneTab> = if let Some(items) = editor_scenes {
        items
            .iter()
            .map(|s| ProjectLoaded3dSceneTab {
                id: s.id,
                name: s.name.clone(),
            })
            .collect()
    } else if project.scenes.is_empty() {
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
        worldRadius: view.world.worldRadius.or_else(|| {
            let d = view.world.worldDepth.unwrap_or(50.0);
            Some(view.world.worldWidth.min(view.world.worldHeight).min(d) * 0.5)
        }),
        gridVisible: view.world.gridVisible,
        gridCellSize: view.world.gridCellSize,
        gravity: view.world.gravity,
        targetFps: view.world.targetFps,
        lightAmbient: view.world.lightAmbient,
        lightIntensity: view.world.lightIntensity,
        shadowDarkness: view.world.shadowDarkness,
        graphicsTextureTier: view
            .world
            .graphicsTextureTier
            .clone()
            .or_else(|| Some("medium".to_string())),
        textureDetailDistance: view.world.textureDetailDistance.or_else(|| {
            Some(crate::config_3d::entity_textures::default_texture_detail_near_m())
        }),
        reflectionTier: view
            .world
            .reflectionTier
            .clone()
            .or_else(|| {
                Some(
                    crate::config_3d::reflection_graphics::DEFAULT_REFLECTION_TIER
                        .wire()
                        .to_string(),
                )
            }),
        reflectionRaytracing: view.world.reflectionRaytracing,
        reflectionProbes: view.world.reflectionProbes,
        shadowTier: view.world.shadowTier.clone().or_else(|| {
            Some(
                crate::config_3d::shadow_graphics::DEFAULT_SHADOW_TIER
                    .wire()
                    .to_string(),
            )
        }),
    };

    let models = editor_models_from_manifest(project);

    let blueprints =
        serde_json::to_value(&project.blueprints).unwrap_or(serde_json::Value::Array(vec![]));

    let player = view
        .player
        .as_ref()
        .and_then(|p| serde_json::to_value(p).ok());
    let config_camera = view
        .config_camera
        .as_ref()
        .and_then(|c| serde_json::to_value(c).ok());

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
                category: None,
                model_id: None,
                asset: None,
            })
            .collect(),
        fonts: project
            .fonts
            .iter()
            .map(|f| ImportSceneSprite {
                path: f.path.clone(),
                name: f.name.clone(),
                category: None,
                model_id: None,
                asset: None,
            })
            .collect(),
        backgrounds: project
            .backgrounds
            .iter()
            .map(|b| ImportSceneSprite {
                path: b.path.clone(),
                name: b.name.clone(),
                category: None,
                model_id: None,
                asset: None,
            })
            .collect(),
        hud_images: project
            .hud_images
            .iter()
            .map(|img| ImportSceneSprite {
                path: img.path.clone(),
                name: img.name.clone(),
                category: None,
                model_id: None,
                asset: None,
            })
            .collect(),
        blueprints,
        world,
        player,
        config_camera,
        playerUiScreens: project.playerUiScreens.clone(),
        menuUiScreens: project.menuUiScreens.clone(),
    });
}

fn active_view_from_saved_scene(scene: &SavedScene) -> ActiveSaveView {
    ActiveSaveView {
        world: scene.world.clone(),
        entities: scene.entities.clone(),
        player: scene.player.clone(),
        config_camera: scene.config_camera.clone(),
        sceneId: scene.id,
        sceneName: scene.name.clone(),
    }
}

pub(crate) fn saved_scene_from_active_view(view: &ActiveSaveView) -> SavedScene {
    SavedScene {
        id: view.sceneId,
        name: view.sceneName.clone(),
        world: view.world.clone(),
        backgroundPath: None,
        entities: view.entities.clone(),
        player: view.player.clone(),
        config_camera: view.config_camera.clone(),
        config_editor_camera: None,
        sprites: Vec::new(),
        models: Vec::new(),
    }
}

pub(crate) fn saved_scene_from_snapshot_payload(
    p: &crate::ipc::SaveSceneSnapshotPayload,
    id: u32,
    name: &str,
) -> SavedScene {
    use crate::ipc::SaveEntity3DSnapshot;

    fn map_entity(e: &SaveEntity3DSnapshot) -> SavedEntity3D {
        SavedEntity3D {
            id: e.id,
            name: e.name.clone(),
            category: e.category.clone(),
            model: e.model.clone(),
            model_id: e.model_id.clone(),
            position: e.position,
            rotation: Some(e.rotation),
            scale: e.scale,
            physics_type: e.physics_type.clone(),
            colision: e.colision,
            animations: None,
            scripts: None,
            controls: e.controls.as_ref().map(|c| SavedControlBindings {
                keyboard_mouse: c
                    .keyboard_mouse
                    .iter()
                    .map(|(k, s)| {
                        (
                            k.clone(),
                            SavedScript {
                                name: s.name.clone(),
                                source: s.source.clone(),
                            },
                        )
                    })
                    .collect(),
                gamepad: c
                    .gamepad
                    .iter()
                    .map(|(k, s)| {
                        (
                            k.clone(),
                            SavedScript {
                                name: s.name.clone(),
                                source: s.source.clone(),
                            },
                        )
                    })
                    .collect(),
            }),
            blueprint_id: e.blueprint_id.clone(),
            texture_lod: e.texture_lod.clone(),
            attach_parent_id: e.attach_parent_id,
            attach_local_position: e.attach_local_position,
            attach_local_rotation: e.attach_local_rotation,
            attach_local_scale: e.attach_local_scale,
            attach_socket_host_id: e.attach_socket_host_id,
            attach_socket_name: e.attach_socket_name.clone(),
            sockets: e.sockets.clone(),
            bone_physics: e.bone_physics.clone(),
        }
    }

    SavedScene {
        id,
        name: name.to_string(),
        world: SavedWorldConfig {
            worldWidth: p.world.world_width,
            worldHeight: p.world.world_height,
            worldDepth: Some(p.world.world_depth),
            worldRadius: Some(p.world.world_radius),
            gridVisible: p.world.grid_visible,
            gridCellSize: p.world.grid_cell_size,
            gravity: Some(p.world.gravity),
            targetFps: p.world.target_fps as f64,
            lightAmbient: p.world.light_ambient,
            lightIntensity: p.world.light_intensity,
            shadowDarkness: p.world.shadow_darkness,
            graphicsTextureTier: p.world.graphics_texture_tier.clone(),
            textureDetailDistance: p.world.texture_detail_distance_m,
            reflectionTier: p.world.reflection_tier.clone(),
            reflectionRaytracing: p.world.reflection_raytracing,
            reflectionProbes: p.world.reflection_probes,
            shadowTier: p.world.shadow_tier.clone(),
        },
        backgroundPath: p.background_path.clone(),
        entities: p.entities.iter().map(map_entity).collect(),
        player: p.player.as_ref().map(map_entity),
        config_camera: p.config_camera.as_ref().map(|c| {
            use crate::ipc::PlayCameraFollowMode;
            let follow = match c.camera_follow_mode {
                PlayCameraFollowMode::FollowCharacter => "follow_character",
                PlayCameraFollowMode::MoveWithCharacter => "move_with_character",
            };
            SavedConfigCamera {
                camera_eye_position: c.camera_eye_position,
                fps_camera_yaw: c.fps_camera_yaw,
                fps_camera_pitch: c.fps_camera_pitch,
                yaw: Some(c.yaw),
                pitch: Some(c.pitch),
                fov_y: Some(c.fov_y),
                frustum_distance: Some(c.frustum_distance),
                camera_follow_mode: Some(follow.to_string()),
            }
        }),
        config_editor_camera: p.config_editor_camera.as_ref().map(|c| {
            SavedConfigEditorCamera {
                position: c.position,
                rotation: c.rotation,
            }
        }),
        sprites: p
            .sprites
            .iter()
            .map(|s| NamedPath {
                name: s.name.clone(),
                path: s.path.clone(),
                category: None,
                model_id: None,
                asset: None,
            })
            .collect(),
        models: Vec::new(),
    }
}

pub(crate) fn build_fp_placeholder_saved_scene(id: u32, name: &str) -> SavedScene {
    use crate::config_3d::character_anchor::{
        PLAY_CHARACTER_EDITOR_ORBIT_PITCH, PLAY_CHARACTER_EDITOR_ORBIT_YAW,
    };

    let world = SavedWorldConfig {
        worldWidth: 56.0,
        worldHeight: 56.0,
        worldDepth: Some(56.0),
        worldRadius: Some(28.0),
        gridVisible: true,
        gridCellSize: 1.0,
        gravity: Some(15.0),
        targetFps: 60.0,
        lightAmbient: Some(DEFAULT_LIGHT_AMBIENT),
        lightIntensity: Some(DEFAULT_LIGHT_INTENSITY),
        shadowDarkness: Some(DEFAULT_SHADOW_DARKNESS),
        graphicsTextureTier: Some("medium".to_string()),
        textureDetailDistance: Some(
            crate::config_3d::entity_textures::default_texture_detail_near_m(),
        ),
        reflectionTier: Some(
            crate::config_3d::reflection_graphics::DEFAULT_REFLECTION_TIER
                .wire()
                .to_string(),
        ),
        reflectionRaytracing: None,
        reflectionProbes: None,
        shadowTier: Some(
            crate::config_3d::shadow_graphics::DEFAULT_SHADOW_TIER
                .wire()
                .to_string(),
        ),
    };

    let ground_scale = [28.0 / 40.0, 0.02, 56.0 / 40.0];
    let sun_position = {
        let center_y = world.worldHeight * 0.5;
        let dir = crate::config_3d::directional_light::DEFAULT_LIGHT_DIR.normalize();
        let center = glam::Vec3::new(0.0, center_y, 0.0);
        (center + dir * crate::config_3d::directional_light::SUN_DISTANCE).to_array()
    };
    let entities = vec![
        SavedEntity3D {
            id: 0,
            name: "Ground".to_string(),
            category: "ground".to_string(),
            model: "[Ground]".to_string(),
            position: [0.0, 0.0, 0.0],
            rotation: Some([0.0, 0.0, 0.0, 1.0]),
            scale: ground_scale,
            physics_type: Some("static".to_string()),
            colision: true,
            animations: None,
            scripts: None,
            controls: None,
            blueprint_id: None,
            texture_lod: None,
            model_id: None,
            attach_parent_id: None,
            attach_local_position: None,
            attach_local_rotation: None,
            attach_local_scale: None,
            attach_socket_host_id: None,
            attach_socket_name: None,
            sockets: vec![],
            bone_physics: vec![],
        },
        SavedEntity3D {
            id: 0,
            name: "Sun".to_string(),
            category: "sun".to_string(),
            model: "[Sun]".to_string(),
            position: sun_position,
            rotation: Some([0.0, 0.0, 0.0, 1.0]),
            scale: [1.0, 1.0, 1.0],
            physics_type: Some("static".to_string()),
            colision: true,
            animations: None,
            scripts: None,
            controls: None,
            blueprint_id: None,
            texture_lod: None,
            model_id: None,
            attach_parent_id: None,
            attach_local_position: None,
            attach_local_rotation: None,
            attach_local_scale: None,
            attach_socket_host_id: None,
            attach_socket_name: None,
            sockets: vec![],
            bone_physics: vec![],
        },
        SavedEntity3D {
            id: 0,
            name: "Ball".to_string(),
            category: "object".to_string(),
            model: "[Ball]".to_string(),
            position: [1.5, 0.3, 8.0],
            rotation: Some([0.0, 0.0, 0.0, 1.0]),
            scale: [0.6, 0.6, 0.6],
            physics_type: Some("dynamic".to_string()),
            colision: true,
            animations: None,
            scripts: None,
            controls: None,
            blueprint_id: None,
            texture_lod: None,
            model_id: None,
            attach_parent_id: None,
            attach_local_position: None,
            attach_local_rotation: None,
            attach_local_scale: None,
            attach_socket_host_id: None,
            attach_socket_name: None,
            sockets: vec![],
            bone_physics: vec![],
        },
    ];

    let player = SavedEntity3D {
        id: 0,
        name: "Player".to_string(),
        category: "player".to_string(),
        model: "[Player]".to_string(),
        position: [0.0, 0.0, 5.0],
        rotation: Some([0.0, 0.0, 0.0, 1.0]),
        scale: [0.8, 1.7, 0.8],
        physics_type: Some("dynamic".to_string()),
        colision: true,
        animations: None,
        scripts: None,
        controls: None,
        blueprint_id: None,
        texture_lod: None,
        model_id: None,
        attach_parent_id: None,
        attach_local_position: None,
        attach_local_rotation: None,
        attach_local_scale: None,
        attach_socket_host_id: None,
        attach_socket_name: None,
        sockets: vec![],
        bone_physics: vec![],
    };

    SavedScene {
        id,
        name: name.to_string(),
        world,
        backgroundPath: None,
        entities,
        player: Some(player),
        config_camera: Some(SavedConfigCamera {
            camera_eye_position: None,
            fps_camera_yaw: None,
            fps_camera_pitch: None,
            yaw: Some(PLAY_CHARACTER_EDITOR_ORBIT_YAW),
            pitch: Some(PLAY_CHARACTER_EDITOR_ORBIT_PITCH),
            fov_y: None,
            frustum_distance: None,
            camera_follow_mode: Some("move_with_character".to_string()),
        }),
        config_editor_camera: None,
        sprites: Vec::new(),
        models: Vec::new(),
    }
}

pub(crate) fn build_minimal_project_from_store(
    game_style: &str,
    scenes: &[SavedScene],
) -> ProjectSaveData {
    ProjectSaveData {
        version: 1,
        r#type: "3D".to_string(),
        gameStyle: game_style.to_string(),
        activeSceneId: None,
        world: None,
        backgroundPath: None,
        entities: Vec::new(),
        player: None,
        config_camera: None,
        config_editor_camera: None,
        sprites: Vec::new(),
        sounds: Vec::new(),
        fonts: Vec::new(),
        backgrounds: Vec::new(),
        scenes: scenes.to_vec(),
        blueprints: Vec::new(),
        language: None,
        playerUiScreens: crate::config_3d::player_ui::defaults::default_3d_project_ui_screens(),
        menuUiScreens: Vec::new(),
        player_ui_text_boxes: Vec::new(),
        player_ui_buttons: Vec::new(),
        player_ui_images: Vec::new(),
        player_ui_objects: crate::config_3d::player_ui::defaults::default_3d_project_ui_objects(),
        hud_images: Vec::new(),
        resources: None,
    }
}
