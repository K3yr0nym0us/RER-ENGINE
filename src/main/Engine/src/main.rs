mod ecs;
mod engine;
mod gizmo;
mod ipc;
mod mesh;
mod scripting;
mod texture;

// ── Módulos de lógica de escena separados por modo ───────────────────────────
#[path = "CONFIG_BASE/mod.rs"]   mod config_base;
#[path = "CONFIG_2D/mod.rs"]     mod config_2d;
#[path = "CONFIG_3D/mod.rs"]     mod config_3d;
#[path = "CONFIG_SHARED/mod.rs"] mod config_shared;

use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use ipc::{EngineCommand, EngineEvent};

// ---------------------------------------------------------------------------
// Hilo nativo Win32: position tracker (sólo Windows)
// ---------------------------------------------------------------------------
/// Offset en píxeles de pantalla entre la esquina superior-izquierda del padre
/// (Electron) y la esquina superior-izquierda del motor.
/// Actualizado atómicamente por user_event cuando llega SetBounds.
/// El tracker lee este offset en cada iteración para calcular la posición deseada,
/// eliminando la carrera entre IPC y el hilo de tracking.
#[cfg(target_os = "windows")]
pub type TrackerOffset = std::sync::Arc<(std::sync::atomic::AtomicI32, std::sync::atomic::AtomicI32)>;

/// Rastrea la posición de la ventana padre (Electron) en un hilo dedicado
/// y reposiciona la ventana del motor en tiempo real usando Win32 puro.
///
/// Algoritmo (offset-based, sin delta acumulado):
///   cada 8ms, obtiene la posición física del área de contenido del padre con
///   ClientToScreen(parent, {0,0}) — equivalente a getContentBounds() de Electron,
///   sin el "invisible resize border" DPI-aware que tiene GetWindowRect.
///   Si el motor no está en `content_origin + offset`, lo mueve con SetWindowPos.
/// Cuando se produce maximize/restore/cambio de monitor, Electron envía set_bounds
/// que actualiza el offset atómico y el tracker se alinea en el siguiente tick.
#[cfg(target_os = "windows")]
fn start_position_tracker(engine_hwnd: isize, parent_hwnd: isize, offset: TrackerOffset) {
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SetWindowPos,
        SWP_NOSIZE, SWP_NOZORDER, SWP_NOACTIVATE,
    };
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use std::sync::atomic::Ordering;

    let engine_hwnd = HWND(engine_hwnd);
    let parent_hwnd = HWND(parent_hwnd);

    std::thread::Builder::new()
        .name("position-tracker".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(8));
                unsafe {
                    // Usar ClientToScreen para obtener la posición del área de contenido
                    // (sin el invisible resize border DPI-aware de Win32).
                    // Si Electron cerró, ClientToScreen devuelve FALSE.
                    let mut pt = POINT { x: 0, y: 0 };
                    if !ClientToScreen(parent_hwnd, &mut pt).as_bool() {
                        break; // Electron cerró — terminar el hilo
                    }
                    let off_x = offset.0.load(Ordering::Relaxed);
                    let off_y = offset.1.load(Ordering::Relaxed);
                    let desired_x = pt.x + off_x;
                    let desired_y = pt.y + off_y;

                    let mut engine = RECT::default();
                    if GetWindowRect(engine_hwnd, &mut engine).is_ok() {
                        if engine.left != desired_x || engine.top != desired_y {
                            // SAFETY: ambos HWNDs son válidos mientras el motor esté activo.
                            let _ = SetWindowPos(
                                engine_hwnd,
                                HWND(0isize),
                                desired_x,
                                desired_y,
                                0, 0,
                                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                            );
                        }
                    }
                }
            }
        })
        .expect("No se pudo crear el hilo position-tracker");
}

#[cfg(not(target_os = "windows"))]
fn start_position_tracker(_e: isize, _p: isize, _o: std::sync::Arc<(std::sync::atomic::AtomicI32, std::sync::atomic::AtomicI32)>) {}

// ---------------------------------------------------------------------------
// Consulta de estado de teclado vía X11 (sin depender del foco de ventana)
// ---------------------------------------------------------------------------
#[cfg(target_os = "linux")]
fn query_ctrl_held_x11() -> bool {
    // SAFETY: llamadas estándar a libX11; Display se abre y cierra en la misma función.
    unsafe {
        let display = x11::xlib::XOpenDisplay(std::ptr::null());
        if display.is_null() { return false; }
        let mut keys = [0u8; 32];
        x11::xlib::XQueryKeymap(display, keys.as_mut_ptr() as *mut i8);
        x11::xlib::XCloseDisplay(display);
        // Keycode 37 = Control_L, keycode 105 = Control_R (estándar X11 en Linux)
        let lctrl = (keys[37 / 8] >> (37 % 8)) & 1;
        let rctrl = (keys[105 / 8] >> (105 % 8)) & 1;
        lctrl != 0 || rctrl != 0
    }
}

