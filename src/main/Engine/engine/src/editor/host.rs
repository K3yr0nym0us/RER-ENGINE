//! Host winit del editor egui de RER-ENGINE.

use std::io::BufRead;
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

use serde_json::Value;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

use crate::editor::app::{EditorApp, EditorStatus};
use crate::editor::cli::Cli;
use crate::editor::engine_host::{EngineHost, UserEvent};
use rer_engine_core::vulkan::VulkanOptions;

const DEFAULT_LOG_FILTER: &str =
    "rer_engine::editor=info,rer_engine_core::vulkan=info,ash=warn,egui_ash_renderer=warn";

/// Icono de ventana, el mismo PNG que usa Electron (`src/resources/RER-ENGINE-LOGO.png`).
const APP_ICON_PNG: &[u8] = include_bytes!("../../../../../../src/resources/RER-ENGINE-LOGO.png");

fn app_window_icon() -> Option<winit::window::Icon> {
    let image = image::load_from_memory(APP_ICON_PNG).ok()?;
    let resized = image.resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
    let rgba = resized.to_rgba8();
    let (width, height) = rgba.dimensions();
    winit::window::Icon::from_rgba(rgba.into_raw(), width, height).ok()
}

struct EditorWindow {
    window: Arc<Window>,
    /// Motor 3D en proceso (solo proyectos 3D). Se declara antes que `gpu` para que sus
    /// recursos Vulkan se liberen antes del device compartido.
    engine: Option<crate::engine3d::host::App>,
    gpu: crate::editor::render::EditorGpu,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    app: EditorApp,
}

