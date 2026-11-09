//! Supervisor del proceso del motor.
//!
//! El editor lanza `rer_engine_2d` / `rer_engine_3d`, es dueño de su
//! `stdin`/`stdout` y le habla directamente en Rust. Electron ya no lanza ni
//! controla el motor; solo recibe eventos reenviados para la UI React antigua.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use serde_json::Value;
use winit::event_loop::EventLoopProxy;

use crate::cli::Cli;
use crate::context::ProjectType;

/// Eventos que llegan al event loop del editor desde otros hilos.
#[derive(Debug, Clone)]
pub enum UserEvent {
    /// Línea JSON emitida por el motor.
    EngineEvent(Value),
    /// Mensaje JSON enviado por Electron (solo puente de la UI antigua).
    FromElectron(Value),
    /// El hilo de escritura del `.save` terminó.
    SaveFinished(Result<crate::save::SaveSuccess, String>),
    /// Un guardado excedió el tiempo máximo esperando el estado/snapshot.
    SaveTimeout(u64),
    /// Pide sincronizar los ajustes de mundo desde el motor.
    WorldSync,
}

pub struct EngineHost {
    child: Child,
    tx: Sender<Value>,
    _stdout_thread: JoinHandle<()>,
    _stderr_thread: JoinHandle<()>,
}

impl EngineHost {
    pub fn spawn(cli: &Cli, proxy: EventLoopProxy<UserEvent>) -> Result<Self, String> {
        let exe = std::env::current_exe()
            .map_err(|err| format!("no se pudo resolver la ruta del editor: {err}"))?;
        let dir = exe
            .parent()
            .ok_or_else(|| "el ejecutable del editor no tiene carpeta padre".to_string())?;
        let binary_name = match cli.context.project_type {
            ProjectType::TwoD => "rer_engine_2d",
            ProjectType::ThreeD => "rer_engine_3d",
        };
        let engine_path = dir.join(format!("{binary_name}{}", std::env::consts::EXE_SUFFIX));

        let mut command = Command::new(&engine_path);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(bounds) = cli.engine_bounds {
            #[cfg(target_os = "windows")]
            {
                command.args([
                    "--overlay",
                    &cli.engine_parent_hwnd.to_string(),
                    &bounds.x.to_string(),
                    &bounds.y.to_string(),
                    &bounds.width.to_string(),
                    &bounds.height.to_string(),
                    &bounds.rel_x.to_string(),
                    &bounds.rel_y.to_string(),
                ]);
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = bounds;
            }
        }

        if let Some(save_path) = &cli.save_path {
            command.env("RER_PROJECT_SAVE_PATH", save_path);
        }
        if let Some(extract_dir) = &cli.extract_dir {
            command.env("RER_PROJECT_EXTRACT_DIR", extract_dir);
        }
        if cli.context.is_3d() && cli.save_path.is_some() {
            command.env("RER_3D_START_FROM_SAVE", "1");
        }

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        log::info!("[editor] lanzando motor: {}", engine_path.display());
        let mut child = command
            .spawn()
            .map_err(|err| format!("no se pudo lanzar {}: {err}", engine_path.display()))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "el motor no expuso stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "el motor no expuso stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "el motor no expuso stderr".to_string())?;

        let (tx, rx) = mpsc::channel::<Value>();
        thread::Builder::new()
            .name("engine-writer".into())
            .spawn(move || {
                for command in rx {
                    let line = command.to_string();
                    if writeln!(stdin, "{line}").is_err() || stdin.flush().is_err() {
                        break;
                    }
                }
            })
            .map_err(|err| format!("no se pudo crear el hilo escritor del motor: {err}"))?;

        let stderr_proxy = proxy.clone();
        let stdout_thread = thread::Builder::new()
            .name("engine-stdout".into())
            .spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }
                    // La UI React antigua sigue recibiendo los eventos del motor.
                    crate::bridge::forward_line(&line);
                    if let Ok(value) = serde_json::from_str::<Value>(&line)
                        && proxy.send_event(UserEvent::EngineEvent(value)).is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|err| format!("no se pudo crear el hilo lector del motor: {err}"))?;

        let stderr_thread = thread::Builder::new()
            .name("engine-stderr".into())
            .spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    // Eventos silenciosos del motor: se reenvían igual que stdout.
                    if let Some(json) = line.strip_prefix("[IPC_EVENT]") {
                        let json = json.trim();
                        crate::bridge::forward_line(json);
                        if let Ok(value) = serde_json::from_str::<Value>(json) {
                            let _ = stderr_proxy.send_event(UserEvent::EngineEvent(value));
                        }
                        continue;
                    }
                    if !line.trim().is_empty() {
                        log::warn!("[engine] {line}");
                    }
                }
            })
            .map_err(|err| format!("no se pudo crear el hilo de logs del motor: {err}"))?;

        Ok(Self {
            child,
            tx,
            _stdout_thread: stdout_thread,
            _stderr_thread: stderr_thread,
        })
    }

    pub fn sender(&self) -> Sender<Value> {
        self.tx.clone()
    }
}

impl Drop for EngineHost {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