#[cfg(not(target_os = "linux"))]
fn query_ctrl_held_x11() -> bool { false }

// ---------------------------------------------------------------------------
// Configuración de embedding (Fase 2)
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct EmbedConfig {
    pub parent_xid: u64,
    pub x:          i32,
    pub y:          i32,
    pub width:      u32,
    pub height:     u32,
    /// Offset físico del EngineView dentro del área de contenido de Electron.
    /// Pasado desde Electron como `bounds.x / bounds.y` (rect * devicePixelRatio),
    /// garantizando que el DPR del monitor actual esté aplicado.
    pub rel_x:      i32,
    pub rel_y:      i32,
}

fn parse_embed_config() -> Option<EmbedConfig> {
    // Espera: --embed <xid> <x> <y> <width> <height> [rel_x rel_y]
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 7 && args[1] == "--embed" {
        Some(EmbedConfig {
            parent_xid: args[2].parse().ok()?,
            x:          args[3].parse().ok()?,
            y:          args[4].parse().ok()?,
            width:      args[5].parse().ok()?,
            height:     args[6].parse().ok()?,
            rel_x: args.get(7).and_then(|a| a.parse().ok()).unwrap_or(0),
            rel_y: args.get(8).and_then(|a| a.parse().ok()).unwrap_or(0),
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Estructura principal de la aplicación winit
// ---------------------------------------------------------------------------
struct App {
    state:           Option<engine::State>,
    embed:           Option<EmbedConfig>,
    // ── Cámara orbital
    mouse_right:     bool,   // botón derecho  → orbitar
    mouse_middle:    bool,   // botón central  → pan
    last_cursor:     Option<(f32, f32)>,
    // Picking con click izquierdo
    left_click_pos:  Option<(f32, f32)>,  // posición al presionar
    // Drag de gizmo
    gizmo_drag_axis: Option<usize>,       // eje activo (0=X,1=Y,2=Z)
    // Teclas modificadoras
    ctrl_held:       bool,                // Ctrl izquierdo o derecho presionado
    // Frame rate cap: tiempo objetivo del próximo frame (evita busy loop)
    next_frame_at:   std::time::Instant,
    // Windows: offset compartido con el hilo position-tracker.
    // Actualizado en SetBounds para sincronizar maximize/monitor-change.
    tracker_offset:     std::sync::Arc<(std::sync::atomic::AtomicI32, std::sync::atomic::AtomicI32)>,
    tracker_parent_hwnd: isize,
}

impl ApplicationHandler<EngineCommand> for App {
    /// Llamado al iniciar (y al volver de suspensión en móvil).
    /// Aquí creamos la ventana y el estado wgpu.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() { return; }

        // Atributos base
        let mut attrs = Window::default_attributes()
            .with_title("RER-ENGINE — Viewport");

        if let Some(embed) = &self.embed {
            // ── Modo embebido ────────────────────────────────────────────────
            attrs = attrs
                .with_inner_size(winit::dpi::PhysicalSize::new(embed.width, embed.height))
                .with_position(winit::dpi::PhysicalPosition::new(embed.x, embed.y))
                .with_decorations(false)
                .with_resizable(false);

            #[cfg(target_os = "linux")]
            {
                use winit::platform::x11::WindowAttributesExtX11;
                // parent_xid == 0 cuando se corre desde Windows/plataforma sin XID real
                if embed.parent_xid != 0 {
                    attrs = attrs.with_embed_parent_window(embed.parent_xid as u32);
                }
            }
            #[cfg(target_os = "windows")]
            {
                // En Windows NO se usa with_parent_window: winit añade WS_CHILD y la
                // superficie de Chromium queda encima interceptando todos los eventos.
                // En su lugar se crea un WS_POPUP normal y se asigna Electron como
                // owner vía Win32 después de la creación (ver bloque post-creación).
            }
        } else {
            // ── Modo standalone ──────────────────────────────────────────────
            attrs = attrs
                .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32))
                .with_decorations(true);
        }

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("No se pudo crear la ventana"),
        );

        // Windows: asignar Electron como owner del popup y añadir WS_EX_NOACTIVATE.
        // Owned popup: queda visualmente encima de Electron sin ser WS_CHILD,
        // por lo que la superficie de Chromium no puede interceptar sus eventos.
        #[cfg(target_os = "windows")]
        if let Some(embed) = &self.embed {
            if embed.parent_xid != 0 {
                use raw_window_handle::HasWindowHandle;
                use windows::Win32::Foundation::HWND;
                use windows::Win32::UI::WindowsAndMessaging::{
                    GetWindowLongPtrW, SetWindowLongPtrW,
                    GWL_EXSTYLE, GWLP_HWNDPARENT, WS_EX_NOACTIVATE,
                };
                if let Ok(handle) = window.window_handle() {
                    if let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
                        let motor_hwnd    = HWND(h.hwnd.get() as isize);
                        let electron_hwnd = HWND(embed.parent_xid as isize);
                        // SAFETY: ambos HWNDs son válidos y viven mientras el motor esté activo
                        unsafe {
                            // Electron como owner: la ventana del motor siempre queda encima
                            SetWindowLongPtrW(motor_hwnd, GWLP_HWNDPARENT, electron_hwnd.0 as isize);
                            // No robar foco de teclado a Electron al hacer click
                            let ex = GetWindowLongPtrW(motor_hwnd, GWL_EXSTYLE);
                            SetWindowLongPtrW(motor_hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize);
                        }
                        // Lanzar hilo position-tracker con offset inicial calculado
                        // usando ClientToScreen para el área de contenido del padre
                        // (sin invisible resize border), alineado con getContentBounds() de Electron.
                        let offset = unsafe {
                            use windows::Win32::Foundation::POINT;
                            use windows::Win32::Graphics::Gdi::ClientToScreen;
                            // Si Electron pasó rel_x/rel_y (offsets físicos del renderer),
                            // usarlos directamente: son el offset correcto sin conversión DPI.
                            let (off_x, off_y) = if self.embed.as_ref().map(|e| e.rel_x != 0 || e.rel_y != 0).unwrap_or(false) {
                                let rx = self.embed.as_ref().map(|e| e.rel_x).unwrap_or(0);
                                let ry = self.embed.as_ref().map(|e| e.rel_y).unwrap_or(0);
                                (rx, ry)
                            } else {
                                // Fallback: calcular desde ClientToScreen (funciona en monitor principal)
                                let mut pt = POINT { x: 0, y: 0 };
                                let _ = ClientToScreen(electron_hwnd, &mut pt);
                                let embed_x = self.embed.as_ref().map(|e| e.x).unwrap_or(0);
                                let embed_y = self.embed.as_ref().map(|e| e.y).unwrap_or(0);
                                (embed_x - pt.x, embed_y - pt.y)
                            };
                            std::sync::Arc::new((
                                std::sync::atomic::AtomicI32::new(off_x),
                                std::sync::atomic::AtomicI32::new(off_y),
                            ))
                        };
                        start_position_tracker(motor_hwnd.0, electron_hwnd.0, std::sync::Arc::clone(&offset));
                        self.tracker_offset = offset;
                        self.tracker_parent_hwnd = electron_hwnd.0;
                    }
                }
            }
        }

        let state = pollster::block_on(engine::State::new(Arc::clone(&window), self.embed.is_some()));

        // Notificar a Electron que el motor está listo
        ipc::send_event(&EngineEvent::Ready);

        self.state = Some(state);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, cmd: EngineCommand) {
        // El IPC thread envió un comando vía EventLoopProxy — procesar de inmediato.
        if matches!(cmd, EngineCommand::Shutdown) {
            event_loop.exit();
            return;
        }
        // Windows: cuando set_bounds llega (maximize, cambio de monitor, resize),
        // actualizar el offset del position-tracker ANTES de mover la ventana.
        // Así el tracker y set_bounds no pelean — ambos apuntan al mismo lugar.
        #[cfg(target_os = "windows")]
        if let EngineCommand::SetBounds { x, y, offset_x, offset_y, .. } = &cmd {
            if self.tracker_parent_hwnd != 0 {
                use windows::Win32::Foundation::{HWND, POINT};
                use windows::Win32::Graphics::Gdi::ClientToScreen;
                use std::sync::atomic::Ordering;
                // Si el comando trae offset_x/offset_y (offsets físicos del renderer),
                // usarlos directamente: son la fuente de verdad sin conversión DPI.
                if let (Some(ox), Some(oy)) = (offset_x, offset_y) {
                    self.tracker_offset.0.store(*ox, Ordering::Relaxed);
                    self.tracker_offset.1.store(*oy, Ordering::Relaxed);
                } else {
                    // Fallback: calcular desde la posición absoluta y ClientToScreen
                    unsafe {
                        let mut pt = POINT { x: 0, y: 0 };
                        if ClientToScreen(HWND(self.tracker_parent_hwnd), &mut pt).as_bool() {
                            self.tracker_offset.0.store(x - pt.x, Ordering::Relaxed);
                            self.tracker_offset.1.store(y - pt.y, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
        if let Some(state) = self.state.as_mut() {
            state.handle_command(cmd);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        // ── Eventos de ventana ───────────────────────────────────────────────
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                state.resize(size);
            }
            // ── Input de ratón para cámara orbital ───────────────────────────
            WindowEvent::MouseInput { button, state: btn_state, .. } => {
                let pressed = btn_state == ElementState::Pressed;
                match button {
                    MouseButton::Left => {
                        if pressed {
                            // Comprobar si el click es sobre un eje del gizmo.
                            // Se omite en modo pivot para no robar el click al handler de pivot.
                            if let Some(cur) = self.last_cursor {
                                let axis = if state.pivot_edit_mode.is_none() {
                                    if state.camera_2d.is_some() {
                                        state.pick_gizmo_axis_2d(cur.0, cur.1)
                                    } else {
                                        state.pick_gizmo_axis(cur.0, cur.1)
                                    }
                                } else {
                                    None
                                };
                                self.gizmo_drag_axis = axis;
                                if axis.is_some() {
                                    state.set_active_gizmo_axis(axis);
                                }
                            }
                            if self.gizmo_drag_axis.is_none() {
                                // Guardar posición inicial del click izquierdo para picking normal
                                self.left_click_pos = self.last_cursor;
                            }
                        } else {
                            if self.gizmo_drag_axis.is_some() {
                                // Fin del drag de gizmo
                                self.gizmo_drag_axis = None;
                                state.set_active_gizmo_axis(None);
                            } else {
                                // Al soltar: si no hubo arrastre, disparar picking
                                if let (Some(start), Some(cur)) = (self.left_click_pos, self.last_cursor) {
                                    let dx = (cur.0 - start.0).abs();
                                    let dy = (cur.1 - start.1).abs();
                                    if dx < 5.0 && dy < 5.0 {
                                        if state.camera_2d.is_some() {
                                            if state.pivot_edit_mode.is_some() {
                                                state.handle_pivot_click_2d(cur.0, cur.1);
                                            } else if !state.handle_tool_click_2d(cur.0, cur.1) {
                                                state.pick_entity_2d(cur.0, cur.1);
                                            }
                                        } else {
                                            state.pick_entity(cur.0, cur.1);
                                        }
                                    }
                                }
                            }
                            self.left_click_pos = None;
                        }
                    }
                    MouseButton::Right  => {
                        self.mouse_right = pressed;
                        // Fin de pan: notificar posición actual de la cámara 2D
                        if !pressed {
                            if let Some(cam2d) = &state.camera_2d {
                                ipc::send_event(&EngineEvent::Camera2dUpdated {
                                    x:      cam2d.x,
                                    y:      cam2d.y,
                                    half_h: cam2d.half_h,
                                });
                            }
                        }
                    }
                    MouseButton::Middle => { self.mouse_middle = pressed; }
                    _ => {}
                }
                if !pressed && matches!(button, MouseButton::Right | MouseButton::Middle) {
                    self.last_cursor = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let cur = (position.x as f32, position.y as f32);
                if let Some((lx, ly)) = self.last_cursor {
                    let dx = cur.0 - lx;
                    let dy = cur.1 - ly;
                    if let Some(axis) = self.gizmo_drag_axis {
                        // Drag de gizmo: mover entidad a lo largo del eje
                        if state.camera_2d.is_some() {
                            // Combina ambas fuentes: winit (self.ctrl_held) e IPC (state.ctrl_held)
                            // + consulta directa X11 (no depende del foco de ventana)
                            let snap = self.ctrl_held || state.ctrl_held || query_ctrl_held_x11();
                            state.drag_gizmo_2d(cur.0, cur.1, lx, ly, axis, snap);
                        } else {
                            state.drag_gizmo(cur.0, cur.1, lx, ly, axis);
                        }
                    } else if self.mouse_right {
                        let (vw, vh) = { let s = state.size(); (s.width as f32, s.height as f32) };
                        if let Some(cam2d) = &mut state.camera_2d {
                            cam2d.pan(dx, dy, vw, vh);
                        } else {
                            state.camera.orbit(dx, dy);
                        }
                    } else if self.mouse_middle {
                        state.camera.pan(dx, dy);
                    }
                }
                // Hover: solo cuando no se está arrastrando
                if !self.mouse_right && !self.mouse_middle && self.gizmo_drag_axis.is_none() {
                    if state.camera_2d.is_some() {
                        state.update_hover_2d(cur.0, cur.1);
                    } else {
                        state.update_hover(cur.0, cur.1);
                    }
                }
                self.last_cursor = Some(cur);
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(code), state: key_state, .. },
                ..
            } => {
                let pressed = key_state == ElementState::Pressed;
                match code {
                    KeyCode::ControlLeft | KeyCode::ControlRight => {
                        self.ctrl_held = pressed;
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y)   => y,
                    MouseScrollDelta::PixelDelta(p)     => p.y as f32 * 0.05,
                };
                if let Some(cam2d) = &mut state.camera_2d {
                    // Zoom ortográfico: reducir/aumentar half_h
                    cam2d.half_h = (cam2d.half_h - scroll * 0.5).clamp(1.0, 50.0);
                    ipc::send_event(&EngineEvent::Camera2dUpdated {
                        x:      cam2d.x,
                        y:      cam2d.y,
                        half_h: cam2d.half_h,
                    });
                } else {
                    state.camera.zoom(scroll);
                }
            }
            WindowEvent::RedrawRequested => {
                state.update();
                match state.render() {
                    Ok(_) => {}
                    // Surface perdida: reconfigurar con el tamaño actual
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        let size = state.size();
                        state.resize(size);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        log::error!("Out of memory — cerrando");
                        event_loop.exit();
                    }
                    Err(e) => log::warn!("render error: {e:?}"),
                }
                // NO llamar request_redraw() aquí: lo hace about_to_wait con WaitUntil.
                // Hacerlo aquí + ControlFlow::Poll crea un busy loop que consume CPU al 100%.
            }
            _ => {}
        }
    }
    /// Llamado cuando winit ha procesado todos los eventos pendientes del ciclo actual.
    /// Es el único lugar correcto para pedir el siguiente frame en modo Poll.
    /// Usando WaitUntil capamos a ~60 fps y el CPU puede dormir entre frames.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        const TARGET_FPS: u64 = 60;
        const FRAME_DURATION: std::time::Duration =
            std::time::Duration::from_nanos(1_000_000_000 / TARGET_FPS);

        let now = std::time::Instant::now();
        if now >= self.next_frame_at {
            if let Some(state) = &self.state {
                state.window().request_redraw();
            }
            // Calcular el próximo tick desde el tiempo objetivo, no desde `now`,
            // para evitar drift acumulado si un frame tardó más de lo esperado.
            self.next_frame_at = self.next_frame_at + FRAME_DURATION;
            // Si nos retrasamos más de un frame, resincronizar para evitar
            // ráfagas de frames de recuperación.
            if self.next_frame_at < now {
                self.next_frame_at = now + FRAME_DURATION;
            }
        }
        // Dormir hasta el próximo frame en lugar de hacer busy-wait
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_at));
    }
}
fn main() {
    // Logs van a stderr; IPC usa stdout.
    // wgpu_hal::vulkan genera spam de "Suboptimal present" y warnings de capas
    // en entornos sin GPU hardware — subirlos a error los silencia.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(
            // wgpu_hal::gles genera spam de advertencias de rendimiento de buffers GPU
            // (copy VIDEO→HOST memory) en cada frame — silenciamos a nivel error.
            "info,wgpu_core=warn,wgpu_hal::vulkan=error,wgpu_hal::gles=error,wgpu_hal=warn,naga=warn",
        ),
    )
    .init();

    // Canal IPC: hilo stdin → event loop vía EventLoopProxy (despierta el loop inmediatamente)
    let event_loop = EventLoop::<EngineCommand>::with_user_event()
        .build()
        .expect("No se pudo crear EventLoop");
    let proxy = event_loop.create_proxy();
    ipc::start_ipc_thread(proxy);

    // ControlFlow se gestiona dinámicamente en about_to_wait con WaitUntil(next_frame).
    // NO usar Poll aquí: Poll + request_redraw en RedrawRequested = busy loop al 100% CPU.

    let embed = parse_embed_config();
    if embed.is_some() {
        log::info!("Modo embebido activado");
    }

    let mut app = App {
        state:               None,
        embed,
        mouse_right:         false,
        mouse_middle:        false,
        last_cursor:         None,
        left_click_pos:      None,
        gizmo_drag_axis:     None,
        ctrl_held:           false,
        next_frame_at:       std::time::Instant::now(),
        tracker_offset:      std::sync::Arc::new((
            std::sync::atomic::AtomicI32::new(0),
            std::sync::atomic::AtomicI32::new(0),
        )),
        tracker_parent_hwnd: 0,
    };
    event_loop.run_app(&mut app).expect("Error en el event loop");
}
