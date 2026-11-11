use serde::{Deserialize, Serialize};

use super::*;

/// Modo de seguimiento del ojo FPS respecto al jugador en editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayCameraFollowMode {
    /// Reposiciona el ojo hacia la cabeza + offset y recorta con raycast si hay obstáculos.
    FollowCharacter,
    /// Traslada el ojo con el mismo delta que los pies del jugador.
    #[default]
    MoveWithCharacter,
}

// ---------------------------------------------------------------------------
// Tipos y eventos exclusivos del motor 3D (comandos comunes -> core/src/ipc)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
pub struct SaveWorldSnapshot {
    pub world_radius: f32,
    pub world_width: f32,
    pub world_height: f32,
    pub world_depth: f32,
    pub grid_visible: bool,
    pub grid_cell_size: f32,
    pub gravity: f32,
    pub target_fps: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light_ambient: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light_intensity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_darkness: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphics_texture_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture_detail_distance_m: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reflection_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reflection_raytracing: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reflection_probes: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msaa_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taa_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taa_blend: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taa_jitter_scale: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SaveScriptSnapshot {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SaveAnimationFrameSnapshot {
    pub path: String,
    pub pivot_x: f32,
    pub pivot_y: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_x: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_y: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_w: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_h: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SaveAnimationSnapshot {
    pub name: String,
    pub fps: u32,
    pub loop_: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facing_right: Option<bool>,
    pub logical_w: u32,
    pub logical_h: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_path: Option<String>,
    pub frames: Vec<SaveAnimationFrameSnapshot>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<SaveScriptSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_cancelable: Option<bool>,
    /// Clip embebido en modelo 3D (sin frames PNG).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedded_in_model: Option<bool>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ModelClipInfoEvent {
    pub name: String,
    pub duration_s: f32,
    pub fps: f32,
}

/// Asignación de textura embebida por material y nivel gráfico (persistencia `.save`).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SaveMaterialTextureLod {
    pub material_index: u32,
    #[serde(default)]
    pub tier_image_index: std::collections::HashMap<String, u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SaveEntityTextureLodSnapshot {
    #[serde(default = "default_texture_lod_preview_tier")]
    pub preview_tier: String,
    #[serde(default)]
    pub active_material_index: u32,
    #[serde(default)]
    pub materials: Vec<SaveMaterialTextureLod>,
}

fn default_texture_lod_preview_tier() -> String {
    "low".to_string()
}

/// Entidad 3D — docs/yaml/gameplay/Entities_Model_3D.yaml (`common`).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SaveEntity3DSnapshot {
    pub id: u32,
    pub name: String,
    pub category: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physics_type: Option<String>,
    pub colision: bool,
    /// Dureza de gameplay (0 = proyectiles la ignoran).
    #[serde(default)]
    pub hardness: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animations: Option<Vec<SaveAnimationSnapshot>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scripts: Option<Vec<SaveScriptSnapshot>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blueprint_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controls: Option<ControlBindingsData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture_lod: Option<SaveEntityTextureLodSnapshot>,
    /// Padre de fusión (entidad ancla; esta entidad es el hijo).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "attachParentId"
    )]
    pub attach_parent_id: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "attachLocalPosition"
    )]
    pub attach_local_position: Option<[f32; 3]>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "attachLocalRotation"
    )]
    pub attach_local_rotation: Option<[f32; 4]>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "attachLocalScale"
    )]
    pub attach_local_scale: Option<[f32; 3]>,
    /// Host del socket (esta entidad es hijo enganchado a un socket).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "attachSocketHostId"
    )]
    pub attach_socket_host_id: Option<u32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "attachSocketName"
    )]
    pub attach_socket_name: Option<String>,
    /// Sockets definidos en esta entidad host.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sockets: Vec<crate::engine3d::config_3d::entity_sockets::EntitySocketSnapshot>,
    /// Física secundaria por hueso (jiggle).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bone_physics: Vec<crate::engine3d::config_3d::bone_physics::BonePhysicsSnapshot>,
    /// Config de disparo (solo categoría `projectile`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projectile: Option<crate::engine3d::config_3d::projectiles::ProjectileConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SaveConfigCameraSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_eye_position: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps_camera_yaw: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps_camera_pitch: Option<f32>,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y: f32,
    pub frustum_distance: f32,
    #[serde(default)]
    pub camera_follow_mode: PlayCameraFollowMode,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SaveConfigEditorCameraSnapshot {
    pub position: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<[f32; 4]>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SaveCamera2dSnapshot {
    pub x: f32,
    pub y: f32,
    pub half_h: f32,
}

#[derive(Debug, Serialize, Clone)]
pub struct SaveAssetRefSnapshot {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SaveUiScreenSnapshot {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub active: bool,
}

/// Cuadros HUD en manifest / snapshot del motor (`screen_id`, mismo criterio que entidades).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SavePlayerUiTextBoxSnapshot {
    pub scope: String,
    #[serde(alias = "screenId")]
    pub screen_id: String,
    pub id: u32,
    #[serde(alias = "fontPath")]
    pub font_path: String,
    #[serde(alias = "fontName")]
    pub font_name: String,
    pub text: String,
    #[serde(alias = "centerX")]
    pub center_x: f32,
    #[serde(alias = "centerY")]
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default)]
    pub locked: bool,
}

