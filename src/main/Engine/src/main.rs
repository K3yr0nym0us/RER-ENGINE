mod ecs;
mod engine;
mod gizmo;
mod ipc;
mod mesh;
mod scripting;
mod spatial;
mod texture;

// ── Módulos de lógica de escena separados por modo ───────────────────────────
#[path = "CONFIG_BASE/mod.rs"]   mod config_base;
#[path = "CONFIG_2D/mod.rs"]     mod config_2d;
#[path = "CONFIG_3D/mod.rs"]     mod config_3d;
#[path = "CONFIG_SHARED/mod.rs"] mod config_shared;

use std::collections::HashSet;
use std::sync::Arc;

use gilrs::{Button as GamepadButton, EventType as GamepadEventType, Gilrs};

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

/// En Windows usamos GetAsyncKeyState para consultar el estado real del Ctrl
/// sin depender del foco de ventana. Esto evita el bug de "toggle" que ocurre
/// cuando Electron pierde el foco al hacer click en el viewport del motor y
/// el keyup de Control nunca llega al renderer.
#[cfg(target_os = "windows")]
fn query_ctrl_held_x11() -> bool {
    // SAFETY: GetAsyncKeyState es seguro de llamar en cualquier contexto Win32.
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LCONTROL, VK_RCONTROL};
        let left  = (GetAsyncKeyState(VK_LCONTROL.0 as i32) as u16 & 0x8000) != 0;
        let right = (GetAsyncKeyState(VK_RCONTROL.0 as i32) as u16 & 0x8000) != 0;
        left || right
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn query_ctrl_held_x11() -> bool { false }

/// Convierte un `KeyCode` de winit a la string de control usada en los bindings.
///
/// Usa el nombre del variant (via Debug) para mapear automáticamente cualquier tecla
/// de letra (KeyA-KeyZ → "A"-"Z") y dígito (Digit0-Digit9 → "0"-"9") sin necesitar
/// actualizar este archivo al agregar nuevas teclas en el frontend.
fn map_keyboard_control_key(code: KeyCode) -> Option<String> {
    let debug = format!("{code:?}");

    // Letras: "KeyA" → "A", "KeyZ" → "Z"
    if let Some(letter) = debug.strip_prefix("Key") {
        if letter.len() == 1 && letter.as_bytes()[0].is_ascii_alphabetic() {
            return Some(letter.to_uppercase());
        }
    }

    // Dígitos: "Digit0" → "0", "Digit9" → "9"
    if let Some(digit) = debug.strip_prefix("Digit") {
        if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() {
            return Some(digit.to_string());
        }
    }

    // Teclas especiales con nombre distinto al variant
    match code {
        KeyCode::Space        => Some("SPACE".to_string()),
        KeyCode::ControlLeft
        | KeyCode::ControlRight => Some("CTRL".to_string()),
        KeyCode::ShiftLeft
        | KeyCode::ShiftRight   => Some("SHIFT".to_string()),
        KeyCode::AltLeft
        | KeyCode::AltRight     => Some("ALT".to_string()),
        _ => None,
    }
}

fn map_mouse_control_key(button: MouseButton) -> Option<&'static str> {
    match button {
        MouseButton::Left => Some("MOUSE_LEFT"),
        MouseButton::Right => Some("MOUSE_RIGHT"),
        MouseButton::Middle => Some("MOUSE_MIDDLE"),
        _ => None,
    }
}

