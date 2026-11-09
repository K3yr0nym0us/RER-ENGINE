//! Editor egui autónomo de RER-ENGINE.
//!
//! El editor lanza y controla el proceso del motor directamente en Rust.
//! Electron solo recibe eventos reenviados para la UI React antigua durante la
//! migración.

mod app;
mod assets;
mod bridge;
mod cli;
mod context;
mod engine_host;
mod engine_viewport;
mod gpu;
mod i18n;
mod project_state;
mod save;
mod scenes;
mod theme;
mod ui;
mod world;

use std::io::BufRead;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use serde_json::Value;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

use crate::app::{EditorApp, EditorStatus};
use crate::cli::Cli;
use crate::engine_host::{EngineHost, UserEvent};

const DEFAULT_LOG_FILTER: &str = "rer_engine_editor=info,ash=warn,egui_ash_renderer=warn";

/// Icono de ventana, el mismo PNG que usa Electron (`src/resources/RER-ENGINE-LOGO.png`).
const APP_ICON_PNG: &[u8] = include_bytes!("../../../../../src/resources/RER-ENGINE-LOGO.png");

fn app_window_icon() -> Option<winit::window::Icon> {
    let image = image::load_from_memory(APP_ICON_PNG).ok()?;
    let resized = image.resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
    let rgba = resized.to_rgba8();
    let (width, height) = rgba.dimensions();
    winit::window::Icon::from_rgba(rgba.into_raw(), width, height).ok()
}

struct EditorWindow {
    window: Arc<Window>,
    gpu: gpu::Gpu,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    app: EditorApp,
}

impl EditorWindow {
    fn new(window: Arc<Window>, cli: Cli) -> Result<Self, String> {
        let gpu = gpu::Gpu::new(window.as_ref())?;

        let egui_ctx = egui::Context::default();
        theme::apply(&egui_ctx);

        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            Some(winit::window::Theme::Dark),
            Some(gpu.max_texture_dimension_2d() as usize),
        );

        Ok(Self {
            window,
            gpu,
            egui_ctx,
            egui_state,
            app: EditorApp::new(cli),
        })
    }

    fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), String> {
        self.gpu.resize(size)
    }

    /// Recoloca el overlay del motor tras un resize/cambio de escala.
    fn update_engine_viewport(&mut self) {
        if let Some(viewport) = engine_viewport::central_viewport(self.window.as_ref()) {
            self.app.send_engine(viewport.set_bounds_command());
        }
    }

    fn render(&mut self) -> Result<(), String> {
        let EditorWindow {
            window,
            gpu,
            egui_ctx,
            egui_state,
            app,
        } = self;

        let repaint = gpu.render(window.as_ref(), egui_ctx, egui_state, |ui| app.ui(ui))?;
        if repaint {
            window.request_redraw();
        }
        Ok(())
    }
}

struct EditorApplication {
    cli: Cli,
    proxy: EventLoopProxy<UserEvent>,
    editor: Option<EditorWindow>,
    engine_host: Option<EngineHost>,
    engine_tx: Option<Sender<Value>>,
}

impl EditorApplication {
    fn send_engine(&self, command: Value) {
        if let Some(tx) = &self.engine_tx {
            let _ = tx.send(command);
        }
    }