/// Botones HUD en manifest / snapshot del motor.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SavePlayerUiButtonSnapshot {
    pub scope: String,
    #[serde(alias = "screenId")]
    pub screen_id: String,
    pub id: u32,
    #[serde(rename = "type", alias = "shape_type")]
    pub shape_type: String,
    pub round: f32,
    #[serde(alias = "backgroundColor")]
    pub background_color: [f32; 4],
    #[serde(default, alias = "texturePath")]
    pub texture_path: Option<String>,
    #[serde(alias = "transparencyBackground")]
    pub transparency_background: f32,
    pub text: String,
    #[serde(alias = "textColor")]
    pub text_color: [f32; 4],
    #[serde(alias = "transparencyText")]
    pub transparency_text: f32,
    #[serde(alias = "fontPath")]
    pub font_path: String,
    #[serde(alias = "fontName")]
    pub font_name: String,
    #[serde(alias = "borderColor")]
    pub border_color: [f32; 4],
    #[serde(alias = "borderWeight")]
    pub border_weight: f32,
    #[serde(alias = "centerX")]
    pub center_x: f32,
    #[serde(alias = "centerY")]
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default, alias = "sourceAspect")]
    pub source_aspect: Option<f32>,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default)]
    pub locked: bool,
}

/// Objetos HUD poligonales en manifest / snapshot del motor.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SavePlayerUiObjectSnapshot {
    pub scope: String,
    #[serde(alias = "screenId")]
    pub screen_id: String,
    pub id: u32,
    pub vertices: Vec<[f32; 2]>,
    #[serde(alias = "fillColor", default = "default_object_fill")]
    pub fill_color: [f32; 4],
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "texturePath"
    )]
    pub texture_path: Option<String>,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default)]
    pub locked: bool,
}

fn default_object_fill() -> [f32; 4] {
    crate::engine3d::config_3d::player_ui::object::DEFAULT_OBJECT_FILL
}

/// Imágenes HUD en manifest / snapshot del motor.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SavePlayerUiImageSnapshot {
    pub scope: String,
    #[serde(alias = "screenId")]
    pub screen_id: String,
    pub id: u32,
    #[serde(alias = "imagePath")]
    pub image_path: String,
    #[serde(alias = "imageName")]
    pub image_name: String,
    #[serde(alias = "centerX")]
    pub center_x: f32,
    #[serde(alias = "centerY")]
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(alias = "sourceAspect")]
    pub source_aspect: f32,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default)]
    pub locked: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct SaveSceneSnapshotPayload {
    pub world: SaveWorldSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_path: Option<String>,
    pub entities: Vec<SaveEntity3DSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player: Option<SaveEntity3DSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_camera: Option<SaveConfigCameraSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_editor_camera: Option<SaveConfigEditorCameraSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera2d: Option<SaveCamera2dSnapshot>,
    pub sprites: Vec<SaveAssetRefSnapshot>,
    pub models: Vec<SaveAssetRefSnapshot>,
    pub sounds: Vec<SaveAssetRefSnapshot>,
    pub backgrounds: Vec<SaveAssetRefSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fonts: Vec<SaveAssetRefSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hud_images: Vec<SaveAssetRefSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub player_ui_text_boxes: Vec<SavePlayerUiTextBoxSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub player_ui_buttons: Vec<SavePlayerUiButtonSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub player_ui_images: Vec<SavePlayerUiImageSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub player_ui_objects: Vec<SavePlayerUiObjectSnapshot>,
}

