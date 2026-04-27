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

use std::sync::{Arc, mpsc};

use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use ipc::{EngineCommand, EngineEvent};

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
}

fn parse_embed_config() -> Option<EmbedConfig> {
    // Espera: --embed <xid> <x> <y> <width> <height>
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 7 && args[1] == "--embed" {
        Some(EmbedConfig {
            parent_xid: args[2].parse().ok()?,
            x:          args[3].parse().ok()?,
            y:          args[4].parse().ok()?,
            width:      args[5].parse().ok()?,
            height:     args[6].parse().ok()?,
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
    rx:              mpsc::Receiver<EngineCommand>,
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
}

impl ApplicationHandler for App {
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
                    }
                }
            }
        }

        let state = pollster::block_on(engine::State::new(Arc::clone(&window), self.embed.is_some()));

        // Notificar a Electron que el motor está listo
        ipc::send_event(&EngineEvent::Ready);

        self.state = Some(state);
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

        // ── Procesar comandos IPC pendientes ─────────────────────────────────
        while let Ok(cmd) = self.rx.try_recv() {
            if matches!(cmd, EngineCommand::Shutdown) {
                event_loop.exit();
                return;
            }
            state.handle_command(cmd);
        }

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
            "info,wgpu_core=warn,wgpu_hal::vulkan=error,wgpu_hal=warn,naga=warn",
        ),
    )
    .init();

    // Canal IPC: hilo stdin → event loop
    let (tx, rx) = mpsc::channel::<EngineCommand>();
    ipc::start_ipc_thread(tx);

    let event_loop = EventLoop::new().expect("No se pudo crear EventLoop");
    // ControlFlow se gestiona dinámicamente en about_to_wait con WaitUntil(next_frame).
    // NO usar Poll aquí: Poll + request_redraw en RedrawRequested = busy loop al 100% CPU.

    let embed = parse_embed_config();
    if embed.is_some() {
        log::info!("Modo embebido activado");
    }

    let mut app = App { state: None, rx, embed, mouse_right: false, mouse_middle: false, last_cursor: None, left_click_pos: None, gizmo_drag_axis: None, ctrl_held: false, next_frame_at: std::time::Instant::now() };
    event_loop.run_app(&mut app).expect("Error en el event loop");
}
