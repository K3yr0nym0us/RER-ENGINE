mod taa;
mod ecs;
mod entity_save_meta;
mod gizmo;
mod ipc;
mod mesh;
mod platform;
mod scripting;
mod spatial;
mod texture;

#[path = "engine/mod.rs"]
mod engine;
#[path = "config_compat/mod.rs"]
mod config_compat;
mod config_base;
#[path = "config_3d/mod.rs"]
mod config_3d;
mod config_shared;

use std::collections::HashSet;
use std::sync::Arc;

use gilrs::{Button as GamepadButton, EventType as GamepadEventType, Gilrs};

use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{DeviceEvent, ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

use ipc::{EngineCommand, EngineEvent};
use platform::query_ctrl_held_os;
use rer_engine_shared::gpu::{resolve_backend, EngineGpuProfile};
use rer_engine_shared::overlay::{parse_overlay_config, OverlayConfig};
#[cfg(any(target_os = "windows", target_os = "linux"))]
use rer_engine_shared::platform::{start_position_tracker, TrackerOffset};
#[cfg(target_os = "linux")]
use rer_engine_shared::platform::setup_overlay_x11;

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
// Estructura principal de la aplicación winit
// ---------------------------------------------------------------------------
struct App {
    state:           Option<engine::State>,
    overlay:         Option<OverlayConfig>,
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
    target_fps:      u64,
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    tracker_offset:      TrackerOffset,
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    tracker_parent_id:   u64,
    cursor_captured:   bool,
}

impl App {
    fn initial_tracker_offset(overlay: &OverlayConfig) -> TrackerOffset {
        std::sync::Arc::new((
            std::sync::atomic::AtomicI32::new(overlay.rel_x),
            std::sync::atomic::AtomicI32::new(overlay.rel_y),
        ))
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn setup_overlay_tracking(&mut self, window: &Window) {
        let Some(overlay) = self.overlay.as_ref() else { return };
        if overlay.parent_id == 0 {
            return;
        }
        use raw_window_handle::HasWindowHandle;
        let Ok(handle) = window.window_handle() else { return };

        let offset = Self::initial_tracker_offset(overlay);
        self.tracker_offset = std::sync::Arc::clone(&offset);
        self.tracker_parent_id = overlay.parent_id;

        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::RawWindowHandle;
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{
                GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, GWLP_HWNDPARENT, WS_EX_NOACTIVATE,
            };
            if let RawWindowHandle::Win32(h) = handle.as_raw() {
                let motor_hwnd = HWND(h.hwnd.get() as isize);
                let electron_hwnd = HWND(overlay.parent_id as isize);
                unsafe {
                    SetWindowLongPtrW(motor_hwnd, GWLP_HWNDPARENT, electron_hwnd.0 as isize);
                    let ex = GetWindowLongPtrW(motor_hwnd, GWL_EXSTYLE);
                    SetWindowLongPtrW(motor_hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize);
                }
                start_position_tracker(motor_hwnd.0, electron_hwnd.0, offset);
            }
        }

        #[cfg(target_os = "linux")]
        {
            use raw_window_handle::RawWindowHandle;
            if let RawWindowHandle::Xlib(x) = handle.as_raw() {
                let engine_xid = x.window as u32;
                let parent_xid = overlay.parent_id as u32;
                setup_overlay_x11(engine_xid, parent_xid);
                start_position_tracker(engine_xid, parent_xid, offset);
            }
        }
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn update_tracker_offset(&mut self, x: i32, y: i32, offset_x: Option<i32>, offset_y: Option<i32>) {
        if self.tracker_parent_id == 0 {
            return;
        }
        use std::sync::atomic::Ordering;
        if let (Some(ox), Some(oy)) = (offset_x, offset_y) {
            self.tracker_offset.0.store(ox, Ordering::Relaxed);
            self.tracker_offset.1.store(oy, Ordering::Relaxed);
            return;
        }
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::Foundation::{HWND, POINT};
            use windows::Win32::Graphics::Gdi::ClientToScreen;
            unsafe {
                let mut pt = POINT { x: 0, y: 0 };
                if ClientToScreen(HWND(self.tracker_parent_id as isize), &mut pt).as_bool() {
                    self.tracker_offset.0.store(x - pt.x, Ordering::Relaxed);
                    self.tracker_offset.1.store(y - pt.y, Ordering::Relaxed);
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            unsafe {
                let display = x11::xlib::XOpenDisplay(std::ptr::null());
                if display.is_null() {
                    return;
                }
                let root = x11::xlib::XDefaultRootWindow(display);
                let mut parent_root_x: i32 = 0;
                let mut parent_root_y: i32 = 0;
                let mut child_return: x11::xlib::Window = 0;
                if x11::xlib::XTranslateCoordinates(
                    display,
                    self.tracker_parent_id as u32,
                    root,
                    0,
                    0,
                    &mut parent_root_x,
                    &mut parent_root_y,
                    &mut child_return,
                ) != 0
                {
                    self.tracker_offset.0.store(x - parent_root_x, Ordering::Relaxed);
                    self.tracker_offset.1.store(y - parent_root_y, Ordering::Relaxed);
                }
                x11::xlib::XCloseDisplay(display);
            }
        }
    }

    fn reset_preview_input_state(&mut self, state: &mut engine::State) {
        self.last_cursor = None;
        self.mouse_right = false;
        self.mouse_middle = false;
        self.left_click_pos = None;
        self.gizmo_drag_axis = None;
        self.gizmo_drag_start = None;
        self.keyboard_mouse_pressed.clear();
        self.gamepad_pressed.clear();
        state.set_active_gizmo_axis(None);
        state.set_snap_hint_visible(false);
        state.reset_play_controller_motion();
    }

    fn capture_cursor_for_preview(&mut self, state: &engine::State) {
        if !state.is_play_controller_active() {
            self.release_cursor_after_preview(state);
            return;
        }

        let window = state.window();
        let size = state.size();
        let center = PhysicalPosition::new(size.width as f64 * 0.5, size.height as f64 * 0.5);
        let _ = window.set_cursor_position(center);

        let grab_result = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|lock_err| {
                log::warn!(
                    "[preview] CursorGrabMode::Locked no disponible: {lock_err}; intentando Confined"
                );
                window.set_cursor_grab(CursorGrabMode::Confined)
            });

        match grab_result {
            Ok(()) => {
                window.set_cursor_visible(false);
                self.cursor_captured = true;
                log::info!("[preview] cursor capturado para runtime first-person");
            }
            Err(err) => {
                window.set_cursor_visible(true);
                self.cursor_captured = false;
                log::warn!("[preview] no se pudo capturar el cursor: {err}");
            }
        }
    }

    fn release_cursor_after_preview(&mut self, state: &engine::State) {
        let window = state.window();
        if let Err(err) = window.set_cursor_grab(CursorGrabMode::None) {
            log::warn!("[preview] no se pudo liberar el cursor: {err}");
        }
        window.set_cursor_visible(true);
        self.cursor_captured = false;
    }

    fn set_preview_playing(&mut self, state: &mut engine::State, playing: bool) {
        let was_playing = state.is_preview_playing();
        self.reset_preview_input_state(state);
        state.handle_command(EngineCommand::SetPreviewPlaying { playing });

        if state.is_preview_playing() {
            state.window().focus_window();
            log::info!("[preview] foco transferido a la ventana del motor");
        }

        if state.is_play_controller_active() {
            state.sync_fps_camera_mode();
            self.capture_cursor_for_preview(state);
        } else {
            self.release_cursor_after_preview(state);
            if state.has_play_character() {
                state.sync_fps_camera_mode();
            }
        }

        if was_playing != state.is_preview_playing() {
            ipc::send_event(&EngineEvent::PreviewPlayingChanged {
                playing: state.is_preview_playing(),
            });
        }
    }
}

impl ApplicationHandler<EngineCommand> for App {
    /// Llamado al iniciar (y al volver de suspensión en móvil).
    /// Aquí creamos la ventana y el estado wgpu.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() { return; }

        // Atributos base
        let mut attrs = Window::default_attributes()
            .with_title("RER-ENGINE — Viewport");

        if let Some(overlay) = &self.overlay {
            attrs = attrs
                .with_inner_size(winit::dpi::PhysicalSize::new(overlay.width, overlay.height))
                .with_position(winit::dpi::PhysicalPosition::new(overlay.x, overlay.y))
                .with_decorations(false)
                .with_resizable(false);
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

        self.setup_overlay_tracking(&window);

        match pollster::block_on(engine::State::new(Arc::clone(&window))) {
            Ok(mut state) => {
                state.setup_default_3d_scene();
                self.state = Some(state);
            }
            Err(e) => {
                log::error!("Inicialización GPU fallida: {e}");
                ipc::send_event(&EngineEvent::Error {
                    message: e.message,
                });
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, cmd: EngineCommand) {
        // El IPC thread envió un comando vía EventLoopProxy — procesar de inmediato.
        if matches!(cmd, EngineCommand::Shutdown) {
            event_loop.exit();
            return;
        }
        if let EngineCommand::SetTargetFps { fps } = &cmd {
            self.target_fps = (*fps).clamp(1, 1000);
            self.next_frame_at = std::time::Instant::now();
        }
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        if let EngineCommand::SetBounds { x, y, offset_x, offset_y, .. } = &cmd {
            self.update_tracker_offset(*x, *y, *offset_x, *offset_y);
        }
        let mut preview_toggle = None;
        match cmd {
            EngineCommand::SetPreviewPlaying { playing } => {
                preview_toggle = Some(playing);
            }
            other => {
                if let Some(state) = self.state.as_mut() {
                    state.handle_command(other);
                }
            }
        }

        if let Some(state) = self.state.as_mut() {
            state.poll_model_preloads();
        }

        if let Some(playing) = preview_toggle {
            if let Some(mut state) = self.state.take() {
                self.set_preview_playing(&mut state, playing);
                self.state = Some(state);
            }
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        if !self.cursor_captured || !state.is_play_controller_active() {
            return;
        }

        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            state.apply_fps_mouse_look(dx as f32, dy as f32);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let mut preview_toggle = None;
        let mut release_cursor_on_focus_loss = false;
        let mut recapture_cursor_on_focus_gain = false;

        {
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
                            let is_quick_build = matches!(state.active_tool, crate::config_compat::ActiveTool::QuickBuildPlace { .. });

                            if is_quick_build && pressed {
                                if let Some(cur) = self.last_cursor {
                                    let ctrl_active = self.ctrl_held || query_ctrl_held_os();
                                    state.ctrl_held = ctrl_active;
                                    state.place_quick_build_at_cursor(Some(cur));
                                }
                                self.left_click_pos = None;
                                self.gizmo_drag_axis = None;
                                state.set_active_gizmo_axis(None);
                                self.gizmo_drag_start = None;
                            } else if !pressed || !is_quick_build {
                            // Comprobar si el click es sobre un eje del gizmo.
                            // Se omite en modo pivot/quick_build para no robar el click al handler.
                            if let Some(cur) = self.last_cursor {
                                let axis = if state.pivot_edit_mode.is_none() && !is_quick_build {
                                    state.pick_gizmo_axis(cur.0, cur.1)
                                } else {
                                    None
                                };
                                self.gizmo_drag_axis = axis;
                                if axis.is_some() {
                                    state.set_active_gizmo_axis(axis);
                                    state.set_snap_hint_visible(false);
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
                            }
                        } else {
                            let is_quick_build_release = matches!(
                                state.active_tool,
                                crate::config_compat::ActiveTool::QuickBuildPlace { .. }
                            );
                            if is_quick_build_release {
                                // Colocado en press.
                            } else if self.gizmo_drag_axis.is_some() {
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
                                        // Usar query_ctrl_held_os() (Windows: GetAsyncKeyState,
                                        // Linux: XQueryKeymap) como fuente autoritativa del OS,
                                        // sin releer state.ctrl_held que podría estar obsoleto si
                                        // Electron perdió el foco y el keyup no llegó.
                                        let ctrl_active = self.ctrl_held || query_ctrl_held_os();
                                        state.ctrl_held = ctrl_active;
                                        if !state.handle_tool_click_3d(cur.0, cur.1) {
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
                if self.cursor_captured && state.is_play_controller_active() {
                    self.last_cursor = None;
                    return;
                }
                if state.is_preview_playing() {
                    self.gizmo_drag_axis = None;
                    self.gizmo_drag_start = None;
                    state.set_active_gizmo_axis(None);
                    state.set_snap_hint_visible(false);
                }
                if !state.is_preview_playing() {
                    let is_quick_build = matches!(
                        state.active_tool,
                        crate::config_compat::ActiveTool::QuickBuildPlace { .. }
                    );
                    if is_quick_build {
                        let ctrl_active = self.ctrl_held || query_ctrl_held_os();
                        state.ctrl_held = ctrl_active;
                        state.update_tool_overlay_cursor_3d(cur.0, cur.1);
                    }
                }
                if let Some((lx, ly)) = self.last_cursor {
                    let dx = cur.0 - lx;
                    let dy = cur.1 - ly;
                    if state.is_play_controller_active() {
                        state.apply_fps_mouse_look(dx, dy);
                    }
                    if !state.is_preview_playing() {
                        if let Some(axis) = self.gizmo_drag_axis {
                        // Drag de gizmo: mover entidad a lo largo del eje
                            state.drag_gizmo(cur.0, cur.1, lx, ly, axis);
                        } else if self.mouse_right {
                        if state.uses_editor_viewport_camera() {
                            state.orbit_editor_viewport(dx, dy);
                        } else {
                            state.camera.orbit(dx, dy);
                        }
                        } else if self.mouse_middle {
                        state.pan_editor_viewport(dx, dy);
                        }
                    }
                }
                // Hover: solo cuando no se está arrastrando
                if !state.is_preview_playing() && !self.mouse_right && !self.mouse_middle && self.gizmo_drag_axis.is_none() {
                    let is_quick_build = matches!(state.active_tool, crate::config_compat::ActiveTool::QuickBuildPlace { .. });
                    if !is_quick_build {
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
                if pressed && !repeat && code == KeyCode::Escape && state.is_play_controller_active() {
                    preview_toggle = Some(false);
                } else if state.is_preview_playing() {
                    if let Some(control_key) = map_keyboard_control_key(code) {
                        if pressed {
                            if self.keyboard_mouse_pressed.insert(control_key.clone()) {
                                state.handle_runtime_control_input("keyboard_mouse", &control_key);
                            }
                        } else {
                            self.keyboard_mouse_pressed.remove(&control_key);
                        }
                    }
                } else if pressed && !repeat && code == KeyCode::Space && state.is_play_controller_active() {
                    if !state.uses_scripted_play_controller() {
                        state.queue_play_controller_jump();
                    }
                }
                match code {
                    KeyCode::ControlLeft | KeyCode::ControlRight => {
                        self.ctrl_held = pressed;
                    }
                    KeyCode::KeyZ => {
                        if pressed && !repeat {
                            let ctrl_active = self.ctrl_held || query_ctrl_held_os();
                            if ctrl_active {
                                state.handle_command(EngineCommand::Undo);
                            }
                        }
                    }
                    KeyCode::KeyY => {
                        if pressed && !repeat {
                            let ctrl_active = self.ctrl_held || query_ctrl_held_os();
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
                if self.cursor_captured {
                    release_cursor_on_focus_loss = true;
                }
            }
            WindowEvent::Focused(true) => {
                if state.is_play_controller_active() {
                    recapture_cursor_on_focus_gain = true;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y)   => y,
                    MouseScrollDelta::PixelDelta(p)     => p.y as f32 * 0.05,
                };
                if state.uses_editor_viewport_camera() {
                    state.zoom_editor_viewport(scroll);
                } else {
                    state.camera.zoom(scroll);
                }
            }
            WindowEvent::RedrawRequested => {
                state.poll_model_preloads();
                state.update();
                if state.is_play_controller_active() {
                    let inputs = state.play_controller_effective_inputs(&self.keyboard_mouse_pressed);
                    state.apply_play_controller_keyboard(&inputs, state.delta_time);
                }
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

        if let Some(playing) = preview_toggle {
            if let Some(mut state) = self.state.take() {
                self.set_preview_playing(&mut state, playing);
                self.state = Some(state);
            }
        } else if release_cursor_on_focus_loss {
            if let Some(state) = self.state.take() {
                self.release_cursor_after_preview(&state);
                self.state = Some(state);
            }
        } else if recapture_cursor_on_focus_gain {
            if let Some(state) = self.state.take() {
                self.capture_cursor_for_preview(&state);
                self.state = Some(state);
            }
        }
    }
    /// Llamado cuando winit ha procesado todos los eventos pendientes del ciclo actual.
    /// Es el único lugar correcto para pedir el siguiente frame en modo Poll.
    /// Usando WaitUntil capamos al FPS objetivo y el CPU puede dormir entre frames.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let frame_duration = std::time::Duration::from_nanos(1_000_000_000 / self.target_fps.max(1));

        let now = std::time::Instant::now();
        if let Some(state) = self.state.as_mut() {
            if state.is_preview_playing() {
                if state.is_play_controller_active() {
                    state.clear_play_controller_script_frame();
                }
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
            self.next_frame_at = self.next_frame_at + frame_duration;
            // Si nos retrasamos más de un frame, resincronizar para evitar
            // ráfagas de frames de recuperación.
            if self.next_frame_at < now {
                self.next_frame_at = now + frame_duration;
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
    #[cfg(target_os = "windows")]
    const DEFAULT_LOG_FILTER: &str =
        "rer_engine_3d=warn,wgpu_core::instance=error,wgpu_hal::dx12::instance=error,wgpu_hal::auxil::dxgi::factory=error,wgpu_core=warn,wgpu_hal=warn,naga=warn";
    #[cfg(not(target_os = "windows"))]
    const DEFAULT_LOG_FILTER: &str =
        "warn,wgpu_core=warn,wgpu_hal::vulkan::conv=error,wgpu_hal::vulkan::instance=error,wgpu_hal=warn,naga=warn";

    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(DEFAULT_LOG_FILTER),
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

    let overlay = parse_overlay_config();
    let gpu = resolve_backend(EngineGpuProfile::ThreeD).label();
    if overlay.is_some() {
        log::info!("Modo overlay activado (GPU: {gpu})");
    } else {
        log::info!("Modo standalone (GPU: {gpu})");
    }

    let mut app = App {
        state:               None,
        overlay,
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
        target_fps:          60,
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        tracker_offset:      std::sync::Arc::new((
            std::sync::atomic::AtomicI32::new(0),
            std::sync::atomic::AtomicI32::new(0),
        )),
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        tracker_parent_id:   0,
        cursor_captured:     false,
    };
    event_loop.run_app(&mut app).expect("Error en el event loop");
}