impl EditorWindow {
    fn new(window: Arc<Window>, cli: Cli) -> Result<Self, String> {
        let options = if cli.context.is_3d() {
            VulkanOptions::three_d()
        } else {
            VulkanOptions::default()
        };
        let gpu = crate::editor::render::EditorGpu::new(window.as_ref(), options)?;

        let egui_ctx = egui::Context::default();
        crate::editor::theme::apply(&egui_ctx);

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
            engine: None,
            gpu,
            egui_ctx,
            egui_state,
            app: EditorApp::new(cli),
        })
    }

    /// Construye el motor 3D dentro del proceso del editor, reutilizando su `SharedGpu`.
    fn start_in_process_engine(&mut self, proxy: EventLoopProxy<UserEvent>) -> Result<(), String> {
        let cli = self.app.cli.clone();
        // El cargador de proyectos lee estas variables, igual que el proceso standalone.
        if let Some(save_path) = &cli.save_path {
            unsafe {
                std::env::set_var("RER_PROJECT_SAVE_PATH", save_path);
                std::env::set_var("RER_3D_START_FROM_SAVE", "1");
            }
        }
        if let Some(extract_dir) = &cli.extract_dir {
            unsafe { std::env::set_var("RER_PROJECT_EXTRACT_DIR", extract_dir) };
        }

        // El sink debe existir antes de construir el estado: la carga inicial ya emite eventos.
        let (sink_tx, sink_rx) = mpsc::channel::<String>();
        crate::engine3d::ipc::install_event_sink(sink_tx);

        let mut state = crate::engine3d::engine::State::with_context(
            Arc::clone(&self.window),
            self.gpu.shared_gpu(),
        )
        .map_err(|error| error.message)?;

        let from_save = std::env::var("RER_3D_START_FROM_SAVE")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if from_save {
            state.setup_empty_3d();
            crate::engine3d::ipc::send_event(&crate::engine3d::ipc::EngineEvent::Ready {
                gravity: state.physics.gravity_magnitude(),
            });
        } else {
            state.setup_default_3d_scene();
        }
        state.editor_parent_id = 0;

        self.engine = Some(crate::engine3d::host::App::new_in_editor(state));

        std::thread::Builder::new()
            .name("engine-events".into())
            .spawn(move || {
                for line in sink_rx {
                    // Mismo JSON hacia Electron: las modales no cambian.
                    crate::editor::bridge::forward_line(&line);
                    if let Ok(value) = serde_json::from_str::<Value>(&line)
                        && proxy.send_event(UserEvent::EngineEvent(value)).is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| format!("no se pudo crear el hilo de eventos del motor: {error}"))?;

        Ok(())
    }

    /// Comando de Electron que debe llegar al motor en proceso.
    fn handle_engine_command(&mut self, command: &Value) {
        if let Some(engine) = self.engine.as_mut()
            && let Some(parsed) = crate::engine3d::ipc::parse_command(command)
        {
            engine.handle_command(parsed);
        }
    }

    /// Reenvía input a la escena cuando el puntero está en el viewport.
    fn forward_input(&mut self, event: &WindowEvent, egui_consumed: bool) {
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        let pixels_per_point = self.egui_ctx.pixels_per_point();
        let viewport = self.app.viewport_rect;
        let preview = engine.is_preview_playing();

        let to_scene = |position: PhysicalPosition<f64>, rect: egui::Rect| {
            let offset_x = (rect.min.x * pixels_per_point).round();
            let offset_y = (rect.min.y * pixels_per_point).round();
            PhysicalPosition::new(position.x - offset_x as f64, position.y - offset_y as f64)
        };
        let inside = |position: PhysicalPosition<f64>| {
            viewport.is_some_and(|rect| {
                let min_x = rect.min.x * pixels_per_point;
                let min_y = rect.min.y * pixels_per_point;
                let max_x = rect.max.x * pixels_per_point;
                let max_y = rect.max.y * pixels_per_point;
                position.x >= min_x as f64
                    && position.x <= max_x as f64
                    && position.y >= min_y as f64
                    && position.y <= max_y as f64
            })
        };

        match event {
            WindowEvent::CursorMoved {
                device_id,
                position,
                ..
            } => {
                if !inside(*position) && !preview {
                    return;
                }
                let Some(rect) = viewport else { return };
                engine.handle_window_event(WindowEvent::CursorMoved {
                    device_id: *device_id,
                    position: to_scene(*position, rect),
                });
            }
            WindowEvent::CursorLeft { .. } => {
                engine.handle_window_event(event.clone());
            }
            WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } => {
                let cursor_inside = self
                    .app
                    .viewport_rect
                    .zip(self.egui_ctx.pointer_latest_pos())
                    .is_some_and(|(rect, position)| rect.contains(position));
                if !cursor_inside && !preview {
                    return;
                }
                engine.handle_window_event(event.clone());
            }
            WindowEvent::KeyboardInput { .. }
            | WindowEvent::ModifiersChanged(_)
            | WindowEvent::Ime(_)
            | WindowEvent::Focused(_) => {
                if self.egui_ctx.egui_wants_keyboard_input() {
                    return;
                }
                engine.handle_window_event(event.clone());
            }
            _ => {}
        }

        let _ = egui_consumed;
    }

    fn device_motion(&mut self, delta_x: f64, delta_y: f64) {
        if let Some(engine) = self.engine.as_mut() {
            engine.handle_device_motion(delta_x as f32, delta_y as f32);
        }
    }

    /// Primer frame antes de construir el motor, para no dejar la ventana en blanco.
    fn render_loading(&mut self) -> Result<(), String> {
        let EditorWindow {
            window,
            gpu,
            egui_ctx,
            egui_state,
            ..
        } = self;
        let _ = gpu.render(
            window.as_ref(),
            egui_ctx,
            egui_state,
            |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label("Cargando motor 3D…");
                });
            },
            None,
            None,
        )?;
        Ok(())
    }

    fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), String> {
        self.gpu.resize(size)
    }

    /// Recoloca el overlay del motor tras un resize/cambio de escala.
    fn update_engine_viewport(&mut self) {
        if let Some(viewport) =
            crate::editor::engine_viewport::central_viewport(self.window.as_ref())
        {
            self.app.send_engine(viewport.set_bounds_command());
        }
    }

    fn render(&mut self) -> Result<(), String> {
        let EditorWindow {
            window,
            engine,
            gpu,
            egui_ctx,
            egui_state,
            app,
        } = self;

        if let Some(engine) = engine.as_mut() {
            engine.tick();
            engine.frame_update();
        }

        // Rect del frame anterior: el layout de egui se conoce al terminar de construir la UI.
        let viewport = app.viewport_rect;
        let repaint = gpu.render(
            window.as_ref(),
            egui_ctx,
            egui_state,
            |ui| app.ui(ui),
            engine.as_mut(),
            viewport,
        )?;
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
    next_frame_at: Instant,
    target_fps: u64,
}

