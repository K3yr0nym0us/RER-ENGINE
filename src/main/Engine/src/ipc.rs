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
        id:       u32,
        name:     String,
        position: [f32; 3],
        rotation: [f32; 4],   // quaternion xyzw
        scale:    [f32; 3],
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
