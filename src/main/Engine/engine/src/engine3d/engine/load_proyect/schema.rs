use super::*;

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone)]
pub(crate) struct ProjectSaveData {
    #[serde(default)]
    pub(crate) version: u32,
    #[serde(default)]
    pub(crate) r#type: String,
    pub gameStyle: String,
    #[serde(default)]
    pub(crate) activeSceneId: Option<u32>,
    #[serde(default)]
    pub(crate) world: Option<SavedWorldConfig>,
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) backgroundPath: Option<String>,
    #[serde(default)]
    pub(crate) entities: Vec<SavedEntity3D>,
    #[serde(default)]
    pub(crate) player: Option<SavedEntity3D>,
    #[serde(default)]
    pub(crate) config_camera: Option<SavedConfigCamera>,
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) config_editor_camera: Option<SavedConfigEditorCamera>,
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) sprites: Vec<NamedPath>,
    #[serde(default)]
    pub(crate) sounds: Vec<NamedPath>,
    #[serde(default)]
    pub(crate) fonts: Vec<NamedPath>,
    #[serde(default)]
    pub(crate) backgrounds: Vec<NamedPath>,
    #[serde(default)]
    pub scenes: Vec<SavedScene>,
    #[serde(default)]
    pub(crate) blueprints: Vec<SavedBlueprint>,
    #[serde(default)]
    pub(crate) language: Option<String>,
    #[serde(default)]
    pub(crate) playerUiScreens: Vec<crate::engine3d::ipc::SaveUiScreenSnapshot>,
    #[serde(default)]
    pub(crate) menuUiScreens: Vec<crate::engine3d::ipc::SaveUiScreenSnapshot>,
    #[serde(default, rename = "playerUiTextBoxes")]
    pub(crate) player_ui_text_boxes: Vec<crate::engine3d::ipc::SavePlayerUiTextBoxSnapshot>,
    #[serde(default, rename = "playerUiButtons")]
    pub(crate) player_ui_buttons: Vec<crate::engine3d::ipc::SavePlayerUiButtonSnapshot>,
    #[serde(default, rename = "playerUiImages")]
    pub(crate) player_ui_images: Vec<crate::engine3d::ipc::SavePlayerUiImageSnapshot>,
    #[serde(default, rename = "playerUiObjects")]
    pub(crate) player_ui_objects: Vec<crate::engine3d::ipc::SavePlayerUiObjectSnapshot>,
    #[serde(default, rename = "hudImages")]
    pub(crate) hud_images: Vec<NamedPath>,
    #[serde(default)]
    pub(crate) resources: Option<SavedResources>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedResources {
    #[serde(default)]
    pub(crate) models: Vec<SavedResourceModel>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedResourceModel {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(rename = "type")]
    pub(crate) model_type: String,
    pub(crate) asset: String,
    pub(crate) importer_version: u16,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedWorldConfig {
    pub(crate) worldWidth: f32,
    pub(crate) worldHeight: f32,
    #[serde(default)]
    pub(crate) worldDepth: Option<f32>,
    #[serde(default)]
    pub(crate) worldRadius: Option<f32>,
    #[serde(default)]
    pub(crate) gridVisible: bool,
    pub(crate) gridCellSize: f32,
    #[serde(default)]
    pub(crate) gravity: Option<f32>,
    pub(crate) targetFps: f64,
    #[serde(default)]
    pub(crate) lightAmbient: Option<f32>,
    #[serde(default)]
    pub(crate) lightIntensity: Option<f32>,
    #[serde(default)]
    pub(crate) shadowDarkness: Option<f32>,
    #[serde(default)]
    pub(crate) graphicsTextureTier: Option<String>,
    #[serde(default)]
    pub(crate) textureDetailDistance: Option<f32>,
    #[serde(default)]
    pub(crate) reflectionTier: Option<String>,
    #[serde(default)]
    pub(crate) reflectionRaytracing: Option<bool>,
    #[serde(default)]
    pub(crate) reflectionProbes: Option<bool>,
    #[serde(default)]
    pub(crate) shadowTier: Option<String>,
    #[serde(default)]
    pub(crate) msaaTier: Option<String>,
}

#[derive(Debug, Default, Deserialize, Clone, Serialize)]
pub(crate) struct NamedPath {
    pub(crate) name: String,
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) category: Option<String>,
    #[serde(default)]
    pub(crate) model_id: Option<String>,
    #[serde(default)]
    pub(crate) asset: Option<String>,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedConfigCamera {
    #[serde(default)]
    pub(crate) camera_eye_position: Option<[f32; 3]>,
    #[serde(default)]
    pub(crate) fps_camera_yaw: Option<f32>,
    #[serde(default)]
    pub(crate) fps_camera_pitch: Option<f32>,
    #[serde(default)]
    pub(crate) yaw: Option<f32>,
    #[serde(default)]
    pub(crate) pitch: Option<f32>,
    #[serde(default)]
    pub(crate) fov_y: Option<f32>,
    #[serde(default)]
    pub(crate) frustum_distance: Option<f32>,
    #[serde(default)]
    pub(crate) camera_follow_mode: Option<String>,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedConfigEditorCamera {
    pub(crate) position: [f32; 3],
    #[serde(default)]
    pub(crate) rotation: Option<[f32; 4]>,
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
    pub hardness: u32,
    #[serde(default)]
    pub animations: Option<Vec<SavedAnimation>>,
    #[serde(default)]
    pub scripts: Option<Vec<SavedScript>>,
    #[serde(default, alias = "control_bindings")]
    pub controls: Option<SavedControlBindings>,
    #[serde(default)]
    pub blueprint_id: Option<String>,
    #[serde(default)]
    pub texture_lod: Option<crate::engine3d::ipc::SaveEntityTextureLodSnapshot>,
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
    pub sockets: Vec<crate::engine3d::config_3d::entity_sockets::EntitySocketSnapshot>,
    #[serde(default)]
    pub bone_physics: Vec<crate::engine3d::config_3d::bone_physics::BonePhysicsSnapshot>,
    #[serde(default)]
    pub projectile: Option<crate::engine3d::config_3d::projectiles::ProjectileConfig>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedAnimation {
    pub(crate) name: String,
    pub(crate) fps: u32,
    pub(crate) r#loop: bool,
    #[serde(default)]
    pub(crate) is_default: Option<bool>,
    #[serde(default)]
    pub(crate) facing_right: Option<bool>,
    pub(crate) logical_w: u32,
    pub(crate) logical_h: u32,
    #[serde(default)]
    pub(crate) audio_path: Option<String>,
    pub(crate) frames: Vec<SavedAnimationFrame>,
    #[serde(default)]
    pub(crate) scripts: Option<Vec<SavedScript>>,
    #[serde(default)]
    pub(crate) is_cancelable: Option<bool>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedAnimationFrame {
    pub(crate) path: String,
    pub(crate) pivot_x: f32,
    pub(crate) pivot_y: f32,
    #[serde(default)]
    pub(crate) src_x: Option<u32>,
    #[serde(default)]
    pub(crate) src_y: Option<u32>,
    #[serde(default)]
    pub(crate) src_w: Option<u32>,
    #[serde(default)]
    pub(crate) src_h: Option<u32>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedScript {
    pub(crate) name: String,
    pub(crate) source: String,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedControlBindings {
    #[serde(default)]
    pub(crate) keyboard_mouse: HashMap<String, SavedScript>,
    #[serde(default)]
    pub(crate) gamepad: HashMap<String, SavedScript>,
}

/// `Blueprint3D` en types.ts.
#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Clone, Serialize)]
pub(crate) struct SavedBlueprint {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) category: String,
    pub(crate) model: String,
    #[serde(default)]
    pub(crate) model_id: Option<String>,
    #[serde(default)]
    pub(crate) physics_type: Option<String>,
    #[serde(default = "default_colision_on")]
    pub(crate) colision: bool,
    #[serde(default)]
    pub(crate) animations: Option<Vec<SavedAnimation>>,
    #[serde(default)]
    pub(crate) scripts: Option<Vec<SavedScript>>,
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
pub(crate) struct PendingRestore {
    pub(crate) transform: EntityRestoreTransform,
    pub(crate) name: Option<String>,
    pub(crate) physics_enabled: bool,
    pub(crate) physics_type: String,
    pub(crate) colision: bool,
    pub(crate) hardness: u32,
    pub(crate) animations: Option<Vec<SavedAnimation>>,
    pub(crate) scripts: Option<Vec<SavedScript>>,
    pub(crate) control_bindings: Option<ControlBindingsData>,
    pub(crate) blueprint_id: Option<String>,
    pub(crate) entity_category: Option<String>,
    pub(crate) visual_model_path: Option<String>,
    /// Id estable del manifest; debe coincidir al spawnear para restaurar fusiones.
    pub(crate) saved_entity_id: u32,
}

pub(crate) fn default_colision_on() -> bool {
    true
}