impl EditorApplication {
    fn send_engine(&mut self, command: Value) {
        // En proceso 3D el comando se despacha directamente; en 2D va al hijo por stdin.
        if let Some(editor) = self.editor.as_mut()
            && editor.engine.is_some()
        {
            editor.handle_engine_command(&command);
            return;
        }
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
                let result = crate::editor::save::write_save(
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
            .with_title(crate::editor::i18n::text(self.cli.locale).window_title)
            .with_window_icon(app_window_icon())
            .with_inner_size(LogicalSize::new(1280.0, 800.0))
            .with_min_inner_size(LogicalSize::new(
                crate::editor::theme::MIN_WINDOW_WIDTH,
                crate::editor::theme::MIN_WINDOW_HEIGHT,
            ));

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("No se pudo crear la ventana del editor"),
        );

        // El motor se cuelga de la ventana egui, no de Electron: usamos el HWND
        // del editor y el rectángulo central del marco.
        if let Some(viewport) = crate::editor::engine_viewport::central_viewport(window.as_ref()) {
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

                if self.cli.context.is_3d() {
                    // Ventana única: el motor 3D vive en este proceso y usa nuestro `SharedGpu`.
                    if let Some(editor) = self.editor.as_mut() {
                        editor.app.proxy = Some(self.proxy.clone());
                        editor.app.push_log("[Editor] ventana lista", false);
                        if let Err(error) = editor.render_loading() {
                            log::warn!("[editor] no se pudo pintar el frame de carga: {error}");
                        }
                        match editor.start_in_process_engine(self.proxy.clone()) {
                            Ok(()) => {
                                editor.app.push_log("[Editor] motor 3D en proceso", false);
                            }
                            Err(message) => {
                                log::error!("[editor] {message}");
                                editor.app.status = EditorStatus::Error;
                                editor.app.push_log(format!("[Editor] {message}"), true);
                            }
                        }
                    }
                } else {
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
                }

                crate::editor::bridge::notify_editor_ready();
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

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if let winit::event::DeviceEvent::MouseMotion { delta } = event
            && let Some(editor) = self.editor.as_mut()
        {
            editor.device_motion(delta.0, delta.1);
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
        // Al motor solo le llega lo que ocurre dentro del viewport (o todo en play).
        editor.forward_input(&event, response.consumed);
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

    /// Mantiene el editor a ~60 fps: el motor en proceso necesita frames continuos y
    /// egui solo pide repintar cuando hay animaciones o cambios.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let frame_duration = Duration::from_nanos(1_000_000_000 / self.target_fps.max(1));
        let now = Instant::now();
        if now >= self.next_frame_at {
            if let Some(editor) = self.editor.as_ref() {
                editor.window.request_redraw();
            }
            self.next_frame_at += frame_duration;
            if self.next_frame_at < now {
                self.next_frame_at = now + frame_duration;
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_at));
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

pub fn run() {
    rer_engine_core::logging::init(DEFAULT_LOG_FILTER);

    let cli = Cli::parse_from(std::env::args());
    log::info!("[editor] {}", Cli::usage());

    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("No se pudo crear el EventLoop del editor");

    let proxy = event_loop.create_proxy();
    spawn_electron_reader(proxy.clone());

    let mut application = EditorApplication {
        cli,
        proxy,
        editor: None,
        engine_host: None,
        engine_tx: None,
        next_frame_at: Instant::now(),
        target_fps: 60,
    };
    event_loop
        .run_app(&mut application)
        .expect("Error en el event loop del editor");
}