// ---------------------------------------------------------------------------
// Eventos que el motor envía a Electron (motor → stdout)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
pub struct EditorSceneListItem {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub dirty: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct PlayerUiTextBoxListItem {
    pub id: u32,
    pub font_name: String,
    pub text: String,
    pub z_index: i32,
    pub locked: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct PlayerUiButtonListItem {
    pub id: u32,
    pub text: String,
    pub font_name: String,
    pub z_index: i32,
    pub locked: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct PlayerUiImageListItem {
    pub id: u32,
    pub image_name: String,
    pub z_index: i32,
    pub locked: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct PlayerUiObjectListItem {
    pub id: u32,
    pub vertex_count: u32,
    pub fill_color: [f32; 4],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture_path: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub texture_name: String,
    pub z_index: i32,
    pub locked: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HudImageInfo {
    pub path: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
}

/// Información básica de un sprite almacenado en el motor.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SpriteInfo {
    pub path: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
}

/// Entrada en `State::model_store` (biblioteca Resources / precarga).
#[derive(Debug, Clone, Default)]
pub struct ModelStoreEntry {
    pub name: String,
    pub category: Option<String>,
    pub model_id: Option<String>,
    pub rerasset_path: Option<String>,
}

/// Información básica de un modelo 3D en el almacén de recursos.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelInfo {
    pub path: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

/// Categorías válidas de biblioteca (`ModelCategory` en el renderer).
pub fn normalize_model_library_category(raw: Option<&str>) -> Option<String> {
    let s = raw?.trim();
    match s {
        "character" | "environment" | "object" | "weapon" | "projectile" => Some(s.to_string()),
        _ => None,
    }
}

/// Manifest de blueprint activa en construcción rápida (`docs/yaml/gameplay/Blueprints_Model_3D.yaml`).
#[derive(Debug, Clone, Deserialize)]
pub struct BlueprintPlacementMeta {
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub colision: Option<bool>,
    #[serde(default)]
    pub physics_type: Option<String>,
    #[serde(default)]
    pub physics_enabled: Option<bool>,
    #[serde(default)]
    pub rotation: Option<[f32; 4]>,
    #[serde(default)]
    pub scale: Option<[f32; 3]>,
    #[serde(default)]
    pub blueprint_id: Option<String>,
    /// Nombre de plantilla (p. ej. `Environment_04`); no es el nombre de la instancia.
    #[serde(default)]
    pub template_name: Option<String>,
    #[serde(default)]
    pub scripts: Option<Vec<SaveScriptSnapshot>>,
    #[serde(default)]
    pub animations: Option<Vec<SaveAnimationSnapshot>>,
}

fn merge_optional_field<T: Clone>(dst: &mut Option<T>, src: &Option<T>) {
    if dst.is_none() {
        *dst = src.clone();
    }
}

/// Completa categoría/física/scripts desde el registro del motor o la biblioteca de modelos.
pub fn enrich_blueprint_placement_meta(
    blueprint_registry: &std::collections::HashMap<String, BlueprintPlacementMeta>,
    model_store: &std::collections::HashMap<String, ModelStoreEntry>,
    model_path_key: &dyn Fn(&str) -> String,
    meta: &mut BlueprintPlacementMeta,
    preview_path: &str,
) {
    if let Some(id) = meta.blueprint_id.as_deref()
        && let Some(reg) = blueprint_registry.get(id)
    {
        let weak = meta
            .category
            .as_deref()
            .map(|c| c.trim().is_empty() || c == "object")
            .unwrap_or(true);
        if weak && let Some(cat) = reg.category.clone() {
            meta.category = Some(cat);
        }
        merge_optional_field(&mut meta.colision, &reg.colision);
        merge_optional_field(&mut meta.physics_type, &reg.physics_type);
        merge_optional_field(&mut meta.physics_enabled, &reg.physics_enabled);
        merge_optional_field(&mut meta.scale, &reg.scale);
        merge_optional_field(&mut meta.scripts, &reg.scripts);
        merge_optional_field(&mut meta.animations, &reg.animations);
        if meta
            .template_name
            .as_deref()
            .map(|n| n.trim().is_empty() || n == "Blueprint")
            .unwrap_or(true)
        {
            merge_optional_field(&mut meta.template_name, &reg.template_name);
        }
    }

    let weak_cat = meta
        .category
        .as_deref()
        .map(|c| c.trim().is_empty() || c == "object")
        .unwrap_or(true);
    if weak_cat {
        let key = model_path_key(preview_path);
        if let Some(entry) = model_store.get(&key)
            && let Some(cat) = normalize_model_library_category(entry.category.as_deref())
        {
            meta.category = Some(cat);
        }
    }
}

/// Categoría para colocar instancias (nombrado incremental + físicas).
pub fn normalize_placement_entity_category(raw: Option<&str>) -> Option<String> {
    let s = raw?.trim();
    match s {
        "environment" | "object" | "character" | "weapon" | "projectile" => Some(s.to_string()),
        "player" => Some("character".to_string()),
        "sun" => Some("sun".to_string()),
        "ground" => Some("ground".to_string()),
        _ => normalize_model_library_category(raw),
    }
}

/// Información básica de un sonido almacenado en el motor.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SoundInfo {
    pub path: String,
    pub name: String,
}

/// Información básica de una fuente almacenada en el motor.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FontInfo {
    pub path: String,
    pub name: String,
}

/// Información básica de un fondo almacenado en el motor.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BackgroundInfo {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct ImportSceneSprite {
    pub path: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
}

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Clone)]
pub struct ProjectLoaded3dSceneTab {
    pub id: u32,
    pub name: String,
}

