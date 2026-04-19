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
