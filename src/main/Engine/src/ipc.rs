use std::{
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
    /// Cambiar la escena activa. `scene` puede ser "2D", "3D", etc.
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
    /// Duplicar un escenario existente (crea una nueva entidad con el mismo PNG).
    DuplicateScenario { id: u32 },
    /// Cargar una imagen PNG como personaje en la escena 2D.
    LoadCharacter {
        path: String,
        /// Si es true, registra la creación en el historial de deshacer.
        #[serde(default)]
        track_undo: Option<bool>,
    },
    /// Ajustar la escala de un personaje 2D específico preservando proporciones.
    SetCharacterScale { id: u32, scale: f32 },
    /// Duplicar un personaje existente (crea una nueva entidad con el mismo PNG).
    DuplicateCharacter { id: u32 },
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
    SetWorldSize { width: f32, height: f32 },
    /// Cambiar la gravedad del mundo físico (valor Y negativo = hacia abajo).
    SetGravity { gravity: f32 },
    /// Mostrar u ocultar la cuadrícula del mundo.
    SetGridVisible { visible: bool },
    /// Cambiar el tamaño de cada celda de la cuadrícula.
    SetGridCellSize { size: f32 },
    /// Estado de la tecla Ctrl enviado desde Electron (ventana embebida no recibe teclado directo).
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
        logical_w:  u32,
        logical_h:  u32,
        /// Scripts Lua que se ejecutan mientras esta animación está activa.
        #[serde(default)]
        scripts:    Vec<AnimScriptData>,
        /// Si false (default), ninguna otra animación puede interrumpirla antes de que termine.
        #[serde(default)]
        is_cancelable: bool,
    },
    /// Definir la animación predeterminada de una entidad.
    SetDefaultAnimation { id: u32, name: String },
    /// Reproducir una animación guardada por ID de entidad y nombre.
    /// El motor busca en su almacén de animaciones — el front no necesita
    /// reenviar los datos de frames en cada reproducción.
    PlayAnimation { id: u32, name: String },
    /// Detener la animación en curso.
    StopAnimation { id: u32 },
    /// Adjuntar un script Lua a una entidad. `source` es el código Lua completo.
    /// `path` se usa solo para mensajes de error y logs.
    LoadScript { id: u32, path: String, source: String },
    /// Ejecutar script de control para una entidad (trigger en runtime por input).
    /// Se procesa solo en modo de juego.
    RunControlScript { id: u32, control_key: String, path: String, source: String },
    /// Desadjuntar todos los scripts de una entidad (sin eliminar la entidad).
    UnloadScript { id: u32 },
    /// Cargar una imagen PNG como sprite (solo almacenamiento, no se renderiza).
    LoadSprite { path: String, name: String },
    /// Eliminar un sprite del almacén del motor.
    RemoveSprite { path: String },
    /// Solicitar la lista de sprites cargados en el motor.
    GetSpritesList,
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

// ---------------------------------------------------------------------------
// Eventos que el motor envía a Electron (motor → stdout)
// ---------------------------------------------------------------------------
#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EngineEvent {
    Ready,
    Pong,
    Error { message: String },
    ModelLoaded { id: u32 },
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
    ScenarioLoaded { id: u32, path: String },
    /// Emitido cuando un personaje PNG se cargó correctamente.
    CharacterLoaded { id: u32, path: String },
    /// Emitido justo después de configurar la escena 2D con el ID y transform del jugador.
    #[serde(rename = "player_ready")]
    PlayerReady {
        id:       u32,
        position: [f32; 3],
        scale:    [f32; 3],
    },
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
    /// Emitido cuando el cursor se mueve y la herramienta quick_build_place está activa.
    QuickBuildMove { x: f32, y: f32 },
    /// Emitido cuando el usuario hace click con la herramienta quick_build_place activa.
    /// `fit_to_grid` indica si Ctrl estaba presionado al colocar.
    QuickBuildClick { x: f32, y: f32, fit_to_grid: bool },
    /// Emitido cuando una entidad es eliminada del mundo (por Ctrl+Z, RemoveEntity, etc.).
    EntityRemoved { id: u32 },
    /// Emitido cuando el motor detecta un input de control en modo juego.
    ControlInputDetected { device: String, control_key: String },
    /// Emitido ~1 vez por segundo con métricas de rendimiento del motor.
    DebugMetrics {
        fps:            f32,
        frame_time_ms:  f32,
        draw_calls:     u32,
        physics_bodies: u32,
    },
    /// Emitido cuando un actor entra en un área de ejecución (trigger).
    TriggerEntered { trigger_id: u32, actor_id: u32 },
    /// Emitido cuando un actor sale de un área de ejecución (trigger).
    TriggerExited { trigger_id: u32, actor_id: u32 },
}

/// Información básica de un sprite almacenado en el motor.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SpriteInfo {
    pub path:   String,
    pub name:   String,
    pub width:  u32,
    pub height: u32,
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
                            Err(e) => eprintln!("[ipc] parse error: {e} — línea: {line}"),
                        }
                    }
                    Err(_) => break, // stdin cerrado
                    _ => {}
                }
            }
        })
        .expect("No se pudo crear el hilo IPC");
}
