//! Host winit del editor egui de RER-ENGINE.

use std::io::BufRead;
use std::io::Write;
use std::sync::Arc;
use std::sync::mpsc;
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
use crate::editor::user_event::UserEvent;
use rer_engine_core::vulkan::VulkanOptions;

const DEFAULT_LOG_FILTER: &str =
    "rer_engine::editor=info,rer_engine_core::vulkan=info,ash=warn,egui_ash_renderer=warn";

/// Motor in-process del editor: 3D o 2D según el tipo de proyecto.
pub(crate) enum InProcessEngine {
    Three(Box<crate::engine3d::host::App>),
    Two(Box<crate::engine2d::host::App>),
}

impl InProcessEngine {
    pub(crate) fn tick(&mut self) {
        if let Self::Three(app) = self {
            app.tick();
        }
    }

    pub(crate) fn frame_update(&mut self) {
        match self {
            Self::Three(app) => app.frame_update(),
            Self::Two(app) => app.frame_update(),
        }
    }

    pub(crate) fn handle_command(&mut self, command: &Value) {
        match self {
            Self::Three(app) => {
                if let Some(parsed) = crate::engine3d::ipc::parse_command(command) {
                    app.handle_command(parsed);
                }
            }
            Self::Two(app) => {
                if let Some(parsed) = crate::engine2d::ipc::parse_command(command) {
                    app.handle_command(parsed);
                }
            }
        }
    }

    pub(crate) fn handle_window_event(&mut self, event: WindowEvent) {
        match self {
            Self::Three(app) => {
                app.handle_window_event(event);
            }
            Self::Two(app) => app.handle_window_event(event),
        }
    }

    pub(crate) fn handle_device_motion(&mut self, delta_x: f32, delta_y: f32) {
        if let Self::Three(app) = self {
            app.handle_device_motion(delta_x, delta_y);
        }
    }

    pub(crate) fn is_preview_playing(&self) -> bool {
        match self {
            Self::Three(app) => app.is_preview_playing(),
            Self::Two(app) => app.is_preview_playing(),
        }
    }

    pub(crate) fn is_cursor_captured(&self) -> bool {
        match self {
            Self::Three(app) => app.is_cursor_captured(),
            Self::Two(_) => false,
        }
    }

    pub(crate) fn release_cursor(&mut self) {
        if let Self::Three(app) = self {
            app.release_cursor_for_dialog();
        }
    }

    /// Libera el cursor al perder foco/ocluirse la ventana, aunque egui consuma el evento.
    pub(crate) fn release_cursor_for_focus_loss(&mut self) {
        if let Self::Three(app) = self {
            app.release_cursor_for_dialog();
        }
    }
}

/// Icono de ventana, el mismo PNG que usa Electron (`src/resources/RER-ENGINE-LOGO.png`).
const APP_ICON_PNG: &[u8] = include_bytes!("../../../../../../src/resources/RER-ENGINE-LOGO.png");

/// Tamaño inicial del viewport central antes del primer layout de egui.
fn initial_viewport_size(window: &Window) -> PhysicalSize<u32> {
    let inner = window.inner_size();
    let scale = window.scale_factor() as f32;
    let sidebar = (crate::editor::theme::SIDEBAR_WIDTH * scale).round() as u32;
    let top = (crate::editor::theme::TOP_BAR_HEIGHT * scale).round() as u32;
    let console = (inner.height as f32 * 0.25).max(120.0 * scale) as u32;
    PhysicalSize::new(
        inner.width.saturating_sub(sidebar * 2).max(1),
        inner.height.saturating_sub(top + console).max(1),
    )
}

/// Cierra el proceso de inmediato tras liberar el cursor y vaciar stdout.
///
/// El `Drop` del `State`/Vulkan tarda ~3 s (debug) en destruir cientos de recursos;
/// tras confirmar la salida o escribir el `.save` no hay nada pendiente que perder.
fn exit_editor_now() -> ! {
    rer_engine_core::platform::release_cursor_clip();
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    std::process::exit(0);
}

fn app_window_icon() -> Option<winit::window::Icon> {
    let image = image::load_from_memory(APP_ICON_PNG).ok()?;
    let resized = image.resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
    let rgba = resized.to_rgba8();
    let (width, height) = rgba.dimensions();
    winit::window::Icon::from_rgba(rgba.into_raw(), width, height).ok()
}

