mod assets;
mod ecs;
mod engine_command;
mod entity_save_meta;
mod gizmo;
mod hud_image_asset;
mod ipc;
mod mesh;
mod platform;
mod reflections;
mod save_entity_3d;
mod screen_hud_image;
mod scripting;
mod shader_loader;
mod spatial;
mod taa;
mod texture;

#[path = "config_3d/mod.rs"]
mod config_3d;
mod config_base;
#[path = "config_compat/mod.rs"]
mod config_compat;
mod config_shared;
#[path = "engine/mod.rs"]
mod engine;

use std::collections::HashSet;
use std::sync::Arc;

use gilrs::{Button as GamepadButton, EventType as GamepadEventType, Gilrs};

use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{DeviceEvent, ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

use ipc::{EngineCommand, EngineCommandCommon, EngineEvent};
use rer_engine_shared::gpu::{EngineGpuProfile, resolve_backend};
use rer_engine_shared::overlay::{OverlayConfig, parse_overlay_config};
use rer_engine_shared::platform::TrackerOffset;
use rer_engine_shared::platform::setup_overlay_win32;
use rer_engine_shared::platform::{query_ctrl_held_os, query_shift_held_os};
use rer_engine_shared::wgpu_surface::SurfacePresentError;

use crate::config_3d::editor_viewport_controls::{self, GIZMO_CENTER_AXIS};
use crate::config_3d::plane_tool_rotate_dbg;
use crate::config_3d::transform_gizmo::TransformGizmoMode;

/// Evita que Q/E se consuman como texto en player_ui mientras la herramienta plano está activa.
fn is_plane_tool_rotate_key(key: PhysicalKey, text: Option<&str>) -> bool {
    if matches!(key, PhysicalKey::Code(KeyCode::KeyQ | KeyCode::KeyE)) {
        return true;
    }
    text.is_some_and(|t| t.eq_ignore_ascii_case("q") || t.eq_ignore_ascii_case("e"))
}

fn key_code_from_physical(key: PhysicalKey) -> Option<KeyCode> {
    match key {
        PhysicalKey::Code(code) => Some(code),
        _ => None,
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

const ENTITY_PICK_DOUBLE_CLICK_MAX_MS: u128 = 450;
const ENTITY_PICK_DOUBLE_CLICK_MAX_DIST_PX: f32 = 8.0;
const ENTITY_CLICK_DRAG_THRESHOLD_PX: f32 = 5.0;

#[derive(Clone, Copy)]
struct EntityPickClickState {
    time: std::time::Instant,
    x: f32,
    y: f32,
    entity_id: u32,
}

fn register_entity_pick_double_click(
    last: &mut Option<EntityPickClickState>,
    x: f32,
    y: f32,
    hit_entity: Option<u32>,
) -> Option<u32> {
    let now = std::time::Instant::now();
    if let (Some(prev), Some(entity_id)) = (*last, hit_entity)
        && now.duration_since(prev.time).as_millis() <= ENTITY_PICK_DOUBLE_CLICK_MAX_MS
    {
        let dist = ((x - prev.x).powi(2) + (y - prev.y).powi(2)).sqrt();
        if dist <= ENTITY_PICK_DOUBLE_CLICK_MAX_DIST_PX && entity_id == prev.entity_id {
            *last = None;
            return Some(entity_id);
        }
    }
    if let Some(entity_id) = hit_entity {
        *last = Some(EntityPickClickState {
            time: now,
            x,
            y,
            entity_id,
        });
    } else {
        *last = None;
    }
    None
}

// ---------------------------------------------------------------------------
// Arrastre de entidades en el viewport (gizmo o libre en plano de vista)
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Debug)]
enum EntityDragMode {
    GizmoTranslate(usize),
    GizmoRotate {
        axis: usize,
        pivot: glam::Vec3,
        start_mouse: (f32, f32),
        plane_u: glam::Vec3,
        plane_v: glam::Vec3,
    },
    Free {
        plane_point: glam::Vec3,
        plane_normal: glam::Vec3,
        last_world: glam::Vec3,
    },
}

#[derive(Clone)]
struct EntityGrabMode {
    plane_point: glam::Vec3,
    plane_normal: glam::Vec3,
    start_world: glam::Vec3,
    start_snapshots: Vec<engine::types::EntityTransformSnapshot>,
    constraint_axis: Option<usize>,
}

fn set_viewport_transform_constraint(entity_grab: &mut Option<EntityGrabMode>, axis: usize) {
    if let Some(session) = entity_grab.as_mut() {
        session.constraint_axis = Some(axis);
    }
}

fn collect_transform_snapshots(
    state: &engine::State,
) -> Vec<engine::types::EntityTransformSnapshot> {
    let selected_ids = state.selected_entity_ids();
    let mut snapshots = Vec::new();
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
    snapshots
}

fn clear_entity_drag_fields(
    entity_drag: &mut Option<EntityDragMode>,
    entity_drag_start: &mut Option<Vec<engine::types::EntityTransformSnapshot>>,
    state: &mut engine::State,
) {
    *entity_drag = None;
    *entity_drag_start = None;
    state.set_active_gizmo_axis(None);
    state.set_snap_hint_visible(false);
}

fn begin_entity_drag_fields(
    entity_drag: &mut Option<EntityDragMode>,
    entity_drag_start: &mut Option<Vec<engine::types::EntityTransformSnapshot>>,
    state: &mut engine::State,
    mode: EntityDragMode,
    set_snap_hint: bool,
) {
    *entity_drag = Some(mode);
    *entity_drag_start = Some(collect_transform_snapshots(state));
    state.set_snap_hint_visible(set_snap_hint);
    if let EntityDragMode::GizmoTranslate(axis) | EntityDragMode::GizmoRotate { axis, .. } = mode {
        state.set_active_gizmo_axis(Some(axis));
    } else {
        state.set_active_gizmo_axis(None);
    }
}

fn try_begin_entity_drag_at_press(
    entity_drag: &mut Option<EntityDragMode>,
    entity_drag_start: &mut Option<Vec<engine::types::EntityTransformSnapshot>>,
    left_click_pos: &mut Option<(f32, f32)>,
    state: &mut engine::State,
    cur: (f32, f32),
) {
    let axis = if state.pivot_edit_mode.is_none() {
        state.pick_gizmo_axis(cur.0, cur.1)
    } else {
        None
    };

    if let Some(axis) = axis {
        if axis == GIZMO_CENTER_AXIS && state.transform_gizmo_mode == TransformGizmoMode::Translate
        {
            if let Some(plane_point) = state.selection_center() {
                let plane_normal = state.camera.view_forward();
                if let Some(last_world) =
                    state.free_drag_world_point(cur.0, cur.1, plane_point, plane_normal)
                {
                    begin_entity_drag_fields(
                        entity_drag,
                        entity_drag_start,
                        state,
                        EntityDragMode::Free {
                            plane_point,
                            plane_normal,
                            last_world,
                        },
                        false,
                    );
                }
            }
        } else if axis <= 2 {
            match state.transform_gizmo_mode {
                TransformGizmoMode::Translate => {
                    begin_entity_drag_fields(
                        entity_drag,
                        entity_drag_start,
                        state,
                        EntityDragMode::GizmoTranslate(axis),
                        false,
                    );
                }
                TransformGizmoMode::Rotate => {
                    let Some(pivot) = state.selection_center() else {
                        return;
                    };
                    let axis_world = [glam::Vec3::X, glam::Vec3::Y, glam::Vec3::Z][axis];
                    let (plane_u, plane_v) =
                        crate::config_3d::editor_viewport_controls::rotation_plane_basis(
                            axis_world,
                            state.camera.view_forward(),
                        );
                    begin_entity_drag_fields(
                        entity_drag,
                        entity_drag_start,
                        state,
                        EntityDragMode::GizmoRotate {
                            axis,
                            pivot,
                            start_mouse: cur,
                            plane_u,
                            plane_v,
                        },
                        false,
                    );
                }
            }
        }
        *left_click_pos = None;
        return;
    }

    *left_click_pos = Some(cur);
}

fn try_promote_left_click_to_entity_drag(
    entity_drag: &mut Option<EntityDragMode>,
    entity_drag_start: &mut Option<Vec<engine::types::EntityTransformSnapshot>>,
    left_click_pos: &mut Option<(f32, f32)>,
    state: &mut engine::State,
    press: (f32, f32),
    current: (f32, f32),
) {
    if entity_drag.is_some() {
        return;
    }
    let dx = (current.0 - press.0).abs();
    let dy = (current.1 - press.1).abs();
    if dx < ENTITY_CLICK_DRAG_THRESHOLD_PX && dy < ENTITY_CLICK_DRAG_THRESHOLD_PX {
        return;
    }

    if state.selected_entity_at_pixel(press.0, press.1).is_some()
        && let Some(plane_point) = state.selection_center()
    {
        let plane_normal = state.camera.view_forward();
        if let Some(last_world) =
            state.free_drag_world_point(current.0, current.1, plane_point, plane_normal)
        {
            begin_entity_drag_fields(
                entity_drag,
                entity_drag_start,
                state,
                EntityDragMode::Free {
                    plane_point,
                    plane_normal,
                    last_world,
                },
                false,
            );
            *left_click_pos = None;
        }
    }
}

fn finish_entity_drag_fields(
    entity_drag: &mut Option<EntityDragMode>,
    entity_drag_start: &mut Option<Vec<engine::types::EntityTransformSnapshot>>,
    state: &mut engine::State,
) {
    if let Some(start_snapshots) = entity_drag_start.take() {
        let mut changed_snapshots: Vec<engine::types::EntityTransformSnapshot> = Vec::new();
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
    clear_entity_drag_fields(entity_drag, entity_drag_start, state);
    if !state.is_play_controller_active() {
        state.sync_editor_camera_focus();
    }
}

fn try_begin_entity_grab(state: &engine::State, cursor: (f32, f32)) -> Option<EntityGrabMode> {
    if state.pivot_edit_mode.is_some() || state.selected_entity_ids().is_empty() {
        return None;
    }
    let plane_point = state.selection_center()?;
    let plane_normal = state.camera.view_forward();
    let start_world = state.free_drag_world_point(cursor.0, cursor.1, plane_point, plane_normal)?;
    Some(EntityGrabMode {
        plane_point,
        plane_normal,
        start_world,
        start_snapshots: collect_transform_snapshots(state),
        constraint_axis: None,
    })
}

fn cancel_entity_grab(entity_grab: &mut Option<EntityGrabMode>, state: &mut engine::State) {
    if let Some(session) = entity_grab.take() {
        state.restore_transform_snapshots(&session.start_snapshots);
        state.set_snap_hint_visible(false);
    }
}

fn finish_entity_grab_confirm(entity_grab: &mut Option<EntityGrabMode>, state: &mut engine::State) {
    let Some(session) = entity_grab.take() else {
        return;
    };
    let mut changed_snapshots: Vec<engine::types::EntityTransformSnapshot> = Vec::new();
    for (id, pos, rot, scl) in session.start_snapshots {
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
    state.set_snap_hint_visible(false);
    if !state.is_play_controller_active() {
        state.sync_editor_camera_focus();
    }
}

// ---------------------------------------------------------------------------
// Estructura principal de la aplicación winit
// ---------------------------------------------------------------------------
struct App {
    state: Option<engine::State>,
    overlay: Option<OverlayConfig>,
    // ── Cámara orbital (Blender: MMB + modificadores)
    mouse_middle: bool,
    last_cursor: Option<(f32, f32)>,
    // Picking con click izquierdo
    left_click_pos: Option<(f32, f32)>, // posición al presionar
    /// Press izquierdo durante edición UI (para click vs drag).
    player_ui_left_press: Option<(f32, f32)>,
    // Drag de entidad / gizmo
    entity_drag: Option<EntityDragMode>,
    entity_drag_start: Option<Vec<engine::types::EntityTransformSnapshot>>,
    entity_grab: Option<EntityGrabMode>,
    last_entity_pick_click: Option<EntityPickClickState>,
    // Teclas modificadoras
    ctrl_held: bool,  // Ctrl izquierdo o derecho presionado
    shift_held: bool, // Shift izquierdo o derecho presionado
    keyboard_mouse_pressed: HashSet<String>,
    // Input de mando (gamepad)
    gilrs: Option<Gilrs>,
    gamepad_pressed: HashSet<GamepadButton>,
    // Frame rate cap: tiempo objetivo del próximo frame (evita busy loop)
    next_frame_at: std::time::Instant,
    target_fps: u64,
    tracker_offset: TrackerOffset,
    tracker_parent_id: u64,
    cursor_captured: bool,
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

    fn reset_preview_input_state(&mut self, state: &mut engine::State) {
        self.last_cursor = None;
        self.mouse_middle = false;
        self.left_click_pos = None;
        clear_entity_drag_fields(&mut self.entity_drag, &mut self.entity_drag_start, state);
        cancel_entity_grab(&mut self.entity_grab, state);
        self.keyboard_mouse_pressed.clear();
        self.gamepad_pressed.clear();
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
                log::info!("[preview] cursor capturado para play (cámara play character)");
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

    fn sync_preview_playback_side_effects(&mut self, state: &mut engine::State, was_playing: bool) {
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

    fn set_preview_playing(&mut self, state: &mut engine::State, playing: bool) {
        let was_playing = state.is_preview_playing();
        self.reset_preview_input_state(state);
        state.handle_command(EngineCommand::Common(
            EngineCommandCommon::SetPreviewPlaying { playing },
        ));
        self.sync_preview_playback_side_effects(state, was_playing);
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
                let from_save = std::env::var("RER_3D_START_FROM_SAVE")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false);
                if from_save {
                    state.setup_empty_3d();
                    ipc::send_event(&EngineEvent::Ready {
                        gravity: state.physics.gravity_magnitude(),
                    });
                } else {
                    state.setup_default_3d_scene();
                }
                state.editor_parent_id = self.tracker_parent_id;
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
        let mut preview_toggle = None;
        match cmd {
            EngineCommand::Common(EngineCommandCommon::SetPreviewPlaying { playing }) => {
                preview_toggle = Some(playing);
            }
            EngineCommand::Common(EngineCommandCommon::SetPlayerUiEditMode {
                active,
                scope,
                screen_id,
            }) => {
                if let Some(state) = self.state.as_mut() {
                    state.handle_command(EngineCommand::Common(
                        EngineCommandCommon::SetPlayerUiEditMode {
                            active,
                            scope,
                            screen_id,
                        },
                    ));
                }
            }
            other => {
                if let Some(state) = self.state.as_mut() {
                    state.handle_command(other);
                }
            }
        }

        if let Some(state) = self.state.as_mut() {
            state.poll_and_advance_model_preloads(
                crate::config_3d::static_model_cache::MODEL_GPU_PARTS_PER_FRAME,
            );
        }

        if let Some(playing) = preview_toggle
            && let Some(mut state) = self.state.take()
        {
            self.set_preview_playing(&mut state, playing);
            self.state = Some(state);
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
                WindowEvent::Ime(Ime::Commit(text)) => {
                    let _ = state.player_ui_text_ime_commit(&text);
                }
                // ── Input de ratón para cámara orbital ───────────────────────────
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
                                clear_entity_drag_fields(
                                    &mut self.entity_drag,
                                    &mut self.entity_drag_start,
                                    state,
                                );
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
                                clear_entity_drag_fields(
                                    &mut self.entity_drag,
                                    &mut self.entity_drag_start,
                                    state,
                                );
                                return;
                            }
                            if pressed {
                                if self.entity_grab.is_some() {
                                    finish_entity_grab_confirm(&mut self.entity_grab, state);
                                    self.left_click_pos = None;
                                    return;
                                }
                                // En modo quick_build_place los clicks son capturados por la herramienta;
                                // no se deben seleccionar entidades ni activar el gizmo.
                                let is_placement_tool =
                                    crate::config_compat::is_editor_placement_tool(
                                        &state.active_tool,
                                    );

                                if is_placement_tool && pressed {
                                    if let Some(cur) = self.last_cursor {
                                        let ctrl_active = self.ctrl_held || query_ctrl_held_os();
                                        state.ctrl_held = ctrl_active;
                                        match &state.active_tool {
                                            crate::config_compat::ActiveTool::QuickBuildPlace {
                                                ..
                                            } => {
                                                state.place_quick_build_at_cursor(Some(cur));
                                            }
                                            crate::config_compat::ActiveTool::PlacePlaneTool {
                                                ..
                                            } => {
                                                state.place_plane_tool_at_cursor(Some(cur));
                                            }
                                            _ => {}
                                        }
                                    }
                                    self.left_click_pos = None;
                                    clear_entity_drag_fields(
                                        &mut self.entity_drag,
                                        &mut self.entity_drag_start,
                                        state,
                                    );
                                } else if (!pressed || !is_placement_tool)
                                    && self.entity_grab.is_none()
                                    && let Some(cur) = self.last_cursor
                                {
                                    try_begin_entity_drag_at_press(
                                        &mut self.entity_drag,
                                        &mut self.entity_drag_start,
                                        &mut self.left_click_pos,
                                        state,
                                        cur,
                                    );
                                }
                            } else {
                                let is_placement_tool_release =
                                    crate::config_compat::is_editor_placement_tool(
                                        &state.active_tool,
                                    );
                                if is_placement_tool_release {
                                    // Colocado en press.
                                } else if self.entity_drag.is_some() {
                                    finish_entity_drag_fields(
                                        &mut self.entity_drag,
                                        &mut self.entity_drag_start,
                                        state,
                                    );
                                } else {
                                    // Al soltar: si no hubo arrastre, disparar picking
                                    if let (Some(start), Some(cur)) =
                                        (self.left_click_pos, self.last_cursor)
                                    {
                                        let dx = (cur.0 - start.0).abs();
                                        let dy = (cur.1 - start.1).abs();
                                        if dx < ENTITY_CLICK_DRAG_THRESHOLD_PX
                                            && dy < ENTITY_CLICK_DRAG_THRESHOLD_PX
                                        {
                                            let ctrl_active =
                                                self.ctrl_held || query_ctrl_held_os();
                                            state.ctrl_held = ctrl_active;
                                            if !state.handle_tool_click_3d(cur.0, cur.1) {
                                                let hit_entity =
                                                    state.entity_at_pixel(cur.0, cur.1);
                                                let properties_entity =
                                                    register_entity_pick_double_click(
                                                        &mut self.last_entity_pick_click,
                                                        cur.0,
                                                        cur.1,
                                                        hit_entity,
                                                    );
                                                state.pick_entity(cur.0, cur.1);
                                                if let Some(id) = properties_entity {
                                                    ipc::send_event(
                                                        &EngineEvent::EntityPropertiesOpen { id },
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                                self.left_click_pos = None;
                            }
                        }
                        MouseButton::Right
                            if !pressed
                                && !state.is_preview_playing()
                                && self.entity_grab.is_some() =>
                        {
                            cancel_entity_grab(&mut self.entity_grab, state);
                        }
                        MouseButton::Middle => {
                            self.mouse_middle = pressed;
                        }
                        _ => {}
                    }
                    if !pressed && matches!(button, MouseButton::Middle) {
                        self.last_cursor = None;
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    let cur = (position.x as f32, position.y as f32);
                    if state.player_ui_edit_active
                        && (state.player_ui_text_drag.is_some()
                            || state.player_ui_object_draw_active())
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
                    if self.cursor_captured && state.is_play_controller_active() {
                        self.last_cursor = None;
                        return;
                    }
                    if state.is_preview_playing() {
                        clear_entity_drag_fields(
                            &mut self.entity_drag,
                            &mut self.entity_drag_start,
                            state,
                        );
                    }
                    if !state.is_preview_playing() {
                        let is_placement_tool =
                            crate::config_compat::is_editor_placement_tool(&state.active_tool);
                        if is_placement_tool {
                            let ctrl_active = self.ctrl_held || query_ctrl_held_os();
                            state.ctrl_held = ctrl_active;
                            state.update_tool_overlay_cursor_3d(cur.0, cur.1);
                        }
                    }
                    if let Some((lx, ly)) = self.last_cursor {
                        let dx = cur.0 - lx;
                        let dy = cur.1 - ly;
                        if let Some(press) = self.left_click_pos {
                            try_promote_left_click_to_entity_drag(
                                &mut self.entity_drag,
                                &mut self.entity_drag_start,
                                &mut self.left_click_pos,
                                state,
                                press,
                                cur,
                            );
                        }
                        if state.is_play_controller_active() {
                            state.apply_fps_mouse_look(dx, dy);
                        }
                        if !state.is_preview_playing() {
                            let shift_active = self.shift_held || query_shift_held_os();
                            self.shift_held = shift_active;
                            state.shift_held = shift_active;
                            let ctrl_active = self.ctrl_held || query_ctrl_held_os();
                            self.ctrl_held = ctrl_active;
                            state.ctrl_held = ctrl_active;

                            if let Some(session) = self.entity_grab.as_ref() {
                                state.set_snap_hint_visible(ctrl_active);
                                state.apply_viewport_grab(
                                    session.plane_point,
                                    session.plane_normal,
                                    &session.start_snapshots,
                                    session.start_world,
                                    cur,
                                    session.constraint_axis,
                                    shift_active,
                                    ctrl_active,
                                );
                            } else {
                                match self.entity_drag {
                                    Some(EntityDragMode::GizmoTranslate(axis)) => {
                                        state.drag_gizmo(
                                            cur.0,
                                            cur.1,
                                            lx,
                                            ly,
                                            axis,
                                            shift_active,
                                            ctrl_active,
                                        );
                                    }
                                    Some(EntityDragMode::GizmoRotate {
                                        axis,
                                        pivot,
                                        start_mouse,
                                        plane_u,
                                        plane_v,
                                    }) => {
                                        if let Some(snapshots) = self.entity_drag_start.as_ref() {
                                            state.drag_gizmo_rotate(
                                                pivot,
                                                snapshots,
                                                start_mouse,
                                                cur,
                                                axis,
                                                plane_u,
                                                plane_v,
                                                shift_active,
                                                ctrl_active,
                                            );
                                        }
                                    }
                                    Some(EntityDragMode::Free {
                                        plane_point,
                                        plane_normal,
                                        ref mut last_world,
                                    }) => {
                                        state.drag_entity_free(
                                            cur.0,
                                            cur.1,
                                            plane_point,
                                            plane_normal,
                                            last_world,
                                            shift_active,
                                            ctrl_active,
                                        );
                                    }
                                    None if self.mouse_middle => {
                                        let nav =
                                        editor_viewport_controls::resolve_editor_camera_nav_mode(
                                            shift_active,
                                            ctrl_active,
                                        );
                                        state.apply_editor_camera_nav(nav, dx, dy);
                                    }
                                    None => {}
                                }
                            }
                        }
                    }
                    // Hover: solo cuando no se está arrastrando ni navegando cámara
                    if !state.is_preview_playing()
                        && !state.player_ui_edit_active
                        && !self.mouse_middle
                        && self.entity_drag.is_none()
                        && self.entity_grab.is_none()
                    {
                        let is_placement_tool =
                            crate::config_compat::is_editor_placement_tool(&state.active_tool);
                        let is_plane_preview =
                            crate::config_compat::is_plane_tool_active(&state.active_tool);
                        if !is_placement_tool && !is_plane_preview {
                            state.update_hover(cur.0, cur.1);
                        }
                    }
                    self.last_cursor = Some(cur);
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    let KeyEvent {
                        physical_key,
                        state: key_state,
                        repeat,
                        text,
                        ..
                    } = event;
                    let pressed = key_state == ElementState::Pressed;

                    let plane_tool_active = matches!(
                        state.active_tool,
                        crate::config_compat::ActiveTool::PlacePlaneTool { .. }
                    );

                    if plane_tool_active
                        && plane_tool_rotate_dbg::is_rotate_related_winit_key(
                            physical_key,
                            text.as_deref(),
                        )
                    {
                        plane_tool_rotate_dbg::log_winit_key(
                            physical_key,
                            text.as_deref(),
                            pressed,
                            repeat,
                        );
                    }

                    if plane_tool_active && is_plane_tool_rotate_key(physical_key, text.as_deref())
                    {
                        return;
                    }

                    let Some(code) = key_code_from_physical(physical_key) else {
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
                    if pressed
                        && !repeat
                        && code == KeyCode::Escape
                        && state.is_play_controller_active()
                    {
                        preview_toggle = Some(false);
                    } else if state.is_preview_playing() {
                        if let Some(control_key) = map_keyboard_control_key(code) {
                            if pressed {
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
                    } else if pressed
                        && !repeat
                        && code == KeyCode::Space
                        && state.is_play_controller_active()
                        && !state.uses_scripted_play_controller()
                    {
                        state.queue_play_controller_jump();
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
                            if self.entity_grab.is_some() && !ctrl_active {
                                set_viewport_transform_constraint(&mut self.entity_grab, 2);
                            } else if ctrl_active {
                                state.handle_command(EngineCommand::Common(
                                    EngineCommandCommon::Undo,
                                ));
                            }
                        }
                        KeyCode::KeyY if pressed && !repeat => {
                            let ctrl_active = self.ctrl_held || query_ctrl_held_os();
                            if self.entity_grab.is_some() && !ctrl_active {
                                set_viewport_transform_constraint(&mut self.entity_grab, 1);
                            } else if ctrl_active {
                                state.handle_command(EngineCommand::Common(
                                    EngineCommandCommon::Redo,
                                ));
                            }
                        }
                        KeyCode::NumpadDecimal | KeyCode::Period
                            if pressed && !repeat && !state.is_preview_playing() =>
                        {
                            state.frame_selected_in_viewport();
                        }
                        KeyCode::KeyG
                            if pressed
                                && !repeat
                                && !state.is_preview_playing()
                                && state.pivot_edit_mode.is_none()
                                && !crate::config_compat::is_editor_placement_tool(
                                    &state.active_tool,
                                ) =>
                        {
                            if self.entity_grab.is_some() {
                                finish_entity_grab_confirm(&mut self.entity_grab, state);
                            } else {
                                if let Some(cur) = self.last_cursor
                                    && let Some(session) = try_begin_entity_grab(state, cur)
                                {
                                    self.entity_grab = Some(session);
                                }
                            }
                        }
                        KeyCode::KeyR
                            if pressed
                                && !repeat
                                && !state.is_preview_playing()
                                && state.pivot_edit_mode.is_none()
                                && !crate::config_compat::is_editor_placement_tool(
                                    &state.active_tool,
                                ) =>
                        {
                            cancel_entity_grab(&mut self.entity_grab, state);
                            clear_entity_drag_fields(
                                &mut self.entity_drag,
                                &mut self.entity_drag_start,
                                state,
                            );
                            state.toggle_transform_gizmo_mode();
                        }
                        KeyCode::KeyX
                            if pressed
                                && !repeat
                                && self.entity_grab.is_some()
                                && !(self.ctrl_held || query_ctrl_held_os()) =>
                        {
                            set_viewport_transform_constraint(&mut self.entity_grab, 0);
                        }
                        KeyCode::Escape
                            if pressed
                                && !repeat
                                && !state.is_preview_playing()
                                && self.entity_grab.is_some() =>
                        {
                            cancel_entity_grab(&mut self.entity_grab, state);
                        }
                        _ => {}
                    }
                }
                WindowEvent::Focused(focused) => {
                    state.engine_window_focused = focused;
                    plane_tool_rotate_dbg::log_focus(focused);
                    if !focused {
                        self.keyboard_mouse_pressed.clear();
                        self.gamepad_pressed.clear();
                        state.clear_plane_tool_rotate_held();
                        if self.cursor_captured {
                            release_cursor_on_focus_loss = true;
                        }
                    } else if state.is_play_controller_active() {
                        recapture_cursor_on_focus_gain = true;
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    if state.is_preview_playing() {
                        return;
                    }
                    let scroll = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.05,
                    };
                    if state.uses_editor_viewport_camera() {
                        state.zoom_editor_viewport(scroll);
                    } else {
                        state.camera.zoom(scroll);
                    }
                }
                WindowEvent::RedrawRequested => {
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
                    state.poll_and_advance_model_preloads(
                        crate::config_3d::static_model_cache::MODEL_GPU_PARTS_PER_FRAME,
                    );
                    state.update();
                    if state.is_play_controller_active() {
                        let inputs =
                            state.play_controller_effective_inputs(&self.keyboard_mouse_pressed);
                        state.apply_play_controller_keyboard(&inputs, state.delta_time);
                    }
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
        } else if recapture_cursor_on_focus_gain && let Some(state) = self.state.take() {
            self.capture_cursor_for_preview(&state);
            self.state = Some(state);
        }
    }
    /// Llamado cuando winit ha procesado todos los eventos pendientes del ciclo actual.
    /// Es el único lugar correcto para pedir el siguiente frame en modo Poll.
    /// Usando WaitUntil capamos al FPS objetivo y el CPU puede dormir entre frames.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let frame_duration =
            std::time::Duration::from_nanos(1_000_000_000 / self.target_fps.max(1));

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
    const DEFAULT_LOG_FILTER: &str = "rer_engine_3d=warn,rer_engine_3d::config_base=info,rer_engine_3d::config_3d::skin_diag=info,\
rer_engine_3d::config_3d::model_asset=info,\
rer_engine_3d::config_3d::reflection_settings=info,\
rer_engine_3d::engine::commands=info,\
rer_engine_3d::reflections=info,\
rer_engine_3d::engine::render=info,\
wgpu_core::instance=error,wgpu_hal::vulkan::conv=error,\
wgpu_hal::vulkan::instance=error,wgpu_core=warn,wgpu_hal=warn,naga=warn";

    rer_engine_shared::logging::init(DEFAULT_LOG_FILTER);

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
        state: None,
        overlay,
        mouse_middle: false,
        last_cursor: None,
        left_click_pos: None,
        player_ui_left_press: None,
        entity_drag: None,
        entity_drag_start: None,
        entity_grab: None,
        last_entity_pick_click: None,
        ctrl_held: false,
        shift_held: false,
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
        cursor_captured: false,
    };
    event_loop
        .run_app(&mut app)
        .expect("Error en el event loop");
}