fn map_gamepad_control_key(button: GamepadButton) -> Option<&'static str> {
    match button {
        GamepadButton::South => Some("A"),
        GamepadButton::East => Some("B"),
        GamepadButton::West => Some("X"),
        GamepadButton::North => Some("Y"),
        GamepadButton::LeftTrigger => Some("LB"),
        GamepadButton::RightTrigger => Some("RB"),
        GamepadButton::LeftTrigger2 => Some("LT"),
        GamepadButton::RightTrigger2 => Some("RT"),
        GamepadButton::Select => Some("BACK"),
        GamepadButton::Start => Some("START"),
        GamepadButton::LeftThumb => Some("L3"),
        GamepadButton::RightThumb => Some("R3"),
        GamepadButton::DPadUp => Some("D-UP"),
        GamepadButton::DPadDown => Some("D-DOWN"),
        GamepadButton::DPadLeft => Some("D-LEFT"),
        GamepadButton::DPadRight => Some("D-RIGHT"),
        _ => None,
    }
}

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
    gizmo_drag_start: Option<Vec<(u32, [f32; 3], [f32; 4], [f32; 3])>>,
    // Teclas modificadoras
    ctrl_held:       bool,                // Ctrl izquierdo o derecho presionado
    keyboard_mouse_pressed: HashSet<String>,
    // Input de mando (gamepad)
    gilrs:           Option<Gilrs>,
    gamepad_pressed: HashSet<GamepadButton>,
    // Frame rate cap: tiempo objetivo del próximo frame (evita busy loop)
    next_frame_at:   std::time::Instant,
    #[cfg(target_os = "windows")]
    // Windows: offset compartido con el hilo position-tracker.
    // Actualizado en SetBounds para sincronizar maximize/monitor-change.
    tracker_offset:     std::sync::Arc<(std::sync::atomic::AtomicI32, std::sync::atomic::AtomicI32)>,
    #[cfg(target_os = "windows")]
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
        // When entering game mode, automatically focus the engine window
        // so keyboard/mouse input is captured without requiring a manual click.
        let entering_play = matches!(cmd, EngineCommand::SetPreviewPlaying { playing: true });

        if let Some(state) = self.state.as_mut() {
            state.handle_command(cmd);
            if entering_play {
                state.window().focus_window();
                log::info!("[preview] foco transferido a la ventana del motor");
            }
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
                if state.is_preview_playing() {
                    if let Some(control_key) = map_mouse_control_key(button) {
                        if pressed {
                            if self.keyboard_mouse_pressed.insert(control_key.to_string()) {
                                state.handle_runtime_control_input("keyboard_mouse", control_key);
                            }
                        } else {
                            self.keyboard_mouse_pressed.remove(control_key);
                        }
                    }
                }
                match button {
                    MouseButton::Left => {
                        if state.is_preview_playing() {
                            self.left_click_pos = None;
                            self.gizmo_drag_axis = None;
                            state.set_active_gizmo_axis(None);
                            state.set_snap_hint_visible(false);
                            return;
                        }
                        if pressed {
                            // En modo quick_build_place los clicks son capturados por la herramienta;
                            // no se deben seleccionar entidades ni activar el gizmo.
                            let is_quick_build = matches!(state.active_tool, crate::config_2d::ActiveTool::QuickBuildPlace { .. });

                            // Comprobar si el click es sobre un eje del gizmo.
                            // Se omite en modo pivot/quick_build para no robar el click al handler.
                            if let Some(cur) = self.last_cursor {
                                let axis = if state.pivot_edit_mode.is_none() && !is_quick_build {
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
                                    state.set_snap_hint_visible(state.camera_2d.is_some());
                                    let selected_ids: Vec<u32> = if !state.selected_entities.is_empty() {
                                        state.selected_entities.clone()
                                    } else {
                                        state.selected_entity.into_iter().collect()
                                    };
                                    let mut snapshots: Vec<(u32, [f32; 3], [f32; 4], [f32; 3])> = Vec::new();
                                    for sel_id in selected_ids {
                                        if let Some(t) = state.world.get::<crate::ecs::Transform>(sel_id) {
                                            snapshots.push((
                                                sel_id,
                                                t.position.to_array(),
                                                [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                                                t.scale.to_array(),
                                            ));
                                        }
                                    }
                                    self.gizmo_drag_start = Some(snapshots);
                                }
                            }
                            if self.gizmo_drag_axis.is_none() {
                                state.set_snap_hint_visible(false);
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
                                state.set_snap_hint_visible(false);
                                if let Some(start_snapshots) = self.gizmo_drag_start.take() {
                                    let mut changed_snapshots: Vec<(u32, [f32; 3], [f32; 4], [f32; 3])> = Vec::new();
                                    for (id, pos, rot, scl) in start_snapshots {
                                        if let Some(t) = state.world.get::<crate::ecs::Transform>(id) {
                                            let changed = t.position.to_array() != pos
                                                || [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w] != rot
                                                || t.scale.to_array() != scl;
                                            if changed {
                                                changed_snapshots.push((id, pos, rot, scl));
                                            }
                                        }
                                    }
                                    if !changed_snapshots.is_empty() {
                                        if changed_snapshots.len() == 1 {
                                            let (id, pos, rot, scl) = changed_snapshots[0];
                                            state.push_undo_transform(id, pos, rot, scl);
                                        } else {
                                            state.push_undo_transforms(changed_snapshots);
                                        }
                                    }
                                }
                            } else {
                                // Al soltar: si no hubo arrastre, disparar picking
                                if let (Some(start), Some(cur)) = (self.left_click_pos, self.last_cursor) {
                                    let dx = (cur.0 - start.0).abs();
                                    let dy = (cur.1 - start.1).abs();
                                    if dx < 5.0 && dy < 5.0 {
                                        // Consultar el estado real del Ctrl al momento del click.
                                        // Usar query_ctrl_held_x11() (Windows: GetAsyncKeyState,
                                        // Linux: XQueryKeymap) como fuente autoritativa del OS,
                                        // sin releer state.ctrl_held que podría estar obsoleto si
                                        // Electron perdió el foco y el keyup no llegó.
                                        let ctrl_active = self.ctrl_held || query_ctrl_held_x11();
                                        state.ctrl_held = ctrl_active;
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
                if state.is_preview_playing() {
                    self.gizmo_drag_axis = None;
                    self.gizmo_drag_start = None;
                    state.set_active_gizmo_axis(None);
                    state.set_snap_hint_visible(false);
                }
                if state.camera_2d.is_some() && !state.is_preview_playing() {
                    // Usar OS query directa: evita que state.ctrl_held obsoleto se propague.
                    let ctrl_active = self.ctrl_held || query_ctrl_held_x11();
                    state.ctrl_held = ctrl_active;
                    state.update_tool_overlay_cursor_2d(cur.0, cur.1);
                }
                if let Some((lx, ly)) = self.last_cursor {
                    let dx = cur.0 - lx;
                    let dy = cur.1 - ly;
                    if !state.is_preview_playing() {
                        if let Some(axis) = self.gizmo_drag_axis {
                        // Drag de gizmo: mover entidad a lo largo del eje
                        if state.camera_2d.is_some() {
                            // OS query directa para snap: independiente del foco de ventana.
                            let snap = self.ctrl_held || query_ctrl_held_x11();
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
                }
                // Hover: solo cuando no se está arrastrando
                if !state.is_preview_playing() && !self.mouse_right && !self.mouse_middle && self.gizmo_drag_axis.is_none() {
                    let is_quick_build = matches!(state.active_tool, crate::config_2d::ActiveTool::QuickBuildPlace { .. });
                    if state.camera_2d.is_some() && !is_quick_build {
                        state.update_hover_2d(cur.0, cur.1);
                    } else if state.camera_2d.is_none() {
                        state.update_hover(cur.0, cur.1);
                    }
                }
                self.last_cursor = Some(cur);
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(code), state: key_state, repeat, .. },
                ..
            } => {
                let pressed = key_state == ElementState::Pressed;
                if state.is_preview_playing() {
                    if let Some(control_key) = map_keyboard_control_key(code) {
                        if pressed {
                            if self.keyboard_mouse_pressed.insert(control_key.clone()) {
                                state.handle_runtime_control_input("keyboard_mouse", &control_key);
                            }
                        } else {
                            self.keyboard_mouse_pressed.remove(&control_key);
                        }
                    }
                }
                match code {
                    KeyCode::ControlLeft | KeyCode::ControlRight => {
                        self.ctrl_held = pressed;
                    }
                    KeyCode::KeyZ => {
                        if pressed && !repeat {
                            let ctrl_active = self.ctrl_held || query_ctrl_held_x11();
                            if ctrl_active {
                                state.handle_command(EngineCommand::Undo);
                            }
                        }
                    }
                    KeyCode::KeyY => {
                        if pressed && !repeat {
                            let ctrl_active = self.ctrl_held || query_ctrl_held_x11();
                            if ctrl_active {
                                state.handle_command(EngineCommand::Redo);
                            }
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::Focused(false) => {
                self.keyboard_mouse_pressed.clear();
                self.gamepad_pressed.clear();
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
        if let Some(state) = self.state.as_mut() {
            if state.is_preview_playing() {
                if let Some(gilrs) = self.gilrs.as_mut() {
                    while let Some(evt) = gilrs.next_event() {
                        match evt.event {
                            GamepadEventType::ButtonPressed(button, _) => {
                                if self.gamepad_pressed.insert(button) {
                                    if let Some(control_key) = map_gamepad_control_key(button) {
                                        state.handle_runtime_control_input("gamepad", control_key);
                                    }
                                }
                            }
                            GamepadEventType::ButtonReleased(button, _) => {
                                self.gamepad_pressed.remove(&button);
                            }
                            _ => {}
                        }
                    }
                }

                for control_key in &self.keyboard_mouse_pressed {
                    state.handle_runtime_control_input("keyboard_mouse", control_key);
                }
                for button in &self.gamepad_pressed {
                    if let Some(control_key) = map_gamepad_control_key(*button) {
                        state.handle_runtime_control_input("gamepad", control_key);
                    }
                }

            } else {
                self.keyboard_mouse_pressed.clear();
                self.gamepad_pressed.clear();
            }
        }

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
            // Por defecto dejamos solo advertencias/errores para un arranque limpio.
            // Quien necesite más detalle puede usar RUST_LOG=info o RUST_LOG=debug.
            // Además, wgpu_hal::gles/vulkan generan spam en algunos entornos.
            "warn,wgpu_core=warn,wgpu_hal::vulkan=error,wgpu_hal::gles=error,wgpu_hal=warn,naga=warn",
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
        gizmo_drag_start:    None,
        ctrl_held:           false,
        keyboard_mouse_pressed: HashSet::new(),
        gilrs:               Gilrs::new().ok(),
        gamepad_pressed:     HashSet::new(),
        next_frame_at:       std::time::Instant::now(),
        #[cfg(target_os = "windows")]
        tracker_offset:      std::sync::Arc::new((
            std::sync::atomic::AtomicI32::new(0),
            std::sync::atomic::AtomicI32::new(0),
        )),
        #[cfg(target_os = "windows")]
        tracker_parent_hwnd: 0,
    };
    event_loop.run_app(&mut app).expect("Error en el event loop");
}