struct EditorWindow {
    window: Arc<Window>,
    /// Motor in-process (solo proyectos 3D o 2D). Se declara antes que `gpu` para que sus
    /// recursos Vulkan se liberen antes del device compartido.
    engine: Option<InProcessEngine>,
    gpu: crate::editor::render::EditorGpu,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    app: EditorApp,
    /// Congelado mientras Electron muestra la confirmación de salida.
    paused: bool,
}

impl EditorWindow {
    /// Rectángulo físico de la ventana nativa + escala, para anclar modales de Electron.
    fn engine_window_rect(&self) -> Option<serde_json::Value> {
        let pos = self.window.outer_position().ok()?;
        let size = self.window.outer_size();
        Some(serde_json::json!({
            "x": pos.x,
            "y": pos.y,
            "width": size.width,
            "height": size.height,
            "scale": self.window.scale_factor(),
        }))
    }

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
            paused: false,
        })
    }

    /// Lanza la construcción del motor del proyecto en un hilo aparte.
    fn spawn_engine_loader(&mut self, proxy: EventLoopProxy<UserEvent>) -> Result<(), String> {
        if self.app.cli.context.is_3d() {
            self.spawn_engine3d_loader(proxy)
        } else {
            self.spawn_engine2d_loader(proxy)
        }
    }

    /// Lanza la construcción del motor 3D en un hilo aparte.
    ///
    /// El event loop sigue libre: la UI pinta el spinner y la consola recibe los
    /// `load_progress` del motor en vivo. Al terminar llega `UserEvent::EngineReady`.
    fn spawn_engine3d_loader(&mut self, proxy: EventLoopProxy<UserEvent>) -> Result<(), String> {
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

        // El hilo de carga y el frame de espera comparten device: se desactiva el wait
        // serializado para que no se pisen queue/device mientras se construye el motor.
        unsafe { std::env::set_var("RER_VK_SYNC_FRAMES", "0") };

        // El sink debe existir antes de construir el estado: la carga inicial ya emite eventos.
        let (sink_tx, sink_rx) = mpsc::channel::<String>();
        crate::engine3d::ipc::install_event_sink(sink_tx);

        let events_proxy = proxy.clone();
        std::thread::Builder::new()
            .name("engine-events".into())
            .spawn(move || {
                for line in sink_rx {
                    // Mismo JSON hacia Electron: las modales no cambian.
                    crate::editor::bridge::forward_line(&line);
                    if let Ok(value) = serde_json::from_str::<Value>(&line)
                        && events_proxy
                            .send_event(UserEvent::EngineEvent(value))
                            .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| format!("no se pudo crear el hilo de eventos del motor: {error}"))?;

        let window = Arc::clone(&self.window);
        let gpu = self.gpu.shared_gpu();
        std::thread::Builder::new()
            .name("engine-init".into())
            .spawn(move || {
                let result = crate::engine3d::engine::State::with_context(window, gpu);
                match result {
                    Ok(mut state) => {
                        state.embedded = true;
                        let from_save = std::env::var("RER_3D_START_FROM_SAVE")
                            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                            .unwrap_or(false);
                        if from_save {
                            state.setup_empty_3d();
                            crate::engine3d::ipc::send_event(
                                &crate::engine3d::ipc::EngineEvent::Ready {
                                    gravity: state.physics.gravity_magnitude(),
                                },
                            );
                        } else {
                            state.setup_default_3d_scene();
                        }
                        let _ = proxy.send_event(UserEvent::EngineReady(Box::new(state)));
                    }
                    Err(error) => {
                        let _ = proxy.send_event(UserEvent::EngineFailed(error.message));
                    }
                }
            })
            .map_err(|error| format!("no se pudo crear el hilo de carga del motor: {error}"))?;

        Ok(())
    }

    /// Lanza la construcción del motor 2D en un hilo aparte sobre el `SharedGpu` del editor.
    fn spawn_engine2d_loader(&mut self, proxy: EventLoopProxy<UserEvent>) -> Result<(), String> {
        let cli = self.app.cli.clone();
        if let Some(save_path) = &cli.save_path {
            unsafe { std::env::set_var("RER_PROJECT_SAVE_PATH", save_path) };
        }
        if let Some(extract_dir) = &cli.extract_dir {
            unsafe { std::env::set_var("RER_PROJECT_EXTRACT_DIR", extract_dir) };
        }
        unsafe { std::env::set_var("RER_VK_SYNC_FRAMES", "0") };

        let (sink_tx, sink_rx) = mpsc::channel::<String>();
        crate::engine2d::ipc::install_event_sink(sink_tx);

        let events_proxy = proxy.clone();
        std::thread::Builder::new()
            .name("engine2d-events".into())
            .spawn(move || {
                for line in sink_rx {
                    crate::editor::bridge::forward_line(&line);
                    if let Ok(value) = serde_json::from_str::<Value>(&line)
                        && events_proxy
                            .send_event(UserEvent::EngineEvent(value))
                            .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| format!("no se pudo crear el hilo de eventos 2D: {error}"))?;

        let window = Arc::clone(&self.window);
        let gpu = self.gpu.shared_gpu();
        let save_path = cli.save_path.clone();
        let extract_dir = cli.extract_dir.clone();
        std::thread::Builder::new()
            .name("engine2d-init".into())
            .spawn(move || {
                let size = initial_viewport_size(window.as_ref());
                let result = (|| -> Result<_, String> {
                    let vulkan = Arc::new(
                        crate::engine2d::vulkan::Vulkan2d::with_gpu(gpu)
                            .map_err(|error| error.to_string())?,
                    );
                    let mut state = crate::engine2d::engine::State::with_vulkan(
                        Arc::clone(&window),
                        vulkan,
                        size,
                    )
                    .map_err(|error| error.message)?;
                    state.embedded = true;
                    state.setup_editor_project(save_path.as_deref(), extract_dir.as_deref());
                    Ok(state)
                })();
                match result {
                    Ok(state) => {
                        crate::engine2d::ipc::send_event(
                            &crate::engine2d::ipc::EngineEvent::Ready {
                                gravity: state.physics_2d.gravity_magnitude(),
                            },
                        );
                        let _ = proxy.send_event(UserEvent::Engine2dReady(Box::new(state)));
                    }
                    Err(message) => {
                        let _ = proxy.send_event(UserEvent::EngineFailed(message));
                    }
                }
            })
            .map_err(|error| format!("no se pudo crear el hilo de carga 2D: {error}"))?;
        Ok(())
    }

    /// El hilo de carga terminó: el motor pasa a ser propiedad del editor.
    fn attach_engine(&mut self, state: crate::engine3d::engine::State) {
        unsafe { std::env::remove_var("RER_VK_SYNC_FRAMES") };
        self.engine = Some(InProcessEngine::Three(Box::new(
            crate::engine3d::host::App::new_in_editor(state),
        )));
        self.app.engine_loading = false;
        self.app.engine_error = None;
        self.window.request_redraw();
    }

    /// El hilo de carga 2D terminó: el motor pasa a ser propiedad del editor.
    fn attach_engine2d(&mut self, state: crate::engine2d::engine::State) {
        unsafe { std::env::remove_var("RER_VK_SYNC_FRAMES") };
        self.engine = Some(InProcessEngine::Two(Box::new(
            crate::engine2d::host::App::new_in_editor(state),
        )));
        self.app.engine_loading = false;
        self.app.engine_error = None;
        self.window.request_redraw();
    }

    fn fail_engine(&mut self, message: String) {
        unsafe { std::env::remove_var("RER_VK_SYNC_FRAMES") };
        log::error!("[editor] {message}");
        self.app.engine_loading = false;
        self.app.engine_error = Some(message.clone());
        self.app.status = EditorStatus::Error;
        self.app.push_log(format!("[Editor] {message}"), true);
        self.window.request_redraw();
    }

    /// Comando de Electron que debe llegar al motor en proceso.
    fn handle_engine_command(&mut self, command: &Value) {
        if let Some(engine) = self.engine.as_mut() {
            engine.handle_command(command);
        }
    }

    /// Reenvía input a la escena cuando el puntero está en el viewport.
    fn forward_input(&mut self, event: &WindowEvent, egui_consumed: bool) {
        // `response.consumed` es global: también es true sobre el viewport vacío. Solo
        // se bloquea si egui está usando el puntero o hay una capa flotante (modal,
        // popup, menú) encima; si no, el motor debe recibir el input del viewport.
        if egui_consumed
            && (self.egui_ctx.egui_is_using_pointer()
                || self
                    .egui_ctx
                    .pointer_latest_pos()
                    .and_then(|pos| self.egui_ctx.layer_id_at(pos))
                    .is_some_and(|layer| layer != egui::LayerId::background()))
        {
            return;
        }
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
            WindowEvent::Focused(_) => {
                // El foco debe llegar siempre al motor: de él depende liberar el cursor.
                engine.handle_window_event(event.clone());
            }
            WindowEvent::KeyboardInput { .. }
            | WindowEvent::ModifiersChanged(_)
            | WindowEvent::Ime(_) => {
                if self.egui_ctx.egui_wants_keyboard_input() {
                    // Escape debe salir del modo play aunque egui tenga foco de teclado.
                    let escape_in_play = engine.is_preview_playing()
                        && matches!(
                            event,
                            WindowEvent::KeyboardInput { event: key_event, .. }
                                if key_event.physical_key
                                    == winit::keyboard::PhysicalKey::Code(
                                        winit::keyboard::KeyCode::Escape,
                                    )
                        );
                    if !escape_in_play {
                        return;
                    }
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

    fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), String> {
        self.gpu.resize(size)
    }

    fn render(&mut self) -> Result<(), String> {
        if self.paused {
            return Ok(());
        }
        let EditorWindow {
            window,
            engine,
            gpu,
            egui_ctx,
            egui_state,
            app,
            paused: _,
        } = self;

        if let Some(engine) = engine.as_mut() {
            let pending: Vec<Value> = app.pending_engine.borrow_mut().drain(..).collect();
            for command in pending {
                engine.handle_command(&command);
            }
            engine.tick();
            engine.frame_update();
        }

        // Rect del frame anterior: el layout de egui se conoce al terminar de construir la UI.
        let viewport = app.viewport_rect;
        // `about_to_wait` es el único que marca el siguiente frame: pedir otro redraw aquí
        // saltaba el `WaitUntil` y el loop corría a la frecuencia del monitor (200 Hz).
        let _ = gpu.render(
            window.as_ref(),
            egui_ctx,
            egui_state,
            |ui| app.ui(ui),
            engine.as_mut(),
            viewport,
        )?;
        Ok(())
    }
}

struct EditorApplication {
    cli: Cli,
    proxy: EventLoopProxy<UserEvent>,
    editor: Option<EditorWindow>,
    next_frame_at: Instant,
    target_fps: u64,
    /// Tras guardar desde la modal de salida, cerrar el editor.
    exit_after_save: bool,
    /// Hay una confirmación de salida abierta en Electron.
    exit_request_pending: bool,
    /// Sincronización de pantallas UI recibida antes de crear la ventana.
    pending_ui_screens: Option<Value>,
}

impl EditorApplication {
    fn send_engine(&mut self, command: Value) {
        // Motor in-process: si ya está cargado se despacha ya; si no, se encola para
        // el primer frame tras `attach_engine`.
        if let Some(editor) = self.editor.as_mut() {
            if editor.engine.is_some() {
                editor.handle_engine_command(&command);
            } else {
                editor.app.send_engine(command);
            }
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
                // La geometría del motor la manda el editor; ya no hay overlay que mover.
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
            "ui_screens_sync" => {
                if let Some(screens) = message.get("screens").cloned() {
                    if let Some(editor) = self.editor.as_mut() {
                        editor.app.ui_screens.apply_sync(&screens);
                        editor.window.request_redraw();
                    } else {
                        self.pending_ui_screens = Some(screens);
                    }
                }
            }
            "quick_build_state" => {
                let state = message.get("state");
                let active = state
                    .and_then(|state| state.get("active"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let name = state
                    .and_then(|state| state.get("name"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if let Some(editor) = self.editor.as_mut() {
                    editor.app.quick_build_active = if active {
                        Some(name.unwrap_or_else(|| "Quick build".to_string()))
                    } else {
                        None
                    };
                    editor.window.request_redraw();
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
            "exit_cancelled" => {
                self.exit_request_pending = false;
                if let Some(editor) = self.editor.as_mut() {
                    editor.paused = false;
                    editor.window.request_redraw();
                }
            }
            "exit_after_save" => {
                self.exit_request_pending = false;
                if let Some(editor) = self.editor.as_mut() {
                    editor.paused = false;
                    if editor.app.can_save() {
                        if editor.app.begin_save(false).is_some() {
                            self.exit_after_save = true;
                        }
                    } else {
                        return true;
                    }
                    editor.window.request_redraw();
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

        match EditorWindow::new(Arc::clone(&window), self.cli.clone()) {
            Ok(editor) => {
                log::info!(
                    "[editor] ventana lista (proyecto={}, estilo={}, locale={})",
                    self.cli.context.project_type.as_str(),
                    self.cli.context.game_style.as_str(),
                    self.cli.locale.as_str()
                );
                self.editor = Some(editor);
                window.focus_window();

                if let Some(pending) = self.pending_ui_screens.take()
                    && let Some(editor) = self.editor.as_mut()
                {
                    editor.app.ui_screens.apply_sync(&pending);
                }

                // La UI se pinta ya; el motor se construye en un hilo y avisa al terminar.
                if let Some(editor) = self.editor.as_mut() {
                    editor.app.proxy = Some(self.proxy.clone());
                    editor.app.engine_loading = true;
                    editor.app.engine_error = None;
                    let engine_label = if self.cli.context.is_3d() { "3D" } else { "2D" };
                    editor
                        .app
                        .push_log(format!("[Editor] cargando motor {engine_label}…"), false);
                    if let Err(message) = editor.spawn_engine_loader(self.proxy.clone()) {
                        editor.fail_engine(message);
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

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
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
                    exit_editor_now();
                }
                if let Some(editor) = self.editor.as_mut() {
                    editor.window.request_redraw();
                }
            }
            UserEvent::SaveFinished(result) => {
                let success = result.is_ok();
                if let Some(editor) = self.editor.as_mut() {
                    editor.app.complete_save(result);
                    editor.window.request_redraw();
                }
                if self.exit_after_save {
                    if success {
                        exit_editor_now();
                    } else {
                        self.exit_after_save = false;
                    }
                }
            }
            UserEvent::SaveTimeout(id) => {
                self.exit_after_save = false;
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
            UserEvent::EngineReady(state) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.attach_engine(*state);
                    editor.app.push_log("[Editor] motor 3D listo", false);
                }
            }
            UserEvent::Engine2dReady(state) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.attach_engine2d(*state);
                    editor.app.push_log("[Editor] motor 2D listo", false);
                }
            }
            UserEvent::EngineFailed(message) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.fail_engine(message);
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
        _event_loop: &ActiveEventLoop,
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

        // Al perder foco u ocultarse la ventana, soltar siempre el clip del cursor
        // antes de que egui pueda consumir el evento.
        if matches!(
            event,
            WindowEvent::Focused(false) | WindowEvent::Occluded(true)
        ) && let Some(editor) = self.editor.as_mut()
            && let Some(engine) = editor.engine.as_mut()
        {
            engine.release_cursor_for_focus_loss();
        }

        if matches!(event, WindowEvent::CloseRequested) {
            if !self.cli.electron_host {
                exit_editor_now();
            }
            if self.exit_request_pending {
                return;
            }
            self.exit_request_pending = true;
            let mut engine_rect = None;
            if let Some(editor) = self.editor.as_mut() {
                editor.paused = true;
                engine_rect = editor.engine_window_rect();
                if let Some(engine) = editor.engine.as_mut() {
                    engine.release_cursor();
                }
            }
            crate::editor::bridge::request_exit_confirmation(engine_rect);
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
                editor.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let size = editor.window.inner_size();
                if let Err(error) = editor.resize(size) {
                    log::error!("[editor] {error}");
                }
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

    /// Mantiene el editor al FPS configurado por el proyecto: el motor en proceso
    /// necesita frames continuos y egui solo pide repintar cuando hay animaciones/cambios.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.editor.as_ref().is_some_and(|editor| editor.paused) {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        let now = Instant::now();
        // El limitador de la UI (`set_target_fps`) se aplica aquí: antes el editor
        // seguía siempre a 60 y el motor recibía `update()` a la frecuencia del monitor.
        let requested = self
            .editor
            .as_ref()
            .map_or(self.target_fps, |editor| {
                u64::from(editor.app.world.target_fps)
            })
            .clamp(1, 1000);
        if requested != self.target_fps {
            self.target_fps = requested;
            self.next_frame_at = now;
        }

        let frame_duration = Duration::from_nanos(1_000_000_000 / self.target_fps.max(1));
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
        next_frame_at: Instant::now(),
        target_fps: 60,
        exit_after_save: false,
        exit_request_pending: false,
        pending_ui_screens: None,
    };
    event_loop
        .run_app(&mut application)
        .expect("Error en el event loop del editor");
}
