use std::{
    io::{self, BufRead, Write},
    sync::mpsc::Sender,
    thread,
};

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
    SetBounds { x: i32, y: i32, width: u32, height: u32 },
    LoadModel { path: String },
    /// Actualizar transform de una entidad por id.
    SetTransform {
        id:       u32,
        position: Option<[f32; 3]>,
        rotation: Option<[f32; 4]>,  // quaternion xyzw
        scale:    Option<[f32; 3]>,
    },
    /// Cambiar la escena activa. `scene` puede ser "2D", "3D", etc.
    SetScene { scene: String },
    /// Cargar una imagen PNG como escenario de fondo en la escena 2D.
    LoadScenario { path: String },
    /// Ajustar la escala de un escenario 2D específico preservando proporciones.
    SetScenarioScale { id: u32, scale: f32 },
    /// Duplicar un escenario existente (crea una nueva entidad con el mismo PNG).
    DuplicateScenario { id: u32 },
    /// Cargar una imagen PNG como personaje en la escena 2D.
    LoadCharacter { path: String },
    /// Ajustar la escala de un personaje 2D específico preservando proporciones.
    SetCharacterScale { id: u32, scale: f32 },
    /// Duplicar un personaje existente (crea una nueva entidad con el mismo PNG).
    DuplicateCharacter { id: u32 },
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
    },
    /// Restaurar el sprite original de una entidad después de una animación.
    RestoreAnimationFrame { id: u32 },
    /// Eliminar una entidad de la escena por su ID.
    RemoveEntity { id: u32 },
    /// Definir el tamaño del área de trabajo del mundo (unidades de mundo).
    SetWorldSize { width: f32, height: f32 },
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
    /// Activar una herramienta de dibujo. tool: "draw_collider" | "" (cancelar)
    SetActiveTool { tool: String },
    /// Recrear un colisionador de 4 puntos desde datos guardados (restauración de proyecto).
    CreateColliderFromPoints { points: [[f32; 2]; 4] },
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
        audio_path: Option<String>,
        logical_w:  u32,
        logical_h:  u32,
    },
    /// Reproducir una animación guardada por ID de entidad y nombre.
    /// El motor busca en su almacén de animaciones — el front no necesita
    /// reenviar los datos de frames en cada reproducción.
    PlayAnimation { id: u32, name: String },
    /// Detener la animación en curso.
    StopAnimation { id: u32 },
}

#[derive(Debug, Deserialize, Clone)]
pub struct AnimationFrameData {
    pub path:      String,
    pub pivot_x:   f32,
    pub pivot_y:   f32,
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
    /// Emitido cuando una herramienta de dibujo fue cancelada desde el motor.
    ToolCancelled,
    /// Emitido cuando el usuario selecciona el pivot de un frame en modo edición.
    PivotSelected { frame_path: String, pivot_x: f32, pivot_y: f32 },
    /// Emitido cuando una animación termina (no loop) o se detiene.
    AnimationFinished { entity_id: u32 },
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
/// los comandos parseados al event loop del motor.
pub fn start_ipc_thread(tx: Sender<EngineCommand>) {
    thread::Builder::new()
        .name("ipc-stdin".into())
        .spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(line) if !line.trim().is_empty() => {
                        match serde_json::from_str::<EngineCommand>(&line) {
                            Ok(cmd) => {
                                if tx.send(cmd).is_err() {
                                    break; // El event loop cerró el receptor
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
