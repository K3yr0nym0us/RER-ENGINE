use std::{
    io::{self, BufRead, Write},
    thread,
};

use winit::event_loop::EventLoopProxy;

use serde::{Deserialize, Serialize};

pub use crate::engine_command::{EngineCommand, EngineCommand2dOnly};
pub use rer_engine_ipc_common::{
    AddPlayerUiButtonPayload, AnimScriptData, AnimationFrameData, ControlBindingsData,
    ControlScriptData, EngineCommandCommon, EntityRestoreAnimation, EntityRestorePhysics,
    EntityRestoreScript, EntityRestoreTransform, PlayerUiScreenInfo,
};

// ---------------------------------------------------------------------------
// Tipos y eventos exclusivos del motor 2D (comandos comunes -> engine_ipc_common)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
pub struct ProjectLoaded2dSceneTab {
    pub id: u32,
    pub name: String,
}

/// Cámara en `project_loaded_2d` — mismas claves que `types.ts`.
#[allow(non_snake_case)]
#[derive(Debug, Serialize, Clone)]
pub struct ProjectLoaded2dCamera2d {
    pub x: f32,
    pub y: f32,
    pub halfH: f32,
}

/// Mundo en `project_loaded_2d` — mismas claves que `SavedWorldConfig` en `types.ts`.
#[allow(non_snake_case)]
#[derive(Debug, Serialize, Clone)]
pub struct ProjectLoaded2dWorld {
    pub worldWidth: f32,
    pub worldHeight: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worldDepth: Option<f32>,
    pub gridVisible: bool,
    pub gridCellSize: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gravity: Option<f32>,
    pub targetFps: f64,
}

/// Evento `project_loaded_2d` (fuera de `EngineEvent` para no heredar snake_case del enum).
#[allow(non_snake_case)]
#[derive(Debug, Serialize)]
pub struct ProjectLoaded2dEvent {
    pub event: &'static str,
    pub activeSceneId: u32,
    pub sceneName: String,
    pub entityCount: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenes: Vec<ProjectLoaded2dSceneTab>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub sprites: Vec<ImportSceneSprite>,
    pub sounds: Vec<ImportSceneSprite>,
    pub fonts: Vec<ImportSceneSprite>,
    pub backgrounds: Vec<ImportSceneSprite>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hudImages: Vec<HudImageInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub playerUiScreens: Vec<PlayerUiScreenInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menuUiScreens: Vec<SaveUiScreenSnapshot>,
    pub blueprints: serde_json::Value,
    pub world: ProjectLoaded2dWorld,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backgroundPath: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera2d: Option<ProjectLoaded2dCamera2d>,
}

/// Escribe `project_loaded_2d` en stdout (claves camelCase = `ProjectLoaded2dPayload` en TS).
pub fn send_project_loaded_2d_event(payload: &ProjectLoaded2dEvent) {
    if let Ok(json) = serde_json::to_string(payload) {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        let _ = writeln!(handle, "{json}");
        let _ = handle.flush();
    }
}