/// Mundo en `project_loaded_3d` — mismas claves que `SavedWorldConfig` en `types.ts`.
#[allow(non_snake_case)]
#[derive(Debug, Serialize, Clone)]
pub struct ProjectLoaded3dWorld {
    pub worldWidth: f32,
    pub worldHeight: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worldDepth: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worldRadius: Option<f32>,
    pub gridVisible: bool,
    pub gridCellSize: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gravity: Option<f32>,
    pub targetFps: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lightAmbient: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lightIntensity: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadowDarkness: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graphicsTextureTier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub textureDetailDistance: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reflectionTier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reflectionRaytracing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reflectionProbes: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadowTier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msaaTier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taaEnabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taaBlend: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taaJitterScale: Option<f32>,
}

/// Evento `project_loaded_3d` (fuera de `EngineEvent` para no heredar snake_case del enum).
#[allow(non_snake_case)]
#[derive(Debug, Serialize)]
pub struct ProjectLoaded3dEvent {
    pub event: &'static str,
    pub activeSceneId: u32,
    pub sceneName: String,
    pub entityCount: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenes: Vec<ProjectLoaded3dSceneTab>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub models: Vec<ImportSceneSprite>,
    pub sounds: Vec<ImportSceneSprite>,
    pub fonts: Vec<ImportSceneSprite>,
    pub backgrounds: Vec<ImportSceneSprite>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "hudImages")]
    pub hud_images: Vec<ImportSceneSprite>,
    pub blueprints: serde_json::Value,
    pub world: ProjectLoaded3dWorld,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_camera: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub playerUiScreens: Vec<SaveUiScreenSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menuUiScreens: Vec<SaveUiScreenSnapshot>,
}
