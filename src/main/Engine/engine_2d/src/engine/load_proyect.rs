use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::ipc::{
    AnimationFrameData, ControlBindingsData, ControlScriptData, EngineCommand, EngineCommand2dOnly,
    EngineCommandCommon, EntityRestoreAnimation, EntityRestorePhysics, EntityRestoreScript,
    EntityRestoreTransform, HudImageInfo, ImportSceneCamera2d, ImportSceneEntity,
    ImportScenePayload, ImportSceneSprite, ImportSceneWorld, PlayerUiScreenInfo,
    ProjectLoaded2dCamera2d, ProjectLoaded2dEvent, ProjectLoaded2dSceneTab, ProjectLoaded2dWorld,
    SavePlayerUiButtonSnapshot, SavePlayerUiImageSnapshot, SavePlayerUiObjectSnapshot,
    SavePlayerUiTextBoxSnapshot, SaveUiScreenSnapshot, send_load_progress,
    send_project_load_2d_complete_event, send_project_loaded_2d_event,
};

use super::State;

const SCRIPT_FILE_PREFIX: &str = "@file:";
const DEFAULT_CAMERA_2D_HALF_H: f32 = 3.5;

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

#[allow(non_snake_case)]
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
    camera2d: Option<SavedCamera2d>,
    #[serde(default)]
    sprites: Vec<NamedPath>,
    #[serde(default)]
    sounds: Vec<NamedPath>,
    #[serde(default)]
    fonts: Vec<NamedPath>,
    #[serde(default)]
    backgrounds: Vec<NamedPath>,
    #[serde(default)]
    scenes: Vec<SavedScene>,
    #[serde(default)]
    blueprints: Vec<SavedBlueprint>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    playerUiScreens: Vec<SaveUiScreenSnapshot>,
    #[serde(default)]
    menuUiScreens: Vec<SaveUiScreenSnapshot>,
    #[serde(default, rename = "playerUiTextBoxes")]
    player_ui_text_boxes: Vec<SavePlayerUiTextBoxSnapshot>,
    #[serde(default, rename = "playerUiButtons")]
    player_ui_buttons: Vec<SavePlayerUiButtonSnapshot>,
    #[serde(default, rename = "playerUiImages")]
    player_ui_images: Vec<SavePlayerUiImageSnapshot>,
    #[serde(default, rename = "playerUiObjects")]
    player_ui_objects: Vec<SavePlayerUiObjectSnapshot>,
    #[serde(default, rename = "hudImages")]
    hud_images: Vec<NamedPath>,
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
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone)]
struct SavedCamera2d {
    x: f32,
    y: f32,
    halfH: f32,
}

#[derive(Debug, Deserialize, Clone)]
struct NamedPath {
    name: String,
    path: String,
}

#[allow(non_snake_case)]
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
    camera2d: Option<SavedCamera2d>,
    #[serde(default)]
    sprites: Vec<NamedPath>,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone)]
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
    points: Option<[[f32; 2]; 4]>,
    #[serde(default)]
    animations: Option<Vec<SavedAnimation>>,
    #[serde(default)]
    scripts: Option<Vec<SavedScript>>,
    #[serde(default)]
    control_bindings: Option<SavedControlBindings>,
    #[serde(default)]
    blueprint_id: Option<String>,
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
    #[serde(default)]
    selection_mode: Option<String>,
    #[serde(default)]
    grid_size: Option<u32>,
    #[serde(default)]
    cell_offset_x: Option<u32>,
    #[serde(default)]
    cell_offset_y: Option<u32>,
    #[serde(default)]
    embedded_in_model: Option<bool>,
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

#[allow(non_snake_case)]
struct ActiveSaveView {
    world: SavedWorldConfig,
    backgroundPath: Option<String>,
    entities: Vec<SavedEntity>,
    camera2d: Option<SavedCamera2d>,
    sprites: Vec<NamedPath>,
    sceneId: u32,
    sceneName: String,
}

