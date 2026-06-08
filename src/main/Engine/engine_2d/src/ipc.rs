use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    thread,
};

use winit::event_loop::EventLoopProxy;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Comandos que Electron envía al motor (stdin → motor)
// ---------------------------------------------------------------------------
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum EngineCommand {
    Ping,
    Shutdown,
    SetClearColor { r: f64, g: f64, b: f64 },
    Resize { width: u32, height: u32 },
    SetBounds { x: i32, y: i32, width: u32, height: u32,
        /// Offset físico (en píxeles de pantalla) del EngineView dentro del área de
        /// contenido de Electron. Calculado en el renderer como `rect * devicePixelRatio`,
        /// sin la conversión DPI de getContentBounds() que puede ser inexacta en monitores
        /// secundarios. El position-tracker Win32 lo usa como offset directo.
        #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
        #[serde(default)] offset_x: Option<i32>,
        #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
        #[serde(default)] offset_y: Option<i32>,
    },
    LoadModel { path: String },
    ReplaceEntityModel { id: u32, path: String },
    /// Actualizar transform de una entidad por id.
    SetTransform {
        id:       u32,
        position: Option<[f32; 3]>,
        rotation: Option<[f32; 4]>,  // quaternion xyzw
        scale:    Option<[f32; 3]>,
        /// Controla si el cambio se registra en historial Undo/Redo.
        /// None/true: registrar (acciones de usuario). false: no registrar (restore/carga).
        #[serde(default)]
        track_undo: Option<bool>,
    },
    /// Cambiar el nombre de una entidad por id.
    /// `force`: si es true, omite la validación de nombre duplicado (usado en restore de proyecto).
    SetEntityName {
        id:   u32,
        name: String,
        #[serde(default)]
        force: bool,
    },
    /// Cambiar la escena activa del binario 2D (por ejemplo: "2D" o "scratch").
    SetScene {
        scene: String,
        #[serde(default)]
        save_path: Option<String>,
    },
    /// Cargar una imagen PNG como escenario de fondo en la escena 2D.
    LoadScenario {
        path: String,
        /// Si es true, registra la creación en el historial de deshacer.
        /// None/false: no registrar (carga inicial, restore de proyecto).
        #[serde(default)]
        track_undo: Option<bool>,
    },
    /// Ajustar la escala de un escenario 2D específico preservando proporciones.
    SetScenarioScale { id: u32, scale: f32 },
    /// Cargar una imagen PNG como personaje en la escena 2D.
    LoadCharacter {
        path: String,
        /// Si es true, registra la creación en el historial de deshacer.
        #[serde(default)]
        track_undo: Option<bool>,
    },
    /// Ajustar la escala de un personaje 2D específico preservando proporciones.
    SetCharacterScale { id: u32, scale: f32 },
        /// Limpiar el fondo del mundo 2D actual.
        ClearBackground,
/// Cambiar el sprite de una entidad (escenario o personaje) a un frame de animación.
    /// pivot_x/pivot_y: punto ancla en píxeles dentro del frame (0,0 = esquina superior-izq).
    /// logical_w/logical_h: bounding box lógico fijo de la animación (en píxeles).
    PlayAnimationFrame {
        id:        u32,
        path:      String,
        /// Si falta, el motor usa centro X y base Y del rect (`src_*` o `logical_*`).
        #[serde(default)]
        pivot_x:   Option<f32>,
        #[serde(default)]
        pivot_y:   Option<f32>,
        logical_w: u32,
        logical_h: u32,
        #[serde(default)]
        src_x:     Option<u32>,
        #[serde(default)]
        src_y:     Option<u32>,
        #[serde(default)]
        src_w:     Option<u32>,
        #[serde(default)]
        src_h:     Option<u32>,
    },
    /// Restaurar el sprite original de una entidad después de una animación.
    RestoreAnimationFrame { id: u32 },
    /// Eliminar una entidad de la escena por su ID.
    RemoveEntity { id: u32 },
    /// Deseleccionar la entidad activa (limpia gizmo y multi-selección).
    DeselectEntity,
    /// Definir el tamaño del área de trabajo del mundo (unidades de mundo).
    SetWorldSize { width: f32, height: f32 },
    /// Cambiar la gravedad del mundo físico (valor Y negativo = hacia abajo).
    SetGravity { gravity: f32 },
    /// Mostrar u ocultar la cuadrícula del mundo.
    SetGridVisible { visible: bool },
    /// Cambiar el tamaño de cada celda de la cuadrícula.
    SetGridCellSize { size: f32 },
    /// Cambiar el límite de FPS del loop principal.
    SetTargetFps { fps: u64 },
    /// Estado de la tecla Ctrl enviado desde Electron (ventana overlay no recibe teclado directo).
    SetCtrlHeld { held: bool },
    /// Restaurar posición y zoom de la cámara 2D ortográfica.
    SetCamera2d { x: f32, y: f32, half_h: f32 },
    /// Cargar una imagen PNG/GIF como fondo de mundo (cubre todo el área del mundo).
    LoadBackground { path: String },
    /// Activar o desactivar física en una entidad. body_type: "dynamic" | "static" | "kinematic"
    SetPhysics { id: u32, enabled: bool, body_type: String },
    /// Activar una herramienta de dibujo. tool: "draw_collider" | "draw_execution_area" | "" (cancelar)
    SetActiveTool {
        tool: String,
        /// Path al sprite del blueprint a previsualizar como entidad fantasma.
        #[serde(default)]
        preview_path: Option<String>,
        /// Tipo del blueprint ("scenario" | "character") para elegir cómo cargarlo.
        #[serde(default)]
        preview_kind: Option<String>,
        /// Escala del blueprint [x, y, z] en unidades de mundo.
        #[serde(default)]
        preview_scale: Option<[f32; 3]>,
        /// Rectángulo opcional de recorte [x, y, w, h] dentro de `preview_path`.
        /// Se usa para mostrar solo el frame inicial cuando el blueprint viene de spritesheet.
        #[serde(default)]
        preview_src_rect: Option<[u32; 4]>,
    },
    /// Recrear un colisionador de 4 puntos desde datos guardados (restauración de proyecto).
    CreateColliderFromPoints {
        points: [[f32; 2]; 4],
        /// true/None: registrar en undo; false: no registrar (carga/restore).
        #[serde(default)]
        track_undo: Option<bool>,
    },
    /// Crear un área de ejecución de 4 puntos (trigger sin colisión física).
    CreateExecutionAreaFromPoints {
        points: [[f32; 2]; 4],
        /// true/None: registrar en undo; false: no registrar (carga/restore).
        #[serde(default)]
        track_undo: Option<bool>,
    },
    /// Activar modo edición de pivot: muestra el frame en la entidad y captura el siguiente click.
    /// pivot_x/pivot_y: coordenadas del pivot ya asignado (para mostrarlo visualmente).
    SetPivotEditMode { id: u32, frame_path: String, pivot_x: f32, pivot_y: f32 },
    /// Cancelar modo edición de pivot y restaurar el sprite original.
    CancelPivotEditMode,
    /// Mostrar el borde del área lógica de una entidad (w×h píxeles).
    SetLogicalAreaMode { id: u32, w: u32, h: u32 },
    /// Ocultar el borde del área lógica.
    CancelLogicalAreaMode,
    /// Reproducir un archivo de audio (wav/ogg/mp3). loop_: true para repetir indefinidamente.
    PlayAudio { path: String, loop_: bool },
    /// Detener el audio que está sonando actualmente.
    StopAudio,
    /// Guardar una animación en el motor para reproducción posterior.
    SetAnimation {
        id:         u32,
        name:       String,
        frames:     Vec<AnimationFrameData>,
        fps:        u32,
        loop_:      bool,
        #[serde(default)]
        flip_horizontal: bool,
        audio_path: Option<String>,
        /// Tamaño lógico opcional enviado por el front.
        /// Si falta, el motor lo normaliza automáticamente usando los frames
        /// y la referencia de las animaciones existentes de la entidad.
        #[serde(default)]
        logical_w:  Option<u32>,
        #[serde(default)]
        logical_h:  Option<u32>,
        /// Scripts Rhai que se ejecutan mientras esta animación está activa.
        #[serde(default)]
        scripts:    Vec<AnimScriptData>,
        /// Si false (default), ninguna otra animación puede interrumpirla antes de que termine.
        #[serde(default)]
        is_cancelable: bool,
    },
    /// Eliminar una animación del motor por ID de entidad y nombre.
    RemoveAnimation { id: u32, name: String },
    /// Definir la animación predeterminada de una entidad.
    SetDefaultAnimation { id: u32, name: String },
    /// Reproducir una animación guardada por ID de entidad y nombre.
    /// El motor busca en su almacén de animaciones — el front no necesita
    /// reenviar los datos de frames en cada reproducción.
    PlayAnimation { id: u32, name: String },
    /// Detener la animación en curso.
    StopAnimation { id: u32 },
    /// Adjuntar un script Rhai a una entidad. `source` es el código Rhai completo.
    /// `path` se usa solo para mensajes de error y logs.
    LoadScript { id: u32, path: String, source: String },
    /// Ejecutar script de control para una entidad (trigger en runtime por input).
    /// Se procesa solo en modo de juego.
    RunControlScript { id: u32, control_key: String, path: String, source: String },
    /// Configurar bindings de runtime por entidad. El frontend solo sincroniza la
    /// configuración; la detección y ejecución de inputs ocurre dentro del motor.
    SetControlBindings { id: u32, bindings: ControlBindingsData },
    /// Desadjuntar todos los scripts de una entidad (sin eliminar la entidad).
    UnloadScript { id: u32 },
    /// Cargar script Rhai compilado desde el editor de nodos (lógica por escena).
    LoadSceneVisualScript { scene_id: u32, source: String },
    /// Cargar una imagen PNG como sprite (solo almacenamiento, no se renderiza).
    LoadSprite { path: String, name: String },
    /// Eliminar un sprite del almacén del motor.
    RemoveSprite { path: String },
    /// Solicitar la lista de sprites cargados en el motor.
    GetSpritesList,
    /// Registrar un archivo de audio en el almacén del motor (nombre → ruta).
    LoadSound { path: String, name: String },
    /// Eliminar un sonido del almacén del motor.
    RemoveSound { path: String },
    /// Solicitar la lista de sonidos cargados en el motor.
    GetSoundsList,
    /// Registrar un archivo de fuente en el almacén del motor (nombre → ruta).
    LoadFont { path: String, name: String },
    /// Eliminar una fuente del almacén del motor.
    RemoveFont { path: String },
    /// Solicitar la lista de fuentes cargadas en el motor.
    GetFontsList,
    /// Registrar una imagen como fondo en el almacén del motor (nombre → ruta).
    LoadBackgroundAsset { path: String, name: String },
    /// Eliminar un fondo del almacén del motor.
    RemoveBackgroundAsset { path: String },
    /// Solicitar la lista de fondos cargados en el motor.
    GetBackgroundsList,
    /// Alternar modo de prueba del juego: true = simular juego, false = modo editor.
    SetPreviewPlaying { playing: bool },
    /// Deshacer la última acción disponible.
    Undo,
    /// Rehacer la última acción deshecha (si existe historial de redo).
    Redo,
    /// Recargar un asset PNG desde disco sin recrear entidades ni cambiar UVs.
    /// Electron lo envía cuando detecta que el archivo fue modificado externamente.
    ReloadAsset { path: String },
    /// Cambiar el locale del motor para seleccionar assets localizados (ej. imágenes de hint).
    /// locale: "en" | "es"
    SetLocale { locale: String },
    /// Activar/desactivar autosave coordinado por el motor.
    SetAutosave { enabled: bool },
    /// Pedir al motor la instantánea de la escena activa para persistir en `.save`.
    ExportSaveSnapshot,
    /// Nombre por defecto de escena del editor (`Scene-01`, …).
    GetDefaultSceneName { id: u32 },
    /// Mostrar colliders incluso en modo juego (debug).
    SetDebugMode { show: bool },
    /// Activar/desactivar V-Sync en el swapchain.
    SetVsync { enabled: bool },
    /// Restore post-carga de entidad 2D (transform, física, animaciones, scripts).
    /// Sustituye ráfagas de IPC desde el front al aplicar `pendingRestores`.
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
    /// Carga una escena 2D completa (reset + mundo + entidades + restores) en un solo IPC.
    ImportScene(ImportScenePayload),
}

