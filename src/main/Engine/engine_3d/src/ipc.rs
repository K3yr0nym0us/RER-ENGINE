use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    thread,
};

use winit::event_loop::EventLoopProxy;

use serde::{Deserialize, Serialize};

fn default_clip_loop() -> bool {
    true
}

/// Delta de rotación en grados sobre un eje Euler YXZ (convención `glam::EulerRot::YXZ`).
#[derive(Debug, Deserialize, Clone)]
pub struct RotationEulerDelta {
    /// 0 = pitch (X), 1 = yaw (Y), 2 = roll (Z).
    pub axis: u8,
    pub degrees: f32,
}

/// Cambio de un solo eje (posición o escala) sin pisar los demás. El motor lee el valor actual
/// y solo reemplaza el eje indicado — evita que el front envíe datos viejos.
#[derive(Debug, Deserialize, Clone)]
pub struct AxisValue {
    /// 0 = X, 1 = Y, 2 = Z.
    pub axis: u8,
    pub value: f32,
}

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
// Comandos que Electron envía al motor (stdin → motor)
//
// Nota: este contrato IPC es compartido con frontend y otros binarios.
// Algunos comandos orientados a 2D se conservan por compatibilidad y el
// binario 3D los ignora o los deriva a stubs explícitos de compat.
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
    /// Sustituir el mesh visual de una entidad existente (sin crear entidad nueva).
    ReplaceEntityModel { id: u32, path: String },
    /// Registrar un .glb/.gltf/.fbx en el almacén de recursos (sin instanciar en escena).
    LoadModelAsset { path: String, name: String },
    /// Eliminar un modelo del almacén de recursos.
    RemoveModelAsset { path: String },
    /// Solicitar la lista de modelos 3D precargados.
    GetModelsList,
    /// Cubo procedural del editor (restaurar bloques de plantilla sin `.glb`).
    SpawnEditorBox {
        name: String,
        position: [f32; 3],
        scale: [f32; 3],
    },
    /// Icono del sol (luz direccional) en escenas 3D.
    SpawnSun {
        name: String,
        position: [f32; 3],
        scale: [f32; 3],
    },
    /// Suelo checker de la escena 3D (placeholder visual + física).
    SpawnGround {
        position: [f32; 3],
        scale: [f32; 3],
    },
    /// Ajustar luz direccional (sol) sin mover el icono.
    SetDirectionalLight {
        #[serde(default)]
        ambient: Option<f32>,
        #[serde(default)]
        intensity: Option<f32>,
        #[serde(default)]
        shadow_darkness: Option<f32>,
    },
    /// Restaurar posición y orientación del personaje jugable (carga de `.save`).
    #[serde(rename = "set_play_character_spawn", alias = "set_first_person_spawn")]
    SetPlayCharacterSpawn {
        position: [f32; 3],
        yaw: f32,
        pitch: f32,
    },
    /// Vista del personaje jugable: pies del Player, orientación de cámara, FOV y frustum de editor.
    ///
    /// Con `camera_only: true` solo `position_axis`, `yaw`, `fov_y` y `frustum_distance`
    /// afectan al ojo/cono FPS (sin mover al Player). El front envía únicamente el campo modificado.
    /// Sin `camera_only` (carga/restauración): `position` = pies del Player; `yaw`/`pitch` recomendados.
    #[serde(rename = "set_play_character_view", alias = "set_first_person_view")]
    SetPlayCharacterView {
        #[serde(default)]
        position: Option<[f32; 3]>,
        #[serde(default)]
        position_axis: Option<AxisValue>,
        #[serde(default)]
        yaw: Option<f32>,
        #[serde(default)]
        pitch: Option<f32>,
        #[serde(default)]
        fov_y: Option<f32>,
        #[serde(default)]
        frustum_distance: Option<f32>,
        #[serde(default)]
        camera_only: Option<bool>,
        #[serde(default)]
        camera_follow_mode: Option<PlayCameraFollowMode>,
    },
    /// Actualizar transform de una entidad por id.
    ///
    /// **Motor-first**: el front envía únicamente el campo que cambió. Para mover un solo eje
    /// usar `position_axis` / `scale_axis` (el motor lee el valor actual y reemplaza solo ese
    /// eje). Los campos vectoriales (`position`, `scale`, `rotation`) son la ruta legacy y
    /// pisan los demás ejes con datos del front (no recomendado para edición interactiva).
    SetTransform {
        id:       u32,
        #[serde(default)]
        position: Option<[f32; 3]>,
        #[serde(default)]
        position_axis: Option<AxisValue>,
        #[serde(default)]
        rotation: Option<[f32; 4]>,  // quaternion xyzw
        #[serde(default)]
        scale:    Option<[f32; 3]>,
        #[serde(default)]
        scale_axis: Option<AxisValue>,
        /// Controla si el cambio se registra en historial Undo/Redo.
        /// None/true: registrar (acciones de usuario). false: no registrar (restore/carga).
        #[serde(default)]
        track_undo: Option<bool>,
        /// Jugador FP en editor: solo transform del mesh (no mover viewport orbital).
        #[serde(default)]
        body_rotation_only: Option<bool>,
        /// Incremento de un eje Euler YXZ en grados (el motor compone el quaternion).
        #[serde(default)]
        rotation_euler_delta: Option<RotationEulerDelta>,
        /// Pitch, yaw, roll absolutos en grados (YXZ, misma convención que glam).
        #[serde(default)]
        rotation_euler_degrees: Option<[f32; 3]>,
    },
    /// Cambiar el nombre de una entidad por id.
    /// `force`: si es true, omite la validación de nombre duplicado (usado en restore de proyecto).
    SetEntityName {
        id:   u32,
        name: String,
        #[serde(default)]
        force: bool,
    },
    /// Cambiar la escena activa solicitada por frontend.
    SetScene { scene: String },
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
        pivot_x:   f32,
        pivot_y:   f32,
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
    /// Definir el tamaño del área de trabajo del mundo (unidades de mundo).
    SetWorldSize {
        width: f32,
        height: f32,
        #[serde(default)]
        depth: Option<f32>,
    },
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
    /// FOV vertical de la cámara 3D (radianes).
    SetCameraFov { fov_y: f32 },
    /// Alcance del gizmo de frustum de cámara FPS en el editor (metros).
    #[serde(rename = "set_fps_editor_frustum_distance", alias = "set_fp_editor_frustum_distance")]
    SetFpsEditorFrustumDistance { distance: f32 },
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
        /// Scripts Lua que se ejecutan mientras esta animación está activa.
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
    PlayAnimation {
        id: u32,
        name: String,
        #[serde(default = "default_clip_loop")]
        loop_: bool,
    },
    /// Detener la animación en curso.
    StopAnimation { id: u32 },
    /// Adjuntar un script Lua a una entidad. `source` es el código Lua completo.
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
    /// Nombre por defecto de pestaña de escena del editor (`Scene-01`, …).
    GetDefaultSceneName { id: u32 },
    /// Restore post-carga de entidad (transform, física, bindings).
    ApplyEntityRestore {
        id: u32,
        #[serde(default)]
        name: Option<String>,
        transform: EntityRestoreTransform,
        #[serde(default)]
        physics: Option<EntityRestorePhysics>,
        #[serde(default)]
        control_bindings: Option<ControlBindingsData>,
        #[serde(default)]
        omit_scale: bool,
        #[serde(default)]
        skip_transform: bool,
    },
}

