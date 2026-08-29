mod bundled_demo;
mod ecs;
mod engine;
mod engine_command;
mod entity_save_meta;
mod gizmo;
mod hud_image_asset;
mod ipc;
mod mesh;
mod on_keep;
mod on_press;
mod scene_target;
mod screen_hud_image;
mod scripting;
mod spatial;
mod texture;

#[path = "config_2d/mod.rs"]
mod config_2d;
mod config_base;
mod config_compat;
mod config_shared;

use std::collections::HashSet;
use std::sync::Arc;

use gilrs::{Button as GamepadButton, EventType as GamepadEventType, Gilrs};

use winit::{
    application::ApplicationHandler,
    event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use ipc::{EngineCommand, EngineCommandCommon, EngineEvent};
use rer_engine_shared::gpu::{EngineGpuProfile, resolve_backend};
use rer_engine_shared::overlay::{OverlayConfig, parse_overlay_config};
use rer_engine_shared::platform::TrackerOffset;
use rer_engine_shared::platform::query_shift_held_os;
use rer_engine_shared::platform::setup_overlay_win32;
use rer_engine_shared::wgpu_surface::SurfacePresentError;

// ---------------------------------------------------------------------------
// Consulta de Ctrl en el OS (sin depender del foco de ventana del motor)
// ---------------------------------------------------------------------------

/// Usamos GetAsyncKeyState para consultar el estado real del Ctrl
/// sin depender del foco de ventana. Esto evita el bug de "toggle" que ocurre
/// cuando Electron pierde el foco al hacer click en el viewport del motor y
/// el keyup de Control nunca llega al renderer.
fn query_ctrl_held_os() -> bool {
    // SAFETY: GetAsyncKeyState es seguro de llamar en cualquier contexto Win32.
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_LCONTROL, VK_RCONTROL,
        };
        let left = (GetAsyncKeyState(VK_LCONTROL.0 as i32) as u16 & 0x8000) != 0;
        let right = (GetAsyncKeyState(VK_RCONTROL.0 as i32) as u16 & 0x8000) != 0;
        left || right
    }
}