#[derive(Debug, Serialize, Clone)]
pub struct ProjectLoaded2dSceneTab {
    pub id:   u32,
    pub name: String,
}

/// Cámara en `project_loaded_2d` — mismas claves que `types.ts`.
#[allow(non_snake_case)]
#[derive(Debug, Serialize, Clone)]
pub struct ProjectLoaded2dCamera2d {
    pub x:     f32,
    pub y:     f32,
    pub halfH: f32,
}

/// Mundo en `project_loaded_2d` — mismas claves que `SavedWorldConfig` en `types.ts`.
#[allow(non_snake_case)]
#[derive(Debug, Serialize, Clone)]
pub struct ProjectLoaded2dWorld {
    pub worldWidth:   f32,
    pub worldHeight:  f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worldDepth:   Option<f32>,
    pub gridVisible:  bool,
    pub gridCellSize: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gravity:      Option<f32>,
    pub targetFps:    f64,
}

/// Evento `project_loaded_2d` (fuera de `EngineEvent` para no heredar snake_case del enum).
#[allow(non_snake_case)]
#[derive(Debug, Serialize)]
pub struct ProjectLoaded2dEvent {
    pub event:          &'static str,
    pub activeSceneId:  u32,
    pub sceneName:      String,
    pub entityCount:    u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenes:         Vec<ProjectLoaded2dSceneTab>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language:       Option<String>,
    pub sprites:        Vec<ImportSceneSprite>,
    pub sounds:         Vec<ImportSceneSprite>,
    pub fonts:          Vec<ImportSceneSprite>,
    pub backgrounds:    Vec<ImportSceneSprite>,
    pub blueprints:     serde_json::Value,
    pub world:          ProjectLoaded2dWorld,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backgroundPath: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera2d:       Option<ProjectLoaded2dCamera2d>,
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
    pub world_width:   f32,
    pub world_height:  f32,
    #[serde(default)]
    pub grid_visible:  bool,
    pub grid_cell_size: f32,
    #[serde(default)]
    pub gravity:       Option<f32>,
    pub target_fps:    u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportSceneCamera2d {
    pub x:      f32,
    pub y:      f32,
    pub half_h: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportSceneSprite {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct ImportSceneEntity {
    pub id:   u32,
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

#[derive(Debug, Deserialize, Clone)]
pub struct AnimationFrameData {
    pub path:      String,
    /// Si falta, el motor usa centro horizontal y base inferior del rect del frame.
    #[serde(default)]
    pub pivot_x:   Option<f32>,
    #[serde(default)]
    pub pivot_y:   Option<f32>,
    #[serde(default)]
    pub src_x:     Option<u32>,
    #[serde(default)]
    pub src_y:     Option<u32>,
    #[serde(default)]
    pub src_w:     Option<u32>,
    #[serde(default)]
    pub src_h:     Option<u32>,
}

impl AnimationFrameData {
    /// Pivot por defecto en coords. del frame: centro X, base Y (solo si falta pivot explícito).
    pub fn resolved_pivot(&self, fallback_w: u32, fallback_h: u32) -> (f32, f32) {
        if let (Some(x), Some(y)) = (self.pivot_x, self.pivot_y) {
            return (x, y);
        }
        let w = self.src_w.unwrap_or(fallback_w).max(1) as f32;
        let h = self.src_h.unwrap_or(fallback_h).max(1) as f32;
        (w * 0.5, h)
    }
}

/// Script Rhai asociado a una animación. Se ejecuta solo mientras la animación está activa.
#[derive(Debug, Deserialize, Clone)]
pub struct AnimScriptData {
    pub name:   String,
    pub source: String,
}

/// Datos de transformación de una entidad para eventos de multiselección
#[derive(Debug, Serialize, Clone)]
pub struct EntityTransformUpdate {
    pub id:       u32,
    pub position: [f32; 3],
    pub rotation: [f32; 4],  // quaternion xyzw
    pub scale:    [f32; 3],
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ControlScriptData {
    pub name:   String,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ControlBindingsData {
    #[serde(default)]
    pub keyboard_mouse: HashMap<String, ControlScriptData>,
    #[serde(default)]
    pub gamepad: HashMap<String, ControlScriptData>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EntityRestoreTransform {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale:    [f32; 3],
}

#[derive(Debug, Deserialize, Clone)]
pub struct EntityRestorePhysics {
    pub enabled:   bool,
    pub body_type: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EntityRestoreAnimation {
    pub name:   String,
    pub frames: Vec<AnimationFrameData>,
    pub fps:    u32,
    pub loop_:  bool,
    #[serde(default)]
    pub flip_horizontal: bool,
    #[serde(default)]
    pub audio_path: Option<String>,
    #[serde(default)]
    pub scripts: Vec<AnimScriptData>,
    #[serde(default)]
    pub is_cancelable: bool,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EntityRestoreScript {
    pub path:   String,
    pub source: String,
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
}

// ---------------------------------------------------------------------------
// Eventos que el motor envía a Electron (motor → stdout)
// ---------------------------------------------------------------------------
#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EngineEvent {
    Ready { gravity: f32 },
    Pong,
    Error { message: String },
    /// Emitido cuando el usuario hace click izquierdo sobre una entidad.
    EntitySelected {
        id:              u32,
        name:            String,
        position:        [f32; 3],
        rotation:        [f32; 4],   // quaternion xyzw
        scale:           [f32; 3],
        physics_enabled: bool,
        physics_type:    String,
    },
    /// Emitido cuando el usuario hace click izquierdo en vacío.
    EntityDeselected,
    /// Emitido cuando el cursor pasa por encima de una entidad (solo cuando cambia).
    EntityHovered { id: u32 },
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
    SceneImported { entity_count: u32 },
    /// Emitido cuando la cámara 2D cambia (fin de pan o zoom).
    #[serde(rename = "camera_2d_updated")]
    Camera2dUpdated { x: f32, y: f32, half_h: f32 },
    /// Emitido cuando se cargó una imagen de fondo del mundo.
    BackgroundLoaded { path: String },
    /// Emitido mientras el usuario está colocando puntos con una herramienta de dibujo.
    DrawingProgress { count: u32 },
    /// Emitido cuando se creó un colisionador de 4 puntos.
    ColliderCreated { id: u32, points: [[f32; 2]; 4] },
    /// Emitido cuando se creó un área de ejecución de 4 puntos.
    ExecutionAreaCreated { id: u32, points: [[f32; 2]; 4] },
    /// Emitido cuando una herramienta de dibujo fue cancelada desde el motor.
    ToolCancelled,
    /// Emitido cuando el usuario selecciona el pivot de un frame en modo edición.
    PivotSelected { frame_path: String, pivot_x: f32, pivot_y: f32 },
    /// Emitido cuando una animación termina (no loop) o se detiene.
    AnimationFinished { entity_id: u32 },
    /// Emitido cuando el estado de física de una entidad cambia (activado/desactivado por script).
    PhysicsChanged { entity_id: u32, enabled: bool, body_type: String },
    /// Emitido cuando un sprite PNG se cargó correctamente en el almacén.
    SpriteLoaded { path: String, name: String, width: u32, height: u32 },
    /// Emitido cuando se eliminó un sprite del almacén.
    SpriteRemoved { path: String },
    /// Emitido como respuesta a GetSpritesList: lista de sprites disponibles.
    SpritesList { sprites: Vec<SpriteInfo> },
    /// Emitido cuando un archivo de audio se registró en el almacén.
    SoundLoaded { path: String, name: String },
    /// Emitido cuando se eliminó un sonido del almacén.
    SoundRemoved { path: String },
    /// Emitido como respuesta a GetSoundsList: lista de sonidos disponibles.
    SoundsList { sounds: Vec<SoundInfo> },
    /// Emitido cuando un archivo de fuente se registró en el almacén.
    FontLoaded { path: String, name: String },
    /// Emitido cuando se eliminó una fuente del almacén.
    FontRemoved { path: String },
    /// Emitido como respuesta a GetFontsList: lista de fuentes disponibles.
    FontsList { fonts: Vec<FontInfo> },
    /// Emitido cuando un fondo se registró en el almacén.
    BackgroundAssetLoaded { path: String, name: String },
    /// Emitido cuando se eliminó un fondo del almacén.
    BackgroundAssetRemoved { path: String },
    /// Emitido como respuesta a GetBackgroundsList: lista de fondos disponibles.
    BackgroundsList { backgrounds: Vec<BackgroundInfo> },
    /// Emitido cuando el cursor se mueve y la herramienta quick_build_place está activa.
    QuickBuildMove { x: f32, y: f32 },
    /// Emitido cuando el usuario hace click con la herramienta quick_build_place activa.
    /// `fit_to_grid` indica si Ctrl estaba presionado al colocar.
    /// `scale` contiene el tamaño final resuelto por el motor para esta colocación.
    QuickBuildClick { x: f32, y: f32, fit_to_grid: bool, scale: [f32; 3] },
    /// Emitido cuando SetAnimation resolvió/normalizó el tamaño lógico final.
    AnimationLogicalResolved { id: u32, name: String, logical_w: u32, logical_h: u32 },
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
    MultiSelectChanged { ids: Vec<u32> },
    /// Emitido durante el arrastre en multiselección, contiene todas las transformaciones
    /// actualizadas de las entidades seleccionadas. Permite sincronizar entityTransformsRef
    /// en el frontend para guardar correctamente.
    MultiSelectionTransformed { entities: Vec<EntityTransformUpdate> },
    /// Emitido ~1 vez por segundo con métricas de rendimiento del motor.
    DebugMetrics {
        fps:            f32,
        frame_time_ms:  f32,
        draw_calls:     u32,
        physics_bodies: u32,
        cpu_percent:    f32,
        #[serde(skip_serializing_if = "Option::is_none")]
        gpu_percent:    Option<f32>,
    },
    /// Emitido cuando un actor entra en un área de ejecución (trigger).
    TriggerEntered {
        trigger_id: u32,
        actor_id: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        has_attached_script: Option<bool>,
    },
    /// Emitido cuando un actor sale de un área de ejecución (trigger).
    TriggerExited { trigger_id: u32, actor_id: u32 },
    /// Emitido cada 5 minutos cuando el autosave está activo.
    AutosaveTick,
    /// Atlas de sprites 2D sin espacio para empacar la imagen solicitada (una vez por llenado).
    AtlasExhausted { atlas_size: u32, width: u32, height: u32 },
    /// Respuesta a `export_save_snapshot`: escena activa lista para el `.save`.
    SaveSnapshotReady { scene: SaveSceneSnapshotPayload },
    /// Respuesta a `get_default_scene_name`.
    DefaultSceneNameReady { id: u32, name: String },
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
    pub path:   String,
    pub name:   String,
    pub width:  u32,
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