#[derive(Debug, Deserialize, Clone)]
pub struct AnimationFrameData {
    pub path:      String,
    pub pivot_x:   f32,
    pub pivot_y:   f32,
    #[serde(default)]
    pub src_x:     Option<u32>,
    #[serde(default)]
    pub src_y:     Option<u32>,
    #[serde(default)]
    pub src_w:     Option<u32>,
    #[serde(default)]
    pub src_h:     Option<u32>,
}

/// Script Lua asociado a una animación. Se ejecuta solo mientras la animación está activa.
#[derive(Debug, Deserialize, Clone)]
pub struct AnimScriptData {
    pub name:   String,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ControlScriptData {
    pub name:   String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light_ambient: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light_intensity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_darkness: Option<f32>,
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
    #[serde(default)]
    pub camera_follow_mode: PlayCameraFollowMode,
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
    pub scale: [f32; 3],
}

#[derive(Debug, Deserialize, Clone)]
pub struct EntityRestorePhysics {
    pub enabled: bool,
    pub body_type: String,
}

// ---------------------------------------------------------------------------
// Eventos que el motor envía a Electron (motor → stdout)
// ---------------------------------------------------------------------------
#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum EngineEvent {
    Ready { gravity: f32 },
    Pong,
    Error { message: String },
    ModelLoaded {
        id: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<[f32; 3]>,
        #[serde(skip_serializing_if = "Option::is_none")]
        scale: Option<[f32; 3]>,
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
    },
    /// Emitido cuando un personaje PNG se cargó correctamente.
    CharacterLoaded { id: u32, path: String },
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
    /// Clips de animación embebidos en el modelo 3D de una entidad.
    ModelClipsReady {
        id: u32,
        path: String,
        clips: Vec<ModelClipInfoEvent>,
    },
    /// Progreso de carga de un GLB grande (hilo en segundo plano).
    ModelLoadProgress { path: String, stage: String },
    /// Emitido cuando el estado de física de una entidad cambia (activado/desactivado por script).
    PhysicsChanged { entity_id: u32, enabled: bool, body_type: String },
    /// Emitido cuando un sprite PNG se cargó correctamente en el almacén.
    SpriteLoaded { path: String, name: String, width: u32, height: u32 },
    /// Emitido cuando se eliminó un sprite del almacén.
    SpriteRemoved { path: String },
    /// Emitido como respuesta a GetSpritesList: lista de sprites disponibles.
    SpritesList { sprites: Vec<SpriteInfo> },
    /// Modelo 3D registrado en el almacén de recursos.
    ModelAssetLoaded { path: String, name: String },
    ModelAssetRemoved { path: String },
    ModelsList { models: Vec<ModelInfo> },
    /// Emitido cuando un archivo de audio se registró en el almacén.
    SoundLoaded { path: String, name: String },
    /// Emitido cuando se eliminó un sonido del almacén.
    SoundRemoved { path: String },
    /// Emitido como respuesta a GetSoundsList: lista de sonidos disponibles.
    SoundsList { sounds: Vec<SoundInfo> },
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
    EntityRemoved { id: u32, kind: String },
    /// Emitido cuando el usuario mantiene Ctrl y hace click añadiendo/quitando entidades
    /// a la selección múltiple. `ids` contiene todos los IDs actualmente seleccionados.
    MultiSelectChanged { ids: Vec<u32> },
    /// Vista del personaje jugable (pies, cámara y transform del mesh).
    #[serde(rename = "play_character_view_changed", alias = "first_person_view_changed")]
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
    /// Emitido ~1 vez por segundo con métricas de rendimiento del motor.
    DebugMetrics {
        fps:            f32,
        frame_time_ms:  f32,
        draw_calls:     u32,
        physics_bodies: u32,
        #[serde(
            rename = "play_character_position",
            alias = "first_person_position",
            skip_serializing_if = "Option::is_none"
        )]
        play_character_position: Option<[f32; 3]>,
        #[serde(
            rename = "play_character_yaw",
            alias = "first_person_yaw",
            skip_serializing_if = "Option::is_none"
        )]
        play_character_yaw: Option<f32>,
        #[serde(
            rename = "play_character_pitch",
            alias = "first_person_pitch",
            skip_serializing_if = "Option::is_none"
        )]
        play_character_pitch: Option<f32>,
    },
    /// Emitido cuando el preview cambia de estado desde el motor.
    PreviewPlayingChanged { playing: bool },
    /// Emitido cuando un actor entra en un área de ejecución (trigger).
    TriggerEntered { trigger_id: u32, actor_id: u32 },
    /// Emitido cuando un actor sale de un área de ejecución (trigger).
    TriggerExited { trigger_id: u32, actor_id: u32 },
    /// Emitido cada 5 minutos cuando el autosave está activo.
    AutosaveTick,
    /// Respuesta a `export_save_snapshot`: escena activa lista para el `.save`.
    SaveSnapshotReady { scene: SaveSceneSnapshotPayload },
    /// Respuesta a `get_default_scene_name`.
    DefaultSceneNameReady { id: u32, name: String },
}

/// Información básica de un sprite almacenado en el motor.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SpriteInfo {
    pub path:   String,
    pub name:   String,
    pub width:  u32,
    pub height: u32,
}

/// Información básica de un modelo 3D en el almacén de recursos.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelInfo {
    pub path: String,
    pub name: String,
}

/// Información básica de un sonido almacenado en el motor.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SoundInfo {
    pub path: String,
    pub name: String,
}

/// Información básica de un fondo almacenado en el motor.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BackgroundInfo {
    pub path: String,
    pub name: String,
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