/// Convierte un `KeyCode` de winit a la string de control usada en los bindings.
///
/// Usa el nombre del variant (via Debug) para mapear automáticamente cualquier tecla
/// de letra (KeyA-KeyZ → "A"-"Z") y dígito (Digit0-Digit9 → "0"-"9") sin necesitar
/// actualizar este archivo al agregar nuevas teclas en el frontend.
fn map_keyboard_control_key(code: KeyCode) -> Option<String> {
    let debug = format!("{code:?}");

    // Letras: "KeyA" → "A", "KeyZ" → "Z"
    if let Some(letter) = debug.strip_prefix("Key")
        && letter.len() == 1
        && letter.as_bytes()[0].is_ascii_alphabetic()
    {
        return Some(letter.to_uppercase());
    }

    // Dígitos: "Digit0" → "0", "Digit9" → "9"
    if let Some(digit) = debug.strip_prefix("Digit")
        && digit.len() == 1
        && digit.as_bytes()[0].is_ascii_digit()
    {
        return Some(digit.to_string());
    }

    // Teclas especiales con nombre distinto al variant
    match code {
        KeyCode::Space => Some("SPACE".to_string()),
        KeyCode::ControlLeft | KeyCode::ControlRight => Some("CTRL".to_string()),
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Some("SHIFT".to_string()),
        KeyCode::AltLeft | KeyCode::AltRight => Some("ALT".to_string()),
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
    state: Option<engine::State>,
    overlay: Option<OverlayConfig>,
    // ── Navegación del editor
    mouse_right: bool,  // botón derecho  → pan
    mouse_middle: bool, // botón central  → pan
    last_cursor: Option<(f32, f32)>,
    // Picking con click izquierdo
    left_click_pos: Option<(f32, f32)>, // posición al presionar
    // Drag de gizmo
    gizmo_drag_axis: Option<usize>, // eje activo (0=X,1=Y)
    gizmo_drag_start: Option<Vec<engine::types::EntityTransformSnapshot>>,
    // Teclas modificadoras
    ctrl_held: bool, // Ctrl izquierdo o derecho presionado
    shift_held: bool,
    player_ui_left_press: Option<(f32, f32)>,
    keyboard_mouse_pressed: HashSet<String>,
    // Input de mando (gamepad)
    gilrs: Option<Gilrs>,
    gamepad_pressed: HashSet<GamepadButton>,
    // Frame rate cap: tiempo objetivo del próximo frame (evita busy loop)
    next_frame_at: std::time::Instant,
    target_fps: u64,
    tracker_offset: TrackerOffset,
    tracker_parent_id: u64,
}

impl App {
    fn initial_tracker_offset(overlay: &OverlayConfig) -> TrackerOffset {
        std::sync::Arc::new((
            std::sync::atomic::AtomicI32::new(overlay.rel_x),
            std::sync::atomic::AtomicI32::new(overlay.rel_y),
        ))
    }

    fn setup_overlay_tracking(&mut self, window: &Window) {
        let Some(overlay) = self.overlay.as_ref() else {
            return;
        };
        if overlay.parent_id == 0 {
            return;
        }
        use raw_window_handle::HasWindowHandle;
        let Ok(handle) = window.window_handle() else {
            return;
        };

        let offset = Self::initial_tracker_offset(overlay);
        self.tracker_offset = std::sync::Arc::clone(&offset);
        self.tracker_parent_id = overlay.parent_id;

        use raw_window_handle::RawWindowHandle;
        if let RawWindowHandle::Win32(h) = handle.as_raw() {
            setup_overlay_win32(h.hwnd.get(), overlay.parent_id as isize, offset);
        }
    }

    fn update_tracker_offset(
        &mut self,
        x: i32,
        y: i32,
        offset_x: Option<i32>,
        offset_y: Option<i32>,
    ) {
        if self.tracker_parent_id == 0 {
            return;
        }
        use std::sync::atomic::Ordering;
        if let (Some(ox), Some(oy)) = (offset_x, offset_y) {
            self.tracker_offset.0.store(ox, Ordering::Relaxed);
            self.tracker_offset.1.store(oy, Ordering::Relaxed);
            return;
        }
        use windows::Win32::Foundation::{HWND, POINT};
        use windows::Win32::Graphics::Gdi::ClientToScreen;
        unsafe {
            let mut pt = POINT { x: 0, y: 0 };
            if ClientToScreen(HWND(self.tracker_parent_id as *mut _), &mut pt).as_bool() {
                self.tracker_offset.0.store(x - pt.x, Ordering::Relaxed);
                self.tracker_offset.1.store(y - pt.y, Ordering::Relaxed);
            }
        }
    }
}

impl ApplicationHandler<EngineCommand> for App {
    /// Llamado al iniciar (y al volver de suspensión en móvil).
    /// Aquí creamos la ventana y el estado wgpu.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        // Atributos base
        let mut attrs = Window::default_attributes().with_title("RER-ENGINE — Viewport");

        if let Some(overlay) = &self.overlay {
            // ── Modo overlay: ventana separada alineada al hueco del editor ───
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
            Ok(state) => {
                ipc::send_event(&EngineEvent::Ready {
                    gravity: state.physics_2d.gravity_magnitude(),
                });
                self.state = Some(state);
            }
            Err(e) => {
                log::error!("Inicialización GPU fallida: {e}");
                ipc::send_event(&EngineEvent::Error { message: e.message });
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, cmd: EngineCommand) {
        // El IPC thread envió un comando vía EventLoopProxy — procesar de inmediato.
        if matches!(cmd, EngineCommand::Common(EngineCommandCommon::Shutdown)) {
            event_loop.exit();
            return;
        }
        if let EngineCommand::Common(EngineCommandCommon::SetTargetFps { fps }) = &cmd {
            self.target_fps = (*fps).clamp(1, 1000);
            self.next_frame_at = std::time::Instant::now();
        }
        if let EngineCommand::Common(EngineCommandCommon::SetBounds {
            x,
            y,
            offset_x,
            offset_y,
            ..
        }) = &cmd
        {
            self.update_tracker_offset(*x, *y, *offset_x, *offset_y);
        }
        // When entering game mode, automatically focus the engine window
        // so keyboard/mouse input is captured without requiring a manual click.
        let entering_play = matches!(
            cmd,
            EngineCommand::Common(EngineCommandCommon::SetPreviewPlaying { playing: true })
        );

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
            WindowEvent::Ime(Ime::Commit(text)) => {
                let _ = state.player_ui_text_ime_commit(&text);
            }
            // ── Input de ratón para editor 2D ─────────────────────────────────
            WindowEvent::MouseInput {
                button,
                state: btn_state,
                ..
            } => {
                let pressed = btn_state == ElementState::Pressed;
                if state.is_preview_playing()
                    && let Some(control_key) = map_mouse_control_key(button)
                {
                    if pressed {
                        if let Some(cur) = self.last_cursor {
                            state.set_play_mouse_px(cur.0, cur.1);
                        }
                        self.keyboard_mouse_pressed.insert(control_key.to_string());
                        state.dispatch_on_press("keyboard_mouse", control_key);
                        state.dispatch_on_keep_key_down(control_key);
                    } else {
                        self.keyboard_mouse_pressed.remove(control_key);
                        state.dispatch_on_keep_key_up("keyboard_mouse", control_key);
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
                        if state.player_ui_edit_active {
                            if let Some(cur) = self.last_cursor {
                                if pressed {
                                    state.player_ui_mouse_down(cur.0, cur.1);
                                    self.player_ui_left_press = Some(cur);
                                } else if let Some(start) = self.player_ui_left_press.take() {
                                    state.player_ui_mouse_up(cur.0, cur.1, start.0, start.1);
                                }
                            }
                            self.left_click_pos = None;
                            self.gizmo_drag_axis = None;
                            self.gizmo_drag_start = None;
                            state.set_active_gizmo_axis(None);
                            state.set_snap_hint_visible(false);
                            return;
                        }
                        if pressed {
                            // En modo quick_build_place los clicks son capturados por la herramienta;
                            // no se deben seleccionar entidades ni activar el gizmo.
                            let is_quick_build = matches!(
                                state.active_tool,
                                crate::config_2d::ActiveTool::QuickBuildPlace { .. }
                            );

                            // Comprobar si el click es sobre un eje del gizmo.
                            // Se omite en modo pivot/quick_build para no robar el click al handler.
                            if let Some(cur) = self.last_cursor {
                                let axis = if state.pivot_edit_mode.is_none() && !is_quick_build {
                                    state.pick_gizmo_axis_2d(cur.0, cur.1)
                                } else {
                                    None
                                };
                                self.gizmo_drag_axis = axis;
                                if axis.is_some() {
                                    state.set_active_gizmo_axis(axis);
                                    state.set_snap_hint_visible(state.camera_2d.is_some());
                                    let selected_ids: Vec<u32> =
                                        if !state.selected_entities.is_empty() {
                                            state.selected_entities.clone()
                                        } else {
                                            state.selected_entity.into_iter().collect()
                                        };
                                    let mut snapshots: Vec<engine::types::EntityTransformSnapshot> =
                                        Vec::new();
                                    for sel_id in selected_ids {
                                        if let Some(t) =
                                            state.world.get::<crate::ecs::Transform>(sel_id)
                                        {
                                            snapshots.push((
                                                sel_id,
                                                t.position.to_array(),
                                                [
                                                    t.rotation.x,
                                                    t.rotation.y,
                                                    t.rotation.z,
                                                    t.rotation.w,
                                                ],
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
                                    let mut changed_snapshots: Vec<
                                        engine::types::EntityTransformSnapshot,
                                    > = Vec::new();
                                    for (id, pos, rot, scl) in start_snapshots {
                                        if let Some(t) =
                                            state.world.get::<crate::ecs::Transform>(id)
                                        {
                                            let changed = t.position.to_array() != pos
                                                || [
                                                    t.rotation.x,
                                                    t.rotation.y,
                                                    t.rotation.z,
                                                    t.rotation.w,
                                                ] != rot
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
                                if let (Some(start), Some(cur)) =
                                    (self.left_click_pos, self.last_cursor)
                                {
                                    let dx = (cur.0 - start.0).abs();
                                    let dy = (cur.1 - start.1).abs();
                                    if dx < 5.0 && dy < 5.0 {
                                        // Consultar el estado real del Ctrl al momento del click.
                                        // Usar query_ctrl_held_os() (GetAsyncKeyState) como fuente
                                        // autoritativa del OS, sin releer state.ctrl_held que podría
                                        // estar obsoleto si Electron perdió el foco y el keyup no llegó.
                                        let ctrl_active = self.ctrl_held || query_ctrl_held_os();
                                        state.ctrl_held = ctrl_active;
                                        if state.pivot_edit_mode.is_some() {
                                            state.handle_pivot_click_2d(cur.0, cur.1);
                                        } else if !state.handle_tool_click_2d(cur.0, cur.1) {
                                            state.pick_entity_2d(cur.0, cur.1);
                                        }
                                    }
                                }
                            }
                            self.left_click_pos = None;
                        }
                    }
                    MouseButton::Right => {
                        self.mouse_right = pressed;
                        // Fin de pan: notificar posición actual de la cámara 2D
                        if !pressed && let Some(cam2d) = &state.camera_2d {
                            ipc::send_event(&EngineEvent::Camera2dUpdated {
                                x: cam2d.x,
                                y: cam2d.y,
                                half_h: cam2d.half_h,
                            });
                        }
                    }
                    MouseButton::Middle => {
                        self.mouse_middle = pressed;
                    }
                    _ => {}
                }
                if !pressed && matches!(button, MouseButton::Right | MouseButton::Middle) {
                    self.last_cursor = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let cur = (position.x as f32, position.y as f32);
                if state.player_ui_edit_active
                    && (state.player_ui_text_drag.is_some() || state.player_ui_object_draw_active())
                {
                    let shift_active = self.shift_held || query_shift_held_os();
                    self.shift_held = shift_active;
                    state.shift_held = shift_active;
                    let ctrl_active = self.ctrl_held || query_ctrl_held_os();
                    self.ctrl_held = ctrl_active;
                    state.ctrl_held = ctrl_active;
                    state.player_ui_mouse_move(cur.0, cur.1);
                    self.last_cursor = Some(cur);
                    return;
                }
                if state.is_preview_playing() {
                    if let Some(cur) = self.last_cursor {
                        state.set_play_mouse_px(cur.0, cur.1);
                    }
                    self.gizmo_drag_axis = None;
                    self.gizmo_drag_start = None;
                    state.set_active_gizmo_axis(None);
                    state.set_snap_hint_visible(false);
                }
                if state.camera_2d.is_some() && !state.is_preview_playing() {
                    // Usar OS query directa: evita que state.ctrl_held obsoleto se propague.
                    let ctrl_active = self.ctrl_held || query_ctrl_held_os();
                    state.ctrl_held = ctrl_active;
                    state.update_tool_overlay_cursor_2d(cur.0, cur.1);
                }
                if let Some((lx, ly)) = self.last_cursor {
                    let dx = cur.0 - lx;
                    let dy = cur.1 - ly;
                    if !state.is_preview_playing() {
                        if let Some(axis) = self.gizmo_drag_axis {
                            // Drag de gizmo: mover entidad en el plano 2D.
                            // OS query directa para snap: independiente del foco de ventana.
                            let snap = self.ctrl_held || query_ctrl_held_os();
                            state.drag_gizmo_2d(cur.0, cur.1, lx, ly, axis, snap);
                        } else if self.mouse_right {
                            let (vw, vh) = {
                                let s = state.size();
                                (s.width as f32, s.height as f32)
                            };
                            if let Some(cam2d) = &mut state.camera_2d {
                                cam2d.pan(dx, dy, vw, vh);
                            }
                        } else if self.mouse_middle {
                            state.camera.pan(dx, dy);
                        }
                    }
                }
                // Hover: solo cuando no se está arrastrando
                if !state.is_preview_playing()
                    && !state.player_ui_edit_active
                    && !self.mouse_right
                    && !self.mouse_middle
                    && self.gizmo_drag_axis.is_none()
                {
                    let is_quick_build = matches!(
                        state.active_tool,
                        crate::config_2d::ActiveTool::QuickBuildPlace { .. }
                    );
                    if !is_quick_build {
                        state.update_hover_2d(cur.0, cur.1);
                    }
                }
                self.last_cursor = Some(cur);
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key,
                        state: key_state,
                        repeat,
                        text,
                        ..
                    },
                ..
            } => {
                let pressed = key_state == ElementState::Pressed;
                let PhysicalKey::Code(code) = physical_key else {
                    return;
                };
                if state.player_ui_edit_key_input(code, pressed, repeat) {
                    return;
                }
                if state.player_ui_text_key_input(
                    code,
                    pressed,
                    repeat,
                    text.as_ref().map(|s| s.as_str()),
                ) {
                    return;
                }
                if state.is_preview_playing()
                    && let Some(control_key) = map_keyboard_control_key(code)
                {
                    if pressed {
                        // Insertar siempre para que on_keep_frame la procese cada frame.
                        // on_press y key_down solo se disparan en el borde (no repeat).
                        self.keyboard_mouse_pressed.insert(control_key.clone());
                        if !repeat {
                            state.dispatch_on_press("keyboard_mouse", &control_key);
                            state.dispatch_on_keep_key_down(&control_key);
                        }
                    } else {
                        self.keyboard_mouse_pressed.remove(&control_key);
                        state.dispatch_on_keep_key_up("keyboard_mouse", &control_key);
                    }
                }
                match code {
                    KeyCode::ControlLeft | KeyCode::ControlRight => {
                        self.ctrl_held = pressed;
                        state.ctrl_held = pressed;
                    }
                    KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                        self.shift_held = pressed;
                        state.shift_held = pressed;
                        if !pressed && state.player_ui_edit_active {
                            state.player_ui_on_shift_released();
                        }
                    }
                    KeyCode::KeyZ if pressed && !repeat => {
                        let ctrl_active = self.ctrl_held || query_ctrl_held_os();
                        if ctrl_active {
                            state.handle_command(EngineCommand::Common(EngineCommandCommon::Undo));
                        }
                    }
                    KeyCode::KeyY if pressed && !repeat => {
                        let ctrl_active = self.ctrl_held || query_ctrl_held_os();
                        if ctrl_active {
                            state.handle_command(EngineCommand::Common(EngineCommandCommon::Redo));
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::Focused(false) => {
                self.keyboard_mouse_pressed.clear();
                self.gamepad_pressed.clear();
                state.clear_all_on_keep_horizontal_blocks();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.05,
                };
                if let Some(cam2d) = &mut state.camera_2d {
                    // Zoom ortográfico: reducir/aumentar half_h
                    cam2d.half_h = (cam2d.half_h - scroll * 0.5).clamp(1.0, 50.0);
                    ipc::send_event(&EngineEvent::Camera2dUpdated {
                        x: cam2d.x,
                        y: cam2d.y,
                        half_h: cam2d.half_h,
                    });
                } else {
                    state.camera.zoom(scroll);
                }
            }
            WindowEvent::RedrawRequested => {
                // on_keep: ejecutar cada frame mientras la tecla/botón esté sostenido.
                // on_press ya se ejecutó en el evento KeyboardInput/MouseInput (una sola vez).
                if state.is_preview_playing() {
                    for control_key in &self.keyboard_mouse_pressed {
                        state.dispatch_on_keep_frame("keyboard_mouse", control_key);
                    }
                    for button in &self.gamepad_pressed {
                        if let Some(control_key) = map_gamepad_control_key(*button) {
                            state.dispatch_on_keep_frame("gamepad", control_key);
                        }
                    }
                }
                state.update();
                match state.render() {
                    Ok(_) => {}
                    Err(SurfacePresentError::Reconfigure) => {
                        let size = state.size();
                        state.resize(size);
                    }
                    Err(SurfacePresentError::SkipFrame) => {}
                    Err(SurfacePresentError::Validation) => {
                        log::warn!("render validation error");
                    }
                }
                // NO llamar request_redraw() aquí: lo hace about_to_wait con WaitUntil.
                // Hacerlo aquí + ControlFlow::Poll crea un busy loop que consume CPU al 100%.
            }
            _ => {}
        }
    }
    /// Llamado cuando winit ha procesado todos los eventos pendientes del ciclo actual.
    /// Es el único lugar correcto para pedir el siguiente frame en modo Poll.
    /// Usando WaitUntil capamos al FPS objetivo y el CPU puede dormir entre frames.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // El cap real vive en `State::target_fps` (también lo actualiza `import_scene`).
        let fps_limit = self
            .state
            .as_ref()
            .map(|s| s.target_fps)
            .unwrap_or(self.target_fps)
            .max(1);
        let frame_duration = std::time::Duration::from_nanos(1_000_000_000 / fps_limit);

        let now = std::time::Instant::now();
        if let Some(state) = self.state.as_mut() {
            if state.is_preview_playing() {
                if let Some(gilrs) = self.gilrs.as_mut() {
                    while let Some(evt) = gilrs.next_event() {
                        match evt.event {
                            GamepadEventType::ButtonPressed(button, _) => {
                                self.gamepad_pressed.insert(button);
                                if let Some(control_key) = map_gamepad_control_key(button) {
                                    state.dispatch_on_press("gamepad", control_key);
                                    state.dispatch_on_keep_key_down(control_key);
                                }
                            }
                            GamepadEventType::ButtonReleased(button, _) => {
                                self.gamepad_pressed.remove(&button);
                                if let Some(control_key) = map_gamepad_control_key(button) {
                                    state.dispatch_on_keep_key_up("gamepad", control_key);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            } else {
                self.keyboard_mouse_pressed.clear();
                self.gamepad_pressed.clear();
                state.clear_all_on_keep_horizontal_blocks();
            }
        }

        if now >= self.next_frame_at {
            if let Some(state) = &self.state {
                state.window().request_redraw();
            }
            // Calcular el próximo tick desde el tiempo objetivo, no desde `now`,
            // para evitar drift acumulado si un frame tardó más de lo esperado.
            self.next_frame_at += frame_duration;
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
    rer_engine_shared::logging::init(
        // `rer_engine_2d=info` permite ver logs de colisión/física sin RUST_LOG.
        "rer_engine_2d=info,warn,wgpu_core=warn,wgpu_hal::vulkan::conv=error,wgpu_hal::vulkan::instance=error,wgpu_hal=warn,naga=warn",
    );

    // Canal IPC: hilo stdin → event loop vía EventLoopProxy (despierta el loop inmediatamente)
    let event_loop = EventLoop::<EngineCommand>::with_user_event()
        .build()
        .expect("No se pudo crear EventLoop");
    let proxy = event_loop.create_proxy();
    ipc::start_ipc_thread(proxy);

    // ControlFlow se gestiona dinámicamente en about_to_wait con WaitUntil(next_frame).
    // NO usar Poll aquí: Poll + request_redraw en RedrawRequested = busy loop al 100% CPU.

    let overlay = parse_overlay_config();
    let gpu = resolve_backend(EngineGpuProfile::TwoD).label();
    if overlay.is_some() {
        log::info!("Modo overlay activado (GPU: {gpu})");
    } else {
        log::info!("Modo standalone (GPU: {gpu})");
    }

    let mut app = App {
        state: None,
        overlay,
        mouse_right: false,
        mouse_middle: false,
        last_cursor: None,
        left_click_pos: None,
        gizmo_drag_axis: None,
        gizmo_drag_start: None,
        ctrl_held: false,
        shift_held: false,
        player_ui_left_press: None,
        keyboard_mouse_pressed: HashSet::new(),
        gilrs: Gilrs::new().ok(),
        gamepad_pressed: HashSet::new(),
        next_frame_at: std::time::Instant::now(),
        target_fps: 60,
        tracker_offset: std::sync::Arc::new((
            std::sync::atomic::AtomicI32::new(0),
            std::sync::atomic::AtomicI32::new(0),
        )),
        tracker_parent_id: 0,
    };
    event_loop
        .run_app(&mut app)
        .expect("Error en el event loop");
}
