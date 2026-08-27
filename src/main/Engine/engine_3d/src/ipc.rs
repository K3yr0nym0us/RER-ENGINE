use std::{
    io::{self, BufRead, Write},
    thread,
};

use winit::event_loop::EventLoopProxy;

use serde::{Deserialize, Serialize};

pub use crate::engine_command::{EngineCommand, EngineCommand3dOnly};
pub use rer_engine_ipc_common::{
    AddPlayerUiButtonPayload, AnimScriptData, AnimationFrameData, AxisValue, ControlBindingsData,
    ControlScriptData, EngineCommandCommon, EntityRestorePhysics, EntityRestoreTransform,
    PlayerUiScreenInfo, RotationEulerDelta,
};

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
// Tipos y eventos exclusivos del motor 3D (comandos comunes -> engine_ipc_common)
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

/// Entidad 3D — docs/Entities_Model_3D.yaml (`common`).
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
    pub sockets: Vec<crate::config_3d::entity_sockets::EntitySocketSnapshot>,
    /// Física secundaria por hueso (jiggle).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bone_physics: Vec<crate::config_3d::bone_physics::BonePhysicsSnapshot>,
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
    crate::config_3d::player_ui::object::DEFAULT_OBJECT_FILL
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
#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum EngineEvent {
    Ready {
        gravity: f32,
    },
    Pong,
    Error {
        message: String,
    },
    ModelLoaded {
        id: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<[f32; 3]>,
        #[serde(skip_serializing_if = "Option::is_none")]
        scale: Option<[f32; 3]>,
        #[serde(skip_serializing_if = "Option::is_none")]
        rotation: Option<[f32; 4]>,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        blueprint_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        physics_enabled: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        physics_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        entity_category: Option<String>,
    },
    /// Mesh visual de una entidad reemplazado por otro archivo 3D.
    EntityModelReplaced {
        id: u32,
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<[f32; 3]>,
        #[serde(skip_serializing_if = "Option::is_none")]
        rotation: Option<[f32; 4]>,
        #[serde(skip_serializing_if = "Option::is_none")]
        scale: Option<[f32; 3]>,
    },
    /// Emitido cuando el usuario hace click izquierdo sobre una entidad.
    EntitySelected {
        id: u32,
        name: String,
        position: [f32; 3],
        rotation: [f32; 4], // quaternion xyzw
        scale: [f32; 3],
        physics_enabled: bool,
        physics_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        blueprint_id: Option<String>,
    },
    /// Emitido cuando el usuario hace click izquierdo en vacío.
    EntityDeselected,
    /// Emitido cuando el cursor pasa por encima de una entidad (solo cuando cambia).
    EntityHovered {
        id: u32,
    },
    /// Emitido cuando el cursor deja de estar sobre cualquier entidad.
    EntityUnhovered,
    /// Emitido cuando un escenario PNG se cargó correctamente.
    ScenarioLoaded {
        id: u32,
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Emitido cuando un personaje PNG se cargó correctamente.
    CharacterLoaded {
        id: u32,
        path: String,
    },
    /// Emitido cuando la cámara 2D cambia (fin de pan o zoom).
    #[serde(rename = "camera_2d_updated")]
    Camera2dUpdated {
        x: f32,
        y: f32,
        half_h: f32,
    },
    /// Emitido cuando se cargó una imagen de fondo del mundo.
    BackgroundLoaded {
        path: String,
    },
    /// Emitido cuando una herramienta de dibujo fue cancelada desde el motor.
    ToolCancelled,
    /// Progreso de herramienta de dibujo por puntos (colisionador 2D u objeto HUD UI).
    DrawingProgress {
        count: u32,
    },
    /// Colisionador creado (2D: `points`; 3D: `position` + `scale` del muro plano).
    ColliderCreated {
        id: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        points: Option<[[f32; 2]; 4]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        position: Option<[f32; 3]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale: Option<[f32; 3]>,
    },
    /// Área de ejecución creada (2D: `points`; 3D: `position` + `scale` del plano trigger).
    ExecutionAreaCreated {
        id: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        points: Option<[[f32; 2]; 4]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        position: Option<[f32; 3]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale: Option<[f32; 3]>,
    },
    /// Emitido cuando el usuario selecciona el pivot de un frame en modo edición.
    PivotSelected {
        frame_path: String,
        pivot_x: f32,
        pivot_y: f32,
    },
    /// Emitido cuando una animación termina (no loop) o se detiene.
    AnimationFinished {
        entity_id: u32,
    },
    /// Estado de reproducción de animación de una entidad (consulta o tras play/stop).
    #[serde(rename = "entity_animation_play_state")]
    EntityAnimationPlayState {
        entity_id: u32,
        playing: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loop_: Option<bool>,
    },
    /// El array de texturas 1024×256 capas está lleno.
    #[serde(rename = "texture_array_exhausted")]
    TextureArrayExhausted {
        max_layers: u32,
    },
    /// Clips de animación embebidos en el modelo 3D de una entidad.
    ModelClipsReady {
        id: u32,
        path: String,
        clips: Vec<ModelClipInfoEvent>,
    },
    /// Progreso de carga de un GLB grande (hilo en segundo plano).
    ModelLoadProgress {
        path: String,
        stage: String,
    },
    /// Mensaje legible al abrir un `.save` 3D (también en stderr).
    LoadProgress {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        step_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        total_ms: Option<u64>,
    },
    /// Emitido cuando el estado de física de una entidad cambia (activado/desactivado por script).
    PhysicsChanged {
        entity_id: u32,
        enabled: bool,
        body_type: String,
    },
    /// Emitido cuando un sprite PNG se cargó correctamente en el almacén.
    SpriteLoaded {
        path: String,
        name: String,
        width: u32,
        height: u32,
    },
    /// Emitido cuando se eliminó un sprite del almacén.
    SpriteRemoved {
        path: String,
    },
    /// Emitido como respuesta a GetSpritesList: lista de sprites disponibles.
    SpritesList {
        sprites: Vec<SpriteInfo>,
    },
    /// Bake `.rerasset` en curso (import en Resources).
    ModelAssetImporting {
        model_id: String,
        path: String,
        name: String,
    },
    /// Bake `.rerasset` completado (aún puede estar subiendo a GPU).
    ModelAssetImported {
        model_id: String,
        path: String,
        name: String,
        asset: String,
    },
    /// Modelo 3D registrado en el almacén de recursos (GPU listo).
    ModelAssetLoaded {
        path: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model_id: Option<String>,
    },
    /// Precarga de modelo 3D iniciada (parseo en segundo plano).
    ModelAssetPreloadStarted {
        path: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model_id: Option<String>,
    },
    /// Falló la carga/precarga de un modelo en Recursos (libera `loading` en el renderer).
    ModelAssetLoadFailed {
        path: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model_id: Option<String>,
    },
    ModelAssetRemoved {
        path: String,
    },
    ModelsList {
        models: Vec<ModelInfo>,
    },
    /// Emitido cuando un archivo de audio se registró en el almacén.
    SoundLoaded {
        path: String,
        name: String,
    },
    /// Emitido cuando se eliminó un sonido del almacén.
    SoundRemoved {
        path: String,
    },
    /// Emitido como respuesta a GetSoundsList: lista de sonidos disponibles.
    SoundsList {
        sounds: Vec<SoundInfo>,
    },
    /// Emitido cuando un archivo de fuente se registró en el almacén.
    FontLoaded {
        path: String,
        name: String,
    },
    /// Emitido cuando se eliminó una fuente del almacén.
    FontRemoved {
        path: String,
    },
    /// Emitido como respuesta a GetFontsList: lista de fuentes disponibles.
    FontsList {
        fonts: Vec<FontInfo>,
    },
    HudImageLoaded {
        path: String,
        name: String,
        width: u32,
        height: u32,
    },
    HudImageRemoved {
        path: String,
    },
    HudImagesList {
        images: Vec<HudImageInfo>,
    },
    /// Emitido cuando un fondo se registró en el almacén.
    BackgroundAssetLoaded {
        path: String,
        name: String,
    },
    /// Emitido cuando se eliminó un fondo del almacén.
    BackgroundAssetRemoved {
        path: String,
    },
    /// Emitido como respuesta a GetBackgroundsList: lista de fondos disponibles.
    BackgroundsList {
        backgrounds: Vec<BackgroundInfo>,
    },
    /// Ghost de construcción rápida 3D listo para previsualizar.
    QuickBuildGhostReady {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Herramienta muro/trigger 3D activa (ghost listo).
    PlaneToolReady {
        tool: String,
        width: f32,
        height: f32,
    },
    /// Emitido cuando el cursor se mueve y la herramienta quick_build_place está activa.
    QuickBuildMove {
        x: f32,
        y: f32,
        #[serde(default)]
        z: f32,
    },
    /// Emitido cuando el usuario hace click con la herramienta quick_build_place activa.
    /// `fit_to_grid` indica si Ctrl estaba presionado al colocar.
    /// `scale` contiene el tamaño final resuelto por el motor para esta colocación.
    QuickBuildClick {
        x: f32,
        y: f32,
        #[serde(default)]
        z: f32,
        fit_to_grid: bool,
        scale: [f32; 3],
    },
    /// Emitido cuando SetAnimation resolvió/normalizó el tamaño lógico final.
    AnimationLogicalResolved {
        id: u32,
        name: String,
        logical_w: u32,
        logical_h: u32,
    },
    /// Emitido cuando una entidad es eliminada del mundo (por Ctrl+Z, RemoveEntity, etc.).
    /// `kind` permite al frontend sincronizar estado sin inferencias locales.
    EntityRemoved {
        id: u32,
        kind: String,
    },
    /// Emitido cuando un actor entra en un área de ejecución (trigger).
    TriggerEntered {
        trigger_id: u32,
        actor_id: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        has_attached_script: Option<bool>,
    },
    /// Emitido cuando un actor sale de un área de ejecución (trigger).
    TriggerExited {
        trigger_id: u32,
        actor_id: u32,
    },
    /// Emitido cuando el usuario mantiene Ctrl y hace click añadiendo/quitando entidades
    /// a la selección múltiple. `ids` contiene todos los IDs actualmente seleccionados.
    MultiSelectChanged {
        ids: Vec<u32>,
    },
    /// Entidades fusionadas en editor (padre + hijos con offset local).
    #[serde(rename = "entities_merged")]
    EntitiesMerged {
        parent_id: u32,
        child_ids: Vec<u32>,
    },
    /// Vínculos de fusión restaurados al cargar escena desde `.save`.
    #[serde(rename = "entities_attachments_restored")]
    EntitiesAttachmentsRestored {
        count: usize,
    },
    /// Lista de huesos del modelo skinned de una entidad.
    #[serde(rename = "entity_bones_list")]
    EntityBonesList {
        entity_id: u32,
        bones: Vec<String>,
    },
    /// Respuesta de consulta de sockets (sin mutación; no re-disparar fetch).
    #[serde(rename = "entity_sockets_list")]
    EntitySocketsList {
        entity_id: u32,
        sockets: Vec<crate::config_3d::entity_sockets::EntitySocketSnapshot>,
    },
    /// Sockets de una entidad host actualizados (mutación: crear/editar/eliminar).
    #[serde(rename = "entity_sockets_changed")]
    EntitySocketsChanged {
        entity_id: u32,
        sockets: Vec<crate::config_3d::entity_sockets::EntitySocketSnapshot>,
    },
    /// Hueso elegido con click en viewport (modo socket).
    #[serde(rename = "socket_bone_picked")]
    SocketBonePicked {
        entity_id: u32,
        bone_name: String,
    },
    /// Hueso elegido con click en viewport (modo física por hueso).
    #[serde(rename = "bone_physics_picked")]
    BonePhysicsPicked {
        entity_id: u32,
        bone_name: String,
    },
    /// Lista de física por hueso de una entidad.
    #[serde(rename = "entity_bone_physics_list")]
    EntityBonePhysicsList {
        entity_id: u32,
        entries: Vec<crate::config_3d::bone_physics::BonePhysicsSnapshot>,
    },
    /// Física por hueso actualizada.
    #[serde(rename = "entity_bone_physics_changed")]
    EntityBonePhysicsChanged {
        entity_id: u32,
        entries: Vec<crate::config_3d::bone_physics::BonePhysicsSnapshot>,
    },
    /// Entidad(es) vinculada(s) a un socket.
    #[serde(rename = "entity_socket_attached")]
    EntitySocketAttached {
        host_id: u32,
        socket_name: String,
        child_ids: Vec<u32>,
    },
    /// Attachment de una entidad restaurado (undo/redo de vínculo a socket).
    #[serde(rename = "entity_attachment_restored")]
    EntityAttachmentRestored {
        child_id: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        attach_parent_id: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        attach_socket_host_id: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        attach_socket_name: Option<String>,
    },
    /// Vista del personaje jugable (pies, cámara y transform del mesh).
    #[serde(rename = "play_character_view_changed")]
    PlayCharacterViewChanged {
        #[serde(skip_serializing_if = "Option::is_none")]
        player_id: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        editor_camera_id: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        editor_orbit_target: Option<[f32; 3]>,
        /// Pies del Player (mundo). Legacy: el panel Cámara usaba esto como su POSITION
        /// (la cámara estaba acoplada al Player). Para la posición independiente del ojo
        /// de la cámara FPS, leer `camera_eye_position`.
        position: [f32; 3],
        /// Posición absoluta del ojo de la cámara FPS (independiente del Player).
        camera_eye_position: [f32; 3],
        /// Yaw/pitch del cono FPS en editor (`self.camera`), distinto del viewport orbital.
        fps_camera_yaw: f32,
        fps_camera_pitch: f32,
        yaw: f32,
        pitch: f32,
        fov_y: f32,
        frustum_distance: f32,
        camera_follow_mode: PlayCameraFollowMode,
        body_center: [f32; 3],
        body_rotation: [f32; 4],
        body_scale: [f32; 3],
        /// true solo tras `set_play_character_view` / panel Cámara; false tras `set_transform` del jugador.
        #[serde(default)]
        sync_editor_viewport: bool,
    },
    #[serde(rename = "graphics_texture_tier_changed")]
    GraphicsTextureTierChanged {
        tier: String,
    },
    #[serde(rename = "texture_detail_distance_changed")]
    TextureDetailDistanceChanged {
        distance_m: f32,
    },
    #[serde(rename = "reflection_tier_changed")]
    ReflectionTierChanged {
        tier: String,
    },
    #[serde(rename = "reflection_probes_changed")]
    ReflectionProbesChanged {
        enabled: bool,
    },
    #[serde(rename = "reflection_raytracing_changed")]
    ReflectionRaytracingChanged {
        enabled: bool,
    },
    /// Tier de reflejos pedido vs efectivo (p. ej. High degradado a Medium sin RT).
    #[serde(rename = "reflection_tier_effective")]
    ReflectionTierEffective {
        requested: String,
        effective: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rt_available: Option<bool>,
    },
    #[serde(rename = "shadow_tier_changed")]
    ShadowTierChanged {
        tier: String,
    },
    #[serde(rename = "msaa_tier_changed")]
    MsaaTierChanged {
        tier: String,
        sample_count: u32,
    },
    #[serde(rename = "reflection_debug_view_changed")]
    ReflectionDebugViewChanged {
        view: String,
    },
    #[serde(rename = "ssr_debug_mode_changed")]
    SsrDebugModeChanged {
        enabled: bool,
    },
    #[serde(rename = "taa_changed")]
    TaaChanged {
        enabled: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blend: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        jitter_scale: Option<f32>,
    },
    /// Emitido ~1 vez por segundo con métricas de rendimiento del motor.
    DebugMetrics {
        fps: f32,
        frame_time_ms: f32,
        draw_calls: u32,
        physics_bodies: u32,
        cpu_percent: f32,
        #[serde(skip_serializing_if = "Option::is_none")]
        gpu_percent: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        play_character_position: Option<[f32; 3]>,
        #[serde(skip_serializing_if = "Option::is_none")]
        play_character_yaw: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        play_character_pitch: Option<f32>,
    },
    /// Emitido cuando el preview cambia de estado desde el motor.
    PreviewPlayingChanged {
        playing: bool,
    },
    /// Cuadro de texto HUD creado en edición de UI.
    PlayerUiTextBoxAdded {
        id: u32,
        font_path: String,
        font_name: String,
        text: String,
        center_x: f32,
        center_y: f32,
        width: f32,
        height: f32,
    },
    PlayerUiTextBoxUpdated {
        id: u32,
        text: String,
    },
    PlayerUiTextBoxRemoved {
        id: u32,
    },
    /// Lista de cuadros de la pantalla UI al entrar en edición (sincroniza sidebar).
    PlayerUiTextBoxesList {
        scope: String,
        screen_id: String,
        boxes: Vec<PlayerUiTextBoxListItem>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        buttons: Vec<PlayerUiButtonListItem>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<PlayerUiImageListItem>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        objects: Vec<PlayerUiObjectListItem>,
    },
    PlayerUiButtonAdded {
        id: u32,
        text: String,
        font_name: String,
    },
    PlayerUiButtonRemoved {
        id: u32,
    },
    PlayerUiImageAdded {
        id: u32,
        image_name: String,
    },
    PlayerUiImageRemoved {
        id: u32,
    },
    PlayerUiObjectAdded {
        id: u32,
        vertex_count: u32,
    },
    PlayerUiObjectRemoved {
        id: u32,
    },
    /// Dibujo de objeto HUD cancelado (Esc o comando desde el editor).
    PlayerUiObjectDrawEnded,
    /// Pantalla Player UI activa cambiada (editor o script Rhai).
    PlayerUiActiveScreenChanged {
        #[serde(skip_serializing_if = "Option::is_none")]
        screen_id: Option<String>,
    },
    /// Emitido cada 5 minutos cuando el autosave está activo.
    AutosaveTick,
    /// Respuesta a `export_save_snapshot`: escena activa lista para el `.save`.
    SaveSnapshotReady {
        scene: Box<SaveSceneSnapshotPayload>,
    },
    /// Respuesta a `get_default_scene_name`.
    DefaultSceneNameReady {
        id: u32,
        name: String,
    },
    /// Escena creada en el registro del motor.
    EditorSceneCreated {
        id: u32,
        name: String,
        scenes: Vec<EditorSceneListItem>,
    },
    /// Cambio de escena permitido.
    EditorSceneSwitched {
        active_scene_id: u32,
        scenes: Vec<EditorSceneListItem>,
    },
    /// Cambio bloqueado (escena activa modificada vs baseline).
    EditorSceneSwitchBlocked {
        reason: String,
        active_scene_id: u32,
        target_scene_id: u32,
    },
    /// Lista de escenas del editor (registro sincronizado con el motor).
    EditorScenesUpdated {
        active_scene_id: u32,
        scenes: Vec<EditorSceneListItem>,
        /// `project_load` | `boot` | `scene_deleted` | `project_saved` | `sync`
        update_reason: String,
    },
}

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

/// Manifest de blueprint activa en construcción rápida (`docs/Blueprints_Model_3D.yaml`).
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

/// Escribe `project_loaded_3d` en stdout (claves camelCase = `ProjectLoaded3dPayload` en TS).
pub fn send_project_loaded_3d_event(payload: &ProjectLoaded3dEvent) {
    if let Ok(json) = serde_json::to_string(payload) {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        let _ = writeln!(handle, "{json}");
        let _ = handle.flush();
    }
}

/// Fin de carga 3D desde `extract_dir` (el front sincroniza con snapshot).
pub fn send_project_load_3d_complete_event() {
    if let Ok(json) = serde_json::to_string(&serde_json::json!({
        "event": "project_load_3d_complete",
    })) {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        let _ = writeln!(handle, "{json}");
        let _ = handle.flush();
    }
}

/// Progreso humano de `load_proyect` (stdout + panel del editor).
pub fn send_load_progress(message: &str, step_ms: Option<u64>, total_ms: Option<u64>) {
    send_event(&EngineEvent::LoadProgress {
        message: message.to_string(),
        step_ms,
        total_ms,
    });
}

/// Escribe un evento JSON en stdout y lo flushea inmediatamente.
pub fn send_event(event: &EngineEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        let _ = writeln!(handle, "{json}");
        let _ = handle.flush();
    }
}