    fn handle_electron_message(&mut self, message: Value) -> bool {
        let kind = message.get("type").and_then(Value::as_str).unwrap_or("");

        match kind {
            "engine_command" => {
                if let Some(command) = message.get("command") {
                    if command.get("cmd").and_then(Value::as_str) == Some("set_autosave") {
                        let enabled = command
                            .get("enabled")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        if let Some(editor) = self.editor.as_mut() {
                            // El autosave lo ejecuta el editor Rust al recibir `autosave_tick`.
                            editor.app.auto_save_enabled = enabled && editor.app.has_saved_once;
                        }
                    }
                    if let Some(editor) = self.editor.as_mut() {
                        editor.app.apply_engine_command(command);
                    }
                    self.send_engine(command.clone());
                }
            }
            "viewport_bounds" => {
                // El editor egui es ahora dueño de la geometría del motor; el
                // viewport de Electron ya no reposiciona el overlay.
            }
            "editor_state" => {
                if let Some(editor) = self.editor.as_mut() {
                    if let Some(enabled) = message.get("auto_save_enabled").and_then(Value::as_bool)
                    {
                        editor.app.auto_save_enabled = enabled;
                    }
                    if let Some(saved) = message.get("has_saved_once").and_then(Value::as_bool) {
                        editor.app.has_saved_once = saved;
                    }
                    if let Some(saving) = message.get("saving").and_then(Value::as_bool) {
                        editor.app.saving = saving;
                    }
                }
            }
            "project_state" => {
                let id = message.get("id").and_then(Value::as_u64).unwrap_or(0);
                let state = message.get("state").cloned().unwrap_or(Value::Null);
                if let Some(editor) = self.editor.as_mut()
                    && editor.app.receive_project_state(id, &state)
                {
                    log::info!("[editor] estado de proyecto recibido (id={id})");
                }
            }
            "save_request" => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.app.begin_save(false);
                }
            }
            "exit" => return true,
            _ => {}
        }

        false
    }

    /// Procesa eventos del motor que afectan al guardado y a los ajustes de mundo.
    fn handle_engine_save_event(&mut self, value: &Value) {
        match value.get("event").and_then(Value::as_str).unwrap_or("") {
            "save_snapshot_ready" => {
                let scene = value.get("scene").cloned().unwrap_or(Value::Null);
                if let Some(editor) = self.editor.as_mut() {
                    editor.app.apply_world_snapshot(&scene);
                }
                self.start_save_write(scene);
            }
            "autosave_tick" => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.app.on_autosave_tick();
                }
            }
            "ready" => self.schedule_world_sync(),
            "editor_scene_switched" | "project_loaded_2d" | "project_loaded_3d" => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.app.request_world_sync();
                    editor.app.request_asset_sync();
                }
            }
            "editor_scenes_updated" => {
                let reason = value
                    .get("update_reason")
                    .and_then(Value::as_str)
                    .unwrap_or("sync");
                if matches!(reason, "boot" | "project_loaded" | "scene_deleted")
                    && let Some(editor) = self.editor.as_mut()
                {
                    editor.app.request_world_sync();
                }
            }
            _ => {}
        }
    }

    /// Pide un snapshot de mundo tras `ready` (deja aplicar la escena de arranque).
    fn schedule_world_sync(&self) {
        let proxy = self.proxy.clone();
        std::thread::Builder::new()
            .name("world-sync".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1200));
                let _ = proxy.send_event(UserEvent::WorldSync);
            })
            .ok();
    }

    /// Ensambla y escribe el `.save` en un hilo aparte.
    fn start_save_write(&mut self, engine_scene: Value) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let Some((save_path, state)) = editor.app.take_pending_save() else {
            return;
        };

        let project_type = editor.app.project_type_string(&state);
        let game_style = editor.app.game_style_string(&state);
        let extract_dir = editor.app.extract_dir.clone();
        editor.app.push_log("[Guardar] empaquetando .save…", false);
        let proxy = self.proxy.clone();

        let spawned = std::thread::Builder::new()
            .name("save-writer".into())
            .spawn(move || {
                let result = crate::save::write_save(
                    &engine_scene,
                    &state,
                    &project_type,
                    &game_style,
                    &save_path,
                    extract_dir.as_deref(),
                );
                let _ = proxy.send_event(UserEvent::SaveFinished(result));
            });

        if let Err(error) = spawned {
            editor.app.complete_save(Err(format!(
                "no se pudo crear el hilo de guardado: {error}"
            )));
        }
    }
}