impl State {
    /// Carga proyecto 2D desde la carpeta ya extraída por Electron (manifest + assets).
    pub(crate) fn load_proyect_from_save_path(&mut self, extract_path: &str) {
        let path = if extract_path.trim().is_empty() {
            std::env::var("RER_PROJECT_EXTRACT_DIR").unwrap_or_default()
        } else {
            extract_path.to_string()
        };

        match load_project_from_extract_dir(&path) {
            Ok(mut project) => {
                if project.r#type != "2D" {
                    log::warn!("tipo '{}' ignorado en binario 2D", project.r#type);
                    return;
                }
                let extract_dir = PathBuf::from(&path);
                resolve_loaded_paths(&mut project, &extract_dir);
                let load_started_at = Instant::now();
                let mut step_started = Instant::now();
                let open_msg = match pick_active_save_view(&project) {
                    Ok(v) => format!(
                        "Abriendo escena «{}» ({} entidades)…",
                        v.sceneName,
                        v.entities.len()
                    ),
                    Err(_) => "Abriendo proyecto 2D…".to_string(),
                };
                log::info!("{open_msg}");
                send_load_progress(&open_msg, None, None);
                match apply_loaded_proyect_2d(self, &project, load_started_at, &mut step_started) {
                    Ok(view) => {
                        send_project_loaded_2d(self, &project, &view);
                        send_project_load_2d_complete_event();
                        let done_msg = format!(
                            "Carga terminada — escena «{}» ({} entidades, {} ms)",
                            view.sceneName,
                            view.entities.len(),
                            load_started_at.elapsed().as_millis()
                        );
                        log_load_step(load_started_at, &mut step_started, &done_msg);
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
            "se esperaba directorio extraído, no archivo .save (Electron ya descomprimió)"
                .to_string(),
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
    ENTITY_MARKERS.iter().copied().find(|m| *m == marker)
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
    // Rhai embebido en manifest (p. ej. controles tras migración del .save).
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

fn resolve_scripts(
    scripts: &Option<Vec<SavedScript>>,
    extracted_dir: &Path,
) -> Option<Vec<SavedScript>> {
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

fn resolve_entity(entity: &SavedEntity, extracted_dir: &Path) -> SavedEntity {
    SavedEntity {
        path: resolve_path(&entity.path, extracted_dir),
        scripts: resolve_scripts(&entity.scripts, extracted_dir),
        control_bindings: resolve_control_bindings(&entity.control_bindings, extracted_dir),
        animations: entity.animations.as_ref().map(|anims| {
            anims
                .iter()
                .map(|anim| SavedAnimation {
                    audio_path: resolve_optional_path(&anim.audio_path, extracted_dir),
                    frames: anim
                        .frames
                        .iter()
                        .map(|f| SavedAnimationFrame {
                            path: resolve_path(&f.path, extracted_dir),
                            ..f.clone()
                        })
                        .collect(),
                    scripts: resolve_scripts(&anim.scripts, extracted_dir),
                    ..anim.clone()
                })
                .collect()
        }),
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
    project.fonts = project
        .fonts
        .iter()
        .map(|f| NamedPath {
            name: f.name.clone(),
            path: resolve_path(&f.path, extracted_dir),
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

    if !has_scenes {
        project.backgroundPath = resolve_optional_path(&project.backgroundPath, extracted_dir);
        project.sprites = project
            .sprites
            .iter()
            .map(|s| NamedPath {
                name: s.name.clone(),
                path: resolve_path(&s.path, extracted_dir),
            })
            .collect();
        project.entities = project
            .entities
            .iter()
            .map(|e| resolve_entity(e, extracted_dir))
            .collect();
        project.camera2d = project.camera2d.clone();
    } else {
        project.entities.clear();
        project.backgroundPath = None;
        project.sprites.clear();
        project.world = None;
        project.camera2d = None;
    }

    project.scenes = project
        .scenes
        .iter()
        .map(|scene| SavedScene {
            backgroundPath: resolve_optional_path(&scene.backgroundPath, extracted_dir),
            sprites: scene
                .sprites
                .iter()
                .map(|s| NamedPath {
                    name: s.name.clone(),
                    path: resolve_path(&s.path, extracted_dir),
                })
                .collect(),
            entities: scene
                .entities
                .iter()
                .map(|e| resolve_entity(e, extracted_dir))
                .collect(),
            ..scene.clone()
        })
        .collect();

    project.hud_images = project
        .hud_images
        .iter()
        .map(|img| NamedPath {
            name: img.name.clone(),
            path: resolve_path(&img.path, extracted_dir),
        })
        .collect();
    project.player_ui_text_boxes = project
        .player_ui_text_boxes
        .iter()
        .map(|b| SavePlayerUiTextBoxSnapshot {
            font_path: resolve_path(&b.font_path, extracted_dir),
            ..b.clone()
        })
        .collect();
    project.player_ui_buttons = project
        .player_ui_buttons
        .iter()
        .map(|b| SavePlayerUiButtonSnapshot {
            font_path: resolve_path(&b.font_path, extracted_dir),
            texture_path: resolve_optional_path(&b.texture_path, extracted_dir),
            ..b.clone()
        })
        .collect();
    project.player_ui_images = project
        .player_ui_images
        .iter()
        .map(|img| SavePlayerUiImageSnapshot {
            image_path: resolve_path(&img.image_path, extracted_dir),
            ..img.clone()
        })
        .collect();
    project.player_ui_objects = project
        .player_ui_objects
        .iter()
        .map(|obj| SavePlayerUiObjectSnapshot {
            texture_path: resolve_optional_path(&obj.texture_path, extracted_dir),
            ..obj.clone()
        })
        .collect();

    project.blueprints = project
        .blueprints
        .iter()
        .map(|bp| SavedBlueprint {
            path: resolve_path(&bp.path, extracted_dir),
            scripts: resolve_scripts(&bp.scripts, extracted_dir),
            control_bindings: resolve_control_bindings(&bp.control_bindings, extracted_dir),
            animations: bp.animations.as_ref().map(|anims| {
                anims
                    .iter()
                    .map(|anim| SavedAnimation {
                        audio_path: resolve_optional_path(&anim.audio_path, extracted_dir),
                        frames: anim
                            .frames
                            .iter()
                            .map(|f| SavedAnimationFrame {
                                path: resolve_path(&f.path, extracted_dir),
                                ..f.clone()
                            })
                            .collect(),
                        scripts: resolve_scripts(&anim.scripts, extracted_dir),
                        ..anim.clone()
                    })
                    .collect()
            }),
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
            backgroundPath: active.backgroundPath.clone(),
            entities: active.entities.clone(),
            camera2d: active.camera2d.clone(),
            sprites: active.sprites.clone(),
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
        backgroundPath: project.backgroundPath.clone(),
        entities: project.entities.clone(),
        camera2d: project.camera2d.clone(),
        sprites: project.sprites.clone(),
        sceneId: 1,
        sceneName: String::new(),
    })
}

fn is_player_path(p: &str) -> bool {
    entity_path_marker(p) == Some("[Player]")
}

fn find_blueprint<'a>(id: &str, blueprints: &'a [SavedBlueprint]) -> Option<&'a SavedBlueprint> {
    blueprints.iter().find(|bp| bp.id == id)
}

fn map_restore_animations(
    anims: Option<&Vec<SavedAnimation>>,
) -> Option<Vec<EntityRestoreAnimation>> {
    anims.map(|list| {
        list.iter()
            .map(|anim| EntityRestoreAnimation {
                name: anim.name.clone(),
                frames: anim
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
                    .collect(),
                fps: anim.fps,
                loop_: anim.r#loop,
                flip_horizontal: !(anim.facing_right.unwrap_or(true)),
                audio_path: anim.audio_path.clone(),
                scripts: anim
                    .scripts
                    .as_ref()
                    .map(|scripts| {
                        scripts
                            .iter()
                            .map(|s| crate::ipc::AnimScriptData {
                                name: s.name.clone(),
                                source: s.source.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                is_cancelable: anim.is_cancelable.unwrap_or(true),
                is_default: anim.is_default.unwrap_or(false),
            })
            .collect()
    })
}

fn map_restore_scripts(scripts: Option<&Vec<SavedScript>>) -> Option<Vec<EntityRestoreScript>> {
    scripts.map(|list| {
        list.iter()
            .map(|s| EntityRestoreScript {
                path: s.name.clone(),
                source: s.source.clone(),
            })
            .collect()
    })
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

struct EntityRestoreResolved {
    rotation: [f32; 4],
    scale: [f32; 3],
    physics_enabled: bool,
    physics_type: String,
    animations: Option<Vec<EntityRestoreAnimation>>,
    scripts: Option<Vec<EntityRestoreScript>>,
    control_bindings: Option<ControlBindingsData>,
}

fn resolve_entity_restore(
    entity: &SavedEntity,
    blueprints: &[SavedBlueprint],
) -> EntityRestoreResolved {
    let bp = entity
        .blueprint_id
        .as_deref()
        .and_then(|id| find_blueprint(id, blueprints));
    let is_player = is_player_path(&entity.path);
    let physics_enabled = if is_player {
        true
    } else {
        bp.and_then(|b| b.physics_enabled)
            .or(entity.physics_enabled)
            .unwrap_or(false)
    };
    let physics_type = if is_player {
        "dynamic".to_string()
    } else {
        bp.and_then(|b| b.physics_type.clone())
            .or_else(|| entity.physics_type.clone())
            .unwrap_or_else(|| "static".to_string())
    };

    EntityRestoreResolved {
        rotation: bp
            .and_then(|b| b.rotation)
            .or(entity.rotation)
            .unwrap_or([0.0, 0.0, 0.0, 1.0]),
        scale: bp.map(|b| b.scale).unwrap_or(entity.scale),
        animations: map_restore_animations(
            bp.and_then(|b| b.animations.as_ref())
                .or(entity.animations.as_ref()),
        ),
        scripts: map_restore_scripts(
            bp.and_then(|b| b.scripts.as_ref())
                .or(entity.scripts.as_ref()),
        ),
        control_bindings: map_control_bindings(
            bp.and_then(|b| b.control_bindings.as_ref())
                .or(entity.control_bindings.as_ref()),
        ),
        physics_enabled,
        physics_type,
    }
}

fn build_import_scene_entity(
    entity: &SavedEntity,
    blueprints: &[SavedBlueprint],
) -> ImportSceneEntity {
    let restore = resolve_entity_restore(entity, blueprints);
    let is_player = is_player_path(&entity.path);

    ImportSceneEntity {
        id: entity.id,
        kind: entity.kind.clone(),
        path: entity.path.clone(),
        name: entity.name.clone().filter(|n| !n.trim().is_empty()),
        transform: EntityRestoreTransform {
            position: entity.position,
            rotation: restore.rotation,
            scale: restore.scale,
        },
        physics: if restore.physics_enabled {
            Some(EntityRestorePhysics {
                enabled: true,
                body_type: restore.physics_type,
            })
        } else {
            None
        },
        animations: restore.animations,
        scripts: restore.scripts,
        control_bindings: restore.control_bindings,
        points: entity.points,
        omit_scale: is_player,
        skip_transform: false,
        apply_initial_animation_frame: Some(true),
    }
}

fn build_import_scene_payload(
    view: &ActiveSaveView,
    blueprints: &[SavedBlueprint],
) -> ImportScenePayload {
    let camera = view
        .camera2d
        .as_ref()
        .map(|c| ImportSceneCamera2d {
            x: c.x,
            y: c.y,
            half_h: c.halfH,
        })
        .or({
            Some(ImportSceneCamera2d {
                x: 0.0,
                y: 0.0,
                half_h: DEFAULT_CAMERA_2D_HALF_H,
            })
        });

    let target_fps = if view.world.targetFps.is_finite() && view.world.targetFps > 0.0 {
        view.world.targetFps as u64
    } else {
        60
    };

    ImportScenePayload {
        scene: "2D".to_string(),
        world: ImportSceneWorld {
            world_width: view.world.worldWidth,
            world_height: view.world.worldHeight,
            grid_visible: view.world.gridVisible,
            grid_cell_size: view.world.gridCellSize,
            gravity: Some(
                view.world
                    .gravity
                    .unwrap_or(rer_engine_shared::DEFAULT_GRAVITY_MAGNITUDE),
            ),
            target_fps,
        },
        background_path: view.backgroundPath.clone(),
        camera2d: camera,
        sprites: view
            .sprites
            .iter()
            .map(|s| ImportSceneSprite {
                path: s.path.clone(),
                name: s.name.clone(),
            })
            .collect(),
        entities: view
            .entities
            .iter()
            .map(|e| build_import_scene_entity(e, blueprints))
            .collect(),
    }
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
        state.handle_command(EngineCommand::Common(
            EngineCommandCommon::LoadBackgroundAsset {
                path: bg.path.clone(),
                name: bg.name.clone(),
            },
        ));
    }
    for img in &project.hud_images {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::LoadHudImage {
            path: img.path.clone(),
            name: img.name.clone(),
        }));
    }
}

fn import_player_ui_from_project(state: &mut State, project: &ProjectSaveData) {
    state.import_player_ui_text_boxes_from_save(&project.player_ui_text_boxes);
    state.import_player_ui_buttons_from_save(&project.player_ui_buttons);
    state.import_player_ui_images_from_save(&project.player_ui_images);
    state.import_player_ui_objects_from_save(&project.player_ui_objects);
    state.ensure_default_player_ui();
    let player_screens: Vec<PlayerUiScreenInfo> = if project.playerUiScreens.is_empty() {
        crate::config_2d::player_ui::defaults::default_2d_project_ui_screens_info()
    } else {
        project
            .playerUiScreens
            .iter()
            .map(|s| PlayerUiScreenInfo {
                id: s.id.clone(),
                name: s.name.clone(),
                active: s.active,
            })
            .collect()
    };
    state.sync_player_ui_screens(&player_screens);
}

fn log_load_step(total: Instant, step: &mut Instant, message: &str) {
    let step_ms = step.elapsed().as_millis() as u64;
    let total_ms = total.elapsed().as_millis() as u64;
    log::info!("{message} (+{step_ms} ms, total {total_ms} ms)");
    send_load_progress(message, Some(step_ms), Some(total_ms));
    *step = Instant::now();
}

fn apply_loaded_proyect_2d(
    state: &mut State,
    project: &ProjectSaveData,
    load_started_at: Instant,
    step_started: &mut Instant,
) -> Result<ActiveSaveView, String> {
    let view = pick_active_save_view(project)?;

    load_project_asset_stores(state, project);
    log_load_step(
        load_started_at,
        step_started,
        &format!(
            "Bibliotecas registradas ({} sonidos, {} fuentes, {} fondos)",
            project.sounds.len(),
            project.fonts.len(),
            project.backgrounds.len()
        ),
    );

    if !view.entities.is_empty() {
        let payload = build_import_scene_payload(&view, &project.blueprints);
        state.import_scene(payload);
        log_load_step(
            load_started_at,
            step_started,
            &format!("Escena importada ({} entidades)", view.entities.len()),
        );
        import_player_ui_from_project(state, project);
        return Ok(view);
    }

    state.handle_command(EngineCommand::Common(EngineCommandCommon::SetWorldSize {
        width: view.world.worldWidth,
        height: view.world.worldHeight,
        depth: None,
    }));
    state.handle_command(EngineCommand::Common(EngineCommandCommon::SetGridVisible {
        visible: view.world.gridVisible,
    }));
    state.handle_command(EngineCommand::Common(
        EngineCommandCommon::SetGridCellSize {
            size: view.world.gridCellSize,
        },
    ));
    let target_fps = if view.world.targetFps.is_finite() && view.world.targetFps > 0.0 {
        view.world.targetFps as u64
    } else {
        60
    };
    state.handle_command(EngineCommand::Common(EngineCommandCommon::SetTargetFps {
        fps: target_fps,
    }));
    if let Some(gravity) = view.world.gravity {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::SetGravity {
            gravity,
        }));
    }

    if let Some(camera) = &view.camera2d {
        state.handle_command(EngineCommand::Only2d(EngineCommand2dOnly::SetCamera2d {
            x: camera.x,
            y: camera.y,
            half_h: camera.halfH,
        }));
    }

    for sprite in &view.sprites {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::LoadSprite {
            path: sprite.path.clone(),
            name: sprite.name.clone(),
        }));
    }

    if let Some(path) = view
        .backgroundPath
        .as_ref()
        .filter(|p| !p.trim().is_empty())
    {
        state.handle_command(EngineCommand::Only2d(EngineCommand2dOnly::LoadBackground {
            path: path.clone(),
        }));
    }

    import_player_ui_from_project(state, project);

    log_load_step(load_started_at, step_started, "Proyecto 2D vacío listo");
    Ok(view)
}

fn send_project_loaded_2d(state: &State, project: &ProjectSaveData, view: &ActiveSaveView) {
    let scenes: Vec<ProjectLoaded2dSceneTab> = if project.scenes.is_empty() {
        vec![ProjectLoaded2dSceneTab {
            id: view.sceneId,
            name: view.sceneName.clone(),
        }]
    } else {
        project
            .scenes
            .iter()
            .map(|s| ProjectLoaded2dSceneTab {
                id: s.id,
                name: s.name.clone(),
            })
            .collect()
    };

    let world = ProjectLoaded2dWorld {
        worldWidth: view.world.worldWidth,
        worldHeight: view.world.worldHeight,
        worldDepth: view.world.worldDepth,
        gridVisible: view.world.gridVisible,
        gridCellSize: view.world.gridCellSize,
        gravity: view.world.gravity,
        targetFps: view.world.targetFps,
    };

    let camera2d = view.camera2d.as_ref().map(|c| ProjectLoaded2dCamera2d {
        x: c.x,
        y: c.y,
        halfH: c.halfH,
    });

    let blueprints =
        serde_json::to_value(&project.blueprints).unwrap_or(serde_json::Value::Array(vec![]));

    send_project_loaded_2d_event(&ProjectLoaded2dEvent {
        event: "project_loaded_2d",
        activeSceneId: view.sceneId,
        sceneName: view.sceneName.clone(),
        entityCount: view.entities.len() as u32,
        scenes,
        language: project.language.clone(),
        sprites: view
            .sprites
            .iter()
            .map(|s| ImportSceneSprite {
                path: s.path.clone(),
                name: s.name.clone(),
            })
            .collect(),
        sounds: project
            .sounds
            .iter()
            .map(|s| ImportSceneSprite {
                path: s.path.clone(),
                name: s.name.clone(),
            })
            .collect(),
        fonts: project
            .fonts
            .iter()
            .map(|f| ImportSceneSprite {
                path: f.path.clone(),
                name: f.name.clone(),
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
        hudImages: state
            .hud_image_store
            .iter()
            .map(|(path, meta)| HudImageInfo {
                path: path.clone(),
                name: meta.name.clone(),
                width: meta.width_px,
                height: meta.height_px,
            })
            .collect(),
        playerUiScreens: if project.playerUiScreens.is_empty() {
            crate::config_2d::player_ui::defaults::default_2d_project_ui_screens_info()
        } else {
            project
                .playerUiScreens
                .iter()
                .map(|s| PlayerUiScreenInfo {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    active: s.active,
                })
                .collect()
        },
        menuUiScreens: project.menuUiScreens.clone(),
        blueprints,
        world,
        backgroundPath: view.backgroundPath.clone(),
        camera2d,
    });
}

impl State {
    /// Proyecto 2D nuevo (sin `.save`): emite progreso y `project_loaded_2d` con Scene-01.
    /// Equivalente funcional a la plantilla 3D (`setup_default_3d_scene` + `editor_scenes_init_from_boot`).
    pub(crate) fn emit_default_2d_boot_loaded(&mut self) {
        send_load_progress("Cargando plantilla 2D…", None, None);
        send_load_progress("Plantilla 2D lista", None, None);
        log::info!("Plantilla 2D por defecto lista");

        let scene_name = rer_engine_shared::editor_defaults::default_scene_name(1);
        let camera2d = self.camera_2d.as_ref().map(|c| ProjectLoaded2dCamera2d {
            x: c.x,
            y: c.y,
            halfH: c.half_h,
        });

        let language = if self.snap_locale == "en" || self.snap_locale == "es" {
            Some(self.snap_locale.clone())
        } else {
            None
        };

        send_project_loaded_2d_event(&ProjectLoaded2dEvent {
            event: "project_loaded_2d",
            activeSceneId: 1,
            sceneName: scene_name.clone(),
            entityCount: 0,
            scenes: vec![ProjectLoaded2dSceneTab {
                id: 1,
                name: scene_name,
            }],
            language,
            sprites: Vec::new(),
            sounds: Vec::new(),
            fonts: Vec::new(),
            backgrounds: Vec::new(),
            hudImages: Vec::new(),
            playerUiScreens:
                crate::config_2d::player_ui::defaults::default_2d_project_ui_screens_info(),
            menuUiScreens: Vec::new(),
            blueprints: serde_json::Value::Array(Vec::new()),
            world: ProjectLoaded2dWorld {
                worldWidth: self.grid_config.world_width,
                worldHeight: self.grid_config.world_height,
                worldDepth: None,
                gridVisible: self.grid_config.visible,
                gridCellSize: self.grid_config.cell_size,
                gravity: Some(self.physics_2d.gravity_magnitude()),
                targetFps: self.target_fps as f64,
            },
            backgroundPath: None,
            camera2d,
        });
    }
}