#[derive(Debug, Deserialize)]
pub struct ImportSceneWorld {
    pub world_width: f32,
    pub world_height: f32,
    #[serde(default)]
    pub grid_visible: bool,
    pub grid_cell_size: f32,
    #[serde(default)]
    pub gravity: Option<f32>,
    pub target_fps: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportSceneCamera2d {
    pub x: f32,
    pub y: f32,
    pub half_h: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportSceneSprite {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct ImportSceneEntity {
    pub id: u32,
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub name: Option<String>,
    pub transform: EntityRestoreTransform,
    #[serde(default)]
    pub physics: Option<EntityRestorePhysics>,
    #[serde(default)]
    pub animations: Option<Vec<EntityRestoreAnimation>>,
    #[serde(default)]
    pub scripts: Option<Vec<EntityRestoreScript>>,
    #[serde(default)]
    pub control_bindings: Option<ControlBindingsData>,
    #[serde(default)]
    pub points: Option<[[f32; 2]; 4]>,
    #[serde(default)]
    pub omit_scale: bool,
    #[serde(default)]
    pub skip_transform: bool,
    #[serde(default)]
    pub apply_initial_animation_frame: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ImportScenePayload {
    #[serde(default = "default_import_scene_name")]
    pub scene: String,
    pub world: ImportSceneWorld,
    #[serde(default)]
    pub background_path: Option<String>,
    #[serde(default)]
    pub camera2d: Option<ImportSceneCamera2d>,
    #[serde(default)]
    pub sprites: Vec<ImportSceneSprite>,
    pub entities: Vec<ImportSceneEntity>,
}

fn default_import_scene_name() -> String {
    "2D".to_string()
}

/// Datos de transformación de una entidad para eventos de multiselección
#[derive(Debug, Serialize, Clone)]
pub struct EntityTransformUpdate {
    pub id: u32,
    pub position: [f32; 3],
    pub rotation: [f32; 4], // quaternion xyzw
    pub scale: [f32; 3],
}

// ── Instantánea de guardado (motor → Electron) ───────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct SaveWorldSnapshot {
    pub world_width: f32,
    pub world_height: f32,
    pub world_depth: f32,
    pub grid_visible: bool,
    pub grid_cell_size: f32,
    pub gravity: f32,
    pub target_fps: u64,
}

#[derive(Debug, Serialize, Clone)]
pub struct SaveScriptSnapshot {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Serialize, Clone)]
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

#[derive(Debug, Serialize, Clone)]
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
}

#[derive(Debug, Serialize, Clone)]
pub struct SaveEntitySnapshot {
    pub id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub kind: String,
    pub path: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physics_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physics_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points: Option<[[f32; 2]; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animations: Option<Vec<SaveAnimationSnapshot>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scripts: Option<Vec<SaveScriptSnapshot>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_bindings: Option<ControlBindingsData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visual_model_path: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SavePlayerTransformSnapshot {
    pub position: [f32; 3],
    pub scale: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y: f32,
    pub frustum_distance: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visual_model_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_bindings: Option<ControlBindingsData>,
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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SaveUiScreenSnapshot {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub active: bool,
}

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
    crate::config_2d::player_ui::object::DEFAULT_OBJECT_FILL
}

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
    pub entities: Vec<SaveEntitySnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_transform: Option<SavePlayerTransformSnapshot>,
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
pub enum EngineEvent {
    Ready {
        gravity: f32,
    },
    Pong,
    Error {
        message: String,
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
        img_width: u32,
        img_height: u32,
        default_pivot_x: f32,
        default_pivot_y: f32,
    },
    /// Emitido cuando un personaje PNG se cargó correctamente.
    CharacterLoaded {
        id: u32,
        path: String,
        img_width: u32,
        img_height: u32,
        default_pivot_x: f32,
        default_pivot_y: f32,
    },
    /// Emitido al terminar `import_scene` (carga atómica de escena 2D).
    SceneImported {
        entity_count: u32,
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
    /// Emitido mientras el usuario está colocando puntos con una herramienta de dibujo.
    DrawingProgress {
        count: u32,
    },
    /// Emitido cuando se creó un colisionador de 4 puntos.
    ColliderCreated {
        id: u32,
        points: [[f32; 2]; 4],
    },
    /// Emitido cuando se creó un área de ejecución de 4 puntos.
    ExecutionAreaCreated {
        id: u32,
        points: [[f32; 2]; 4],
    },
    /// Emitido cuando una herramienta de dibujo fue cancelada desde el motor.
    ToolCancelled,
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
    PlayerUiObjectDrawEnded,
    PlayerUiActiveScreenChanged {
        #[serde(skip_serializing_if = "Option::is_none")]
        screen_id: Option<String>,
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
    /// Emitido cuando el cursor se mueve y la herramienta quick_build_place está activa.
    QuickBuildMove {
        x: f32,
        y: f32,
    },
    /// Emitido cuando el usuario hace click con la herramienta quick_build_place activa.
    /// `fit_to_grid` indica si Ctrl estaba presionado al colocar.
    /// `scale` contiene el tamaño final resuelto por el motor para esta colocación.
    QuickBuildClick {
        x: f32,
        y: f32,
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
        #[serde(skip_serializing_if = "Option::is_none")]
        points: Option<[[f32; 2]; 4]>,
    },
    /// Emitido cuando el usuario mantiene Ctrl y hace click añadiendo/quitando entidades
    /// a la selección múltiple. `ids` contiene todos los IDs actualmente seleccionados.
    MultiSelectChanged {
        ids: Vec<u32>,
    },
    /// Emitido durante el arrastre en multiselección, contiene todas las transformaciones
    /// actualizadas de las entidades seleccionadas. Permite sincronizar entityTransformsRef
    /// en el frontend para guardar correctamente.
    MultiSelectionTransformed {
        entities: Vec<EntityTransformUpdate>,
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
    /// Emitido cada 5 minutos cuando el autosave está activo.
    AutosaveTick,
    /// Atlas de sprites 2D sin espacio para empacar la imagen solicitada (una vez por llenado).
    AtlasExhausted {
        atlas_size: u32,
        width: u32,
        height: u32,
    },
    /// Respuesta a `export_save_snapshot`: escena activa lista para el `.save`.
    SaveSnapshotReady {
        scene: Box<SaveSceneSnapshotPayload>,
    },
    /// Respuesta a `get_default_scene_name`.
    DefaultSceneNameReady {
        id: u32,
        name: String,
    },
    /// Progreso humano de `load_proyect` (stdout + panel del editor).
    #[serde(rename = "load_progress")]
    LoadProgress {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        step_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        total_ms: Option<u64>,
    },
}

/// Información básica de un sprite almacenado en el motor.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SpriteInfo {
    pub path: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HudImageInfo {
    pub path: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
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

/// Información básica de un fondo almacenado en el motor.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BackgroundInfo {
    pub path: String,
    pub name: String,
}

/// Fin de carga 2D desde `extract_dir` (el front sincroniza con snapshot).
pub fn send_project_load_2d_complete_event() {
    if let Ok(json) = serde_json::to_string(&serde_json::json!({
        "event": "project_load_2d_complete",
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