/// Escribe un evento JSON a stderr con prefijo `[IPC_EVENT]` para que el handler de
/// Electron lo reconozca y lo reenvíe al renderer sin mostrar en la terminal.
/// Útil para eventos ruidosos como `entity_bones_list` / `entity_bone_physics_list`.
pub fn send_event_silent(event: &EngineEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        let stderr = io::stderr();
        let mut handle = stderr.lock();
        let _ = writeln!(handle, "[IPC_EVENT]{json}");
        let _ = handle.flush();
    }
}

/// Lanza un hilo dedicado que lee stdin línea a línea y envía
/// los comandos parseados al event loop del motor vía EventLoopProxy.
/// El proxy despierta el event loop inmediatamente (sin esperar el siguiente frame),
/// lo que elimina la latencia de hasta 16 ms del canal mpsc + WaitUntil.
pub fn start_ipc_thread(proxy: EventLoopProxy<EngineCommand>) {
    thread::Builder::new()
        .name("ipc-stdin".into())
        .spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(line) if !line.trim().is_empty() => {
                        match serde_json::from_str::<EngineCommand>(&line) {
                            Ok(cmd) => {
                                if proxy.send_event(cmd).is_err() {
                                    break; // El event loop cerró el proxy
                                }
                            }
                            Err(e) => eprintln!("[ipc] parse error: {e}"),
                        }
                    }
                    Err(_) => break, // stdin cerrado
                    _ => {}
                }
            }
        })
        .expect("No se pudo crear el hilo IPC");
}
