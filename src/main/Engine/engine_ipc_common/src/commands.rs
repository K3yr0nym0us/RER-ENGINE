use serde::Deserialize;

use crate::types::{
    AddPlayerUiButtonPayload, AnimScriptData, AnimationFrameData, ControlBindingsData,
    EntityRestoreAnimation, EntityRestorePhysics, EntityRestoreScript, EntityRestoreTransform,
    PlayerUiScreenInfo,
};

fn default_clip_loop() -> bool {
    true
}

/// Comandos IPC presentes en `engine_2d` y `engine_3d` (mismo JSON `cmd`).
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum EngineCommandCommon {
    Ping,
    Shutdown,
    SetClearColor { r: f64, g: f64, b: f64 },
    Resize { width: u32, height: u32 },
    SetBounds {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
        #[serde(default)]
        offset_x: Option<i32>,
        #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
        #[serde(default)]
        offset_y: Option<i32>,
    },
    LoadModel {
        path: String,
        #[serde(default)]
        single_instance: Option<bool>,
        #[serde(default)]
        entity_category: Option<String>,
        #[serde(default)]
        kind: Option<String>,
    },
    ReplaceEntityModel { id: u32, path: String },
    SetTransform {
        id: u32,
        #[serde(default)]
        position: Option<[f32; 3]>,
        #[serde(default)]
        position_axis: Option<AxisValue>,
        #[serde(default)]
        rotation: Option<[f32; 4]>,
        #[serde(default)]
        scale: Option<[f32; 3]>,
        #[serde(default)]
        scale_axis: Option<AxisValue>,
        #[serde(default)]
        track_undo: Option<bool>,
        #[serde(default)]
        body_rotation_only: Option<bool>,
        #[serde(default)]
        rotation_euler_delta: Option<RotationEulerDelta>,
        #[serde(default)]
        rotation_euler_degrees: Option<[f32; 3]>,
    },
    SetEntityName {
        id:   u32,
        name: String,
        #[serde(default)]
        force: bool,
    },
    SetScene {
        scene: String,
        #[serde(default)]
        save_path: Option<String>,
    },
    RemoveEntity { id: u32 },
    DeselectEntity,
    SetWorldSize {
        width: f32,
        height: f32,
        #[serde(default)]
        depth: Option<f32>,
    },
    SetGravity { gravity: f32 },
    SetGridVisible { visible: bool },
    SetGridCellSize { size: f32 },
    SetTargetFps { fps: u64 },
    SetCtrlHeld { held: bool },
    SetPhysics {
        id: u32,
        enabled: bool,
        body_type: String,
    },
    SetActiveTool {
        tool: String,
        #[serde(default)]
        preview_path: Option<String>,
        #[serde(default)]
        preview_kind: Option<String>,
        #[serde(default)]
        preview_scale: Option<[f32; 3]>,
        #[serde(default)]
        preview_src_rect: Option<[u32; 4]>,
        #[serde(default)]
        preview_rotation: Option<[f32; 4]>,
        #[serde(default)]
        preview_name: Option<String>,
        #[serde(default)]
        preview_physics_enabled: Option<bool>,
        #[serde(default)]
        preview_physics_type: Option<String>,
        #[serde(default)]
        preview_entity_category: Option<String>,
        #[serde(default)]
        preview_blueprint_id: Option<String>,
        #[serde(default)]
        preview_blueprint: Option<serde_json::Value>,
    },
    PlayAudio { path: String, loop_: bool },
    StopAudio,
    SetAnimation {
        id:         u32,
        name:       String,
        frames:     Vec<AnimationFrameData>,
        fps:        u32,
        loop_:      bool,
        #[serde(default)]
        flip_horizontal: bool,
        audio_path: Option<String>,
        #[serde(default)]
        logical_w:  Option<u32>,
        #[serde(default)]
        logical_h:  Option<u32>,
        #[serde(default)]
        scripts:    Vec<AnimScriptData>,
        #[serde(default)]
        is_cancelable: bool,
    },
    RemoveAnimation { id: u32, name: String },
    SetDefaultAnimation { id: u32, name: String },
    PlayAnimation {
        id: u32,
        name: String,
        #[serde(default = "default_clip_loop", alias = "loop")]
        loop_: bool,
    },
    StopAnimation { id: u32 },
    QueryEntityAnimationPlayState { entity_id: u32 },
    LoadScript { id: u32, path: String, source: String },
    RunControlScript {
        id: u32,
        control_key: String,
        path: String,
        source: String,
    },
    SetControlBindings { id: u32, bindings: ControlBindingsData },
    UnloadScript { id: u32 },
    LoadSceneVisualScript { scene_id: u32, source: String },
    LoadSprite { path: String, name: String },
    RemoveSprite { path: String },
    GetSpritesList,
    LoadSound { path: String, name: String },
    RemoveSound { path: String },
    GetSoundsList,
    LoadFont { path: String, name: String },
    RemoveFont { path: String },
    GetFontsList,
    LoadHudImage { path: String, name: String },
    RemoveHudImage { path: String },
    GetHudImagesList,
    LoadBackgroundAsset { path: String, name: String },
    RemoveBackgroundAsset { path: String },
    GetBackgroundsList,
    SetDebugMode { show: bool },
    SetPreviewPlaying { playing: bool },
    SetPlayerUiEditMode {
        active: bool,
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        screen_id: Option<String>,
    },
    AddPlayerUiTextBox { font_path: String },
    RemovePlayerUiTextBox {
        #[serde(default)]
        id: Option<u32>,
    },
    AddPlayerUiButton {
        #[serde(flatten)]
        payload: AddPlayerUiButtonPayload,
    },
    RemovePlayerUiButton {
        #[serde(default)]
        id: Option<u32>,
    },
    AddPlayerUiImage { image_path: String },
    RemovePlayerUiImage {
        #[serde(default)]
        id: Option<u32>,
    },
    SetPlayerUiObjectDraw { active: bool },
    RemovePlayerUiObject {
        #[serde(default)]
        id: Option<u32>,
    },
    SetPlayerUiHudElementProps {
        element_kind: String,
        id: u32,
        #[serde(default)]
        locked: Option<bool>,
        #[serde(default)]
        z_index: Option<i32>,
    },
    SetPlayerUiObjectStyle {
        id: u32,
        #[serde(default)]
        fill_color: Option<[f32; 4]>,
        #[serde(default)]
        texture_path: Option<String>,
        #[serde(default)]
        clear_texture: bool,
        #[serde(default)]
        live: bool,
        #[serde(default, alias = "skipUndo")]
        skip_undo: bool,
    },
    SyncPlayerUiScreens {
        screens: Vec<PlayerUiScreenInfo>,
    },
    SetActivePlayerUiScreen {
        #[serde(default)]
        screen_id: Option<String>,
    },
    Undo,
    Redo,
    ReloadAsset { path: String },
    SetLocale { locale: String },
    SetAutosave { enabled: bool },
    ExportSaveSnapshot,
    GetDefaultSceneName { id: u32 },
    ResendAllModelClips,
    ApplyEntityRestore {
        id: u32,
        #[serde(default)]
        name: Option<String>,
        transform: EntityRestoreTransform,
        #[serde(default)]
        physics: Option<EntityRestorePhysics>,
        #[serde(default)]
        animations: Option<Vec<EntityRestoreAnimation>>,
        #[serde(default)]
        scripts: Option<Vec<EntityRestoreScript>>,
        #[serde(default)]
        control_bindings: Option<ControlBindingsData>,
        #[serde(default)]
        omit_scale: bool,
        #[serde(default)]
        skip_transform: bool,
        #[serde(default)]
        apply_initial_animation_frame: Option<bool>,
    },
}

/// Delta de rotación Euler (grados) — usado por `set_transform` en 3D; ignorado en 2D.
#[derive(Debug, Deserialize, Clone)]
pub struct RotationEulerDelta {
    pub axis: u8,
    pub degrees: f32,
}

/// Un eje de posición/escala — usado por `set_transform` en 3D; ignorado en 2D.
#[derive(Debug, Deserialize, Clone)]
pub struct AxisValue {
    pub axis: u8,
    pub value: f32,
}