impl ApplicationHandler<UserEvent> for EditorApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.editor.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title(i18n::text(self.cli.locale).window_title)
            .with_window_icon(app_window_icon())
            .with_inner_size(LogicalSize::new(1280.0, 800.0))
            .with_min_inner_size(LogicalSize::new(
                theme::MIN_WINDOW_WIDTH,
                theme::MIN_WINDOW_HEIGHT,
            ));

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("No se pudo crear la ventana del editor"),
        );

        // El motor se cuelga de la ventana egui, no de Electron: usamos el HWND
        // del editor y el rectángulo central del marco.
        if let Some(viewport) = engine_viewport::central_viewport(window.as_ref()) {
            self.cli.engine_parent_hwnd = viewport.parent_hwnd;
            self.cli.engine_bounds = Some(viewport.engine_bounds());
            log::info!(
                "[editor] viewport del motor: {}x{} en rel ({}, {})",
                viewport.width,
                viewport.height,
                viewport.rel_x,
                viewport.rel_y
            );
        } else {
            log::warn!("[editor] no se pudo calcular el viewport del motor");
        }

        match EditorWindow::new(Arc::clone(&window), self.cli.clone()) {
            Ok(editor) => {
                log::info!(
                    "[editor] ventana lista (proyecto={}, estilo={}, locale={})",
                    self.cli.context.project_type.as_str(),
                    self.cli.context.game_style.as_str(),
                    self.cli.locale.as_str()
                );
                self.editor = Some(editor);

                match EngineHost::spawn(&self.cli, self.proxy.clone()) {
                    Ok(host) => {
                        let tx = host.sender();
                        if let Some(editor) = self.editor.as_mut() {
                            editor.app.engine_tx = Some(tx.clone());
                            editor.app.proxy = Some(self.proxy.clone());
                            editor.app.push_log("[Editor] ventana lista", false);
                            editor.app.push_log("[Editor] motor iniciado", false);
                        }
                        self.engine_tx = Some(tx);
                        self.engine_host = Some(host);
                    }
                    Err(message) => {
                        log::error!("[editor] {message}");
                        if let Some(editor) = self.editor.as_mut() {
                            editor.app.status = EditorStatus::Error;
                            editor.app.push_log(format!("[Editor] {message}"), true);
                        }
                    }
                }

                crate::bridge::notify_editor_ready();
                window.request_redraw();
            }
            Err(message) => {
                log::error!("[editor] {message}");
                event_loop.exit();
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::EngineEvent(value) => {
                self.handle_engine_save_event(&value);
                if let Some(editor) = self.editor.as_mut() {
                    editor.app.log_engine_event(&value);
                    editor.app.handle_engine_event(&value);
                    editor.window.request_redraw();
                }
            }
            UserEvent::FromElectron(value) => {
                let should_exit = self.handle_electron_message(value);
                if should_exit {
                    event_loop.exit();
                    return;
                }
                if let Some(editor) = self.editor.as_mut() {
                    editor.window.request_redraw();
                }
            }
            UserEvent::SaveFinished(result) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.app.complete_save(result);
                    editor.window.request_redraw();
                }
            }
            UserEvent::SaveTimeout(id) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.app.timeout_save(id);
                    editor.window.request_redraw();
                }
            }
            UserEvent::WorldSync => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.app.request_world_sync();
                    editor.app.request_asset_sync();
                    editor.window.request_redraw();
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let is_main = self
            .editor
            .as_ref()
            .is_some_and(|editor| editor.window.id() == window_id);
        if !is_main {
            return;
        }

        if matches!(event, WindowEvent::CloseRequested) {
            self.send_engine(serde_json::json!({ "cmd": "shutdown" }));
            event_loop.exit();
            return;
        }

        let Some(editor) = self.editor.as_mut() else {
            return;
        };

        let response = editor
            .egui_state
            .on_window_event(editor.window.as_ref(), &event);
        // `on_window_event` devuelve `repaint` también para `RedrawRequested`;
        // pedir otro redraw ahí crearía un bucle de frames infinito.
        let is_redraw = matches!(event, WindowEvent::RedrawRequested);
        if response.repaint && !is_redraw {
            editor.window.request_redraw();
        }
        if response.consumed {
            return;
        }

        match event {
            WindowEvent::Resized(size) => {
                if let Err(error) = editor.resize(size) {
                    log::error!("[editor] {error}");
                }
                editor.update_engine_viewport();
                editor.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let size = editor.window.inner_size();
                if let Err(error) = editor.resize(size) {
                    log::error!("[editor] {error}");
                }
                editor.update_engine_viewport();
                editor.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = editor.render() {
                    log::error!("[editor] {error}");
                }
            }
            _ => {}
        }
    }
}

fn spawn_electron_reader(proxy: EventLoopProxy<UserEvent>) {
    std::thread::Builder::new()
        .name("electron-stdin".into())
        .spawn(move || {
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                let Ok(line) = line else { break };
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(value) = serde_json::from_str::<Value>(line)
                    && proxy.send_event(UserEvent::FromElectron(value)).is_err()
                {
                    break;
                }
            }
        })
        .expect("No se pudo crear el hilo de lectura de Electron");
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(DEFAULT_LOG_FILTER))
        .init();

    let cli = Cli::parse_from(std::env::args());
    log::info!("[editor] {}", Cli::usage());

    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("No se pudo crear el EventLoop del editor");
    event_loop.set_control_flow(ControlFlow::Wait);

    let proxy = event_loop.create_proxy();
    spawn_electron_reader(proxy.clone());

    let mut application = EditorApplication {
        cli,
        proxy,
        editor: None,
        engine_host: None,
        engine_tx: None,
    };
    event_loop
        .run_app(&mut application)
        .expect("Error en el event loop del editor");
}
