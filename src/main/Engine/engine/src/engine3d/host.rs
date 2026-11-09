//! Host winit del motor 3D: event loop, input, overlay y arranque del estado.

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

use crate::engine3d::ipc::{EngineCommand, EngineCommandCommon, EngineEvent};
use rer_engine_core::backend_policy::{EngineGpuProfile, resolve_backend};
use rer_engine_core::platform::{query_ctrl_held_os, query_shift_held_os};
use rer_engine_core::surface::SurfacePresentError;

use crate::engine3d::config_3d::editor_viewport_controls::{self, GIZMO_CENTER_AXIS};
use crate::engine3d::config_3d::plane_tool_rotate_dbg;
use crate::engine3d::config_3d::transform_gizmo::TransformGizmoMode;

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
    start_snapshots: Vec<crate::engine3d::engine::types::EntityTransformSnapshot>,
    constraint_axis: Option<usize>,
}

fn set_viewport_transform_constraint(entity_grab: &mut Option<EntityGrabMode>, axis: usize) {
    if let Some(session) = entity_grab.as_mut() {
        session.constraint_axis = Some(axis);
    }
}

fn collect_transform_snapshots(
    state: &crate::engine3d::engine::State,
) -> Vec<crate::engine3d::engine::types::EntityTransformSnapshot> {
    let selected_ids = state.selected_entity_ids();
    let mut snapshots = Vec::new();
    for sel_id in selected_ids {
        if let Some(t) = state.world.get::<crate::engine3d::ecs::Transform>(sel_id) {
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
    entity_drag_start: &mut Option<Vec<crate::engine3d::engine::types::EntityTransformSnapshot>>,
    state: &mut crate::engine3d::engine::State,
) {
    *entity_drag = None;
    *entity_drag_start = None;
    state.set_active_gizmo_axis(None);
    state.set_snap_hint_visible(false);
}

fn begin_entity_drag_fields(
    entity_drag: &mut Option<EntityDragMode>,
    entity_drag_start: &mut Option<Vec<crate::engine3d::engine::types::EntityTransformSnapshot>>,
    state: &mut crate::engine3d::engine::State,
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
    entity_drag_start: &mut Option<Vec<crate::engine3d::engine::types::EntityTransformSnapshot>>,
    left_click_pos: &mut Option<(f32, f32)>,
    state: &mut crate::engine3d::engine::State,
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
                        crate::engine3d::config_3d::editor_viewport_controls::rotation_plane_basis(
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
    entity_drag_start: &mut Option<Vec<crate::engine3d::engine::types::EntityTransformSnapshot>>,
    left_click_pos: &mut Option<(f32, f32)>,
    state: &mut crate::engine3d::engine::State,
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
    entity_drag_start: &mut Option<Vec<crate::engine3d::engine::types::EntityTransformSnapshot>>,
    state: &mut crate::engine3d::engine::State,
) {
    if let Some(start_snapshots) = entity_drag_start.take() {
        let mut changed_snapshots: Vec<crate::engine3d::engine::types::EntityTransformSnapshot> =
            Vec::new();
        for (id, pos, rot, scl) in start_snapshots {
            if let Some(t) = state.world.get::<crate::engine3d::ecs::Transform>(id) {
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

fn try_begin_entity_grab(
    state: &crate::engine3d::engine::State,
    cursor: (f32, f32),
) -> Option<EntityGrabMode> {
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

fn cancel_entity_grab(
    entity_grab: &mut Option<EntityGrabMode>,
    state: &mut crate::engine3d::engine::State,
) {
    if let Some(session) = entity_grab.take() {
        state.restore_transform_snapshots(&session.start_snapshots);
        state.set_snap_hint_visible(false);
    }
}

fn finish_entity_grab_confirm(
    entity_grab: &mut Option<EntityGrabMode>,
    state: &mut crate::engine3d::engine::State,
) {
    let Some(session) = entity_grab.take() else {
        return;
    };
    let mut changed_snapshots: Vec<crate::engine3d::engine::types::EntityTransformSnapshot> =
        Vec::new();
    for (id, pos, rot, scl) in session.start_snapshots {
        if let Some(t) = state.world.get::<crate::engine3d::ecs::Transform>(id) {
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
pub struct App {
    state: Option<crate::engine3d::engine::State>,
    // ── Cámara orbital (Blender: MMB + modificadores)
    mouse_middle: bool,
    /// ── Cámara libre del editor: RMB mantiene pulsado el "mirar alrededor".
    mouse_right: bool,
    /// Teclas actualmente presionadas para el vuelo del viewport (WASD / Space / C / Shift).
    editor_keys: HashSet<KeyCode>,
    last_cursor: Option<(f32, f32)>,
    // Picking con click izquierdo
    left_click_pos: Option<(f32, f32)>, // posición al presionar
    /// Press izquierdo durante edición UI (para click vs drag).
    player_ui_left_press: Option<(f32, f32)>,
    // Drag de entidad / gizmo
    entity_drag: Option<EntityDragMode>,
    entity_drag_start: Option<Vec<crate::engine3d::engine::types::EntityTransformSnapshot>>,
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
    cursor_captured: bool,
}

impl App {
    /// Host del editor: el `State` ya existe y comparte el `SharedGpu` de la ventana.
    #[cfg_attr(not(feature = "editor"), allow(dead_code))]
    pub(crate) fn new_in_editor(state: crate::engine3d::engine::State) -> Self {
        Self {
            state: Some(state),
            mouse_middle: false,
            mouse_right: false,
            editor_keys: HashSet::new(),
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
            cursor_captured: false,
        }
    }

    /// Graba la escena en el `FrameTarget` del editor y fija el tamaño del viewport.
    pub fn render_scene(
        &mut self,
        frame: &crate::engine3d::vulkan::FrameTarget,
    ) -> Result<(), rer_engine_core::surface::SurfacePresentError> {
        let Some(state) = self.state.as_mut() else {
            return Ok(());
        };
        state.set_viewport_size(winit::dpi::PhysicalSize::new(
            frame.extent.width,
            frame.extent.height,
        ));
        state.render_into(frame)
    }

    pub fn is_preview_playing(&self) -> bool {
        self.state
            .as_ref()
            .is_some_and(|state| state.is_preview_playing())
    }

    /// Movimiento relativo del ratón (DeviceEvent::MouseMotion) durante play.
    pub fn handle_device_motion(&mut self, delta_x: f32, delta_y: f32) {
        if !self.cursor_captured {
            return;
        }
        if let Some(state) = self.state.as_mut()
            && state.is_play_controller_active()
        {
            state.apply_fps_mouse_look(delta_x, delta_y);
        }
    }

    /// Avanza simulación e input de play sin renderizar (el render lo decide el host).
    pub fn frame_update(&mut self) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
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
            crate::engine3d::config_3d::static_model_cache::MODEL_GPU_PARTS_PER_FRAME,
        );
        state.update();
        if state.is_play_controller_active() {
            let inputs = state.play_controller_effective_inputs(&self.keyboard_mouse_pressed);
            state.apply_play_controller_keyboard(&inputs, state.delta_time);
        } else if state.editor_fly_active() {
            // WASD/Space/C para recorrer el mapa en edición (estilo Minecraft).
            state.apply_editor_fly_input(&self.editor_keys, state.delta_time);
        }
    }

    /// Render y present del host standalone.
    pub fn render_standalone(&mut self) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
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
    }
}

impl App {
    fn reset_preview_input_state(&mut self, state: &mut crate::engine3d::engine::State) {
        self.last_cursor = None;
        self.mouse_middle = false;
        self.mouse_right = false;
        self.editor_keys.clear();
        self.left_click_pos = None;
        clear_entity_drag_fields(&mut self.entity_drag, &mut self.entity_drag_start, state);
        cancel_entity_grab(&mut self.entity_grab, state);
        self.keyboard_mouse_pressed.clear();
        self.gamepad_pressed.clear();
        state.reset_play_controller_motion();
    }

    fn capture_cursor_for_preview(&mut self, state: &crate::engine3d::engine::State) {
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

    fn release_cursor_after_preview(&mut self, state: &crate::engine3d::engine::State) {
        let window = state.window();
        if let Err(err) = window.set_cursor_grab(CursorGrabMode::None) {
            log::warn!("[preview] no se pudo liberar el cursor: {err}");
        }
        window.set_cursor_visible(true);
        self.cursor_captured = false;
    }

    fn sync_preview_playback_side_effects(
        &mut self,
        state: &mut crate::engine3d::engine::State,
        was_playing: bool,
    ) {
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
            crate::engine3d::ipc::send_event(&EngineEvent::PreviewPlayingChanged {
                playing: state.is_preview_playing(),
            });
        }
    }

    fn set_preview_playing(&mut self, state: &mut crate::engine3d::engine::State, playing: bool) {
        let was_playing = state.is_preview_playing();
        self.reset_preview_input_state(state);
        state.handle_command(EngineCommand::Common(
            EngineCommandCommon::SetPreviewPlaying { playing },
        ));
        self.sync_preview_playback_side_effects(state, was_playing);
    }
}

impl App {
    /// Procesa un comando IPC. Devuelve `true` si el motor debe apagarse.
    pub fn handle_command(&mut self, cmd: EngineCommand) -> bool {
        // El IPC thread envió un comando vía EventLoopProxy — procesar de inmediato.
        if matches!(cmd, EngineCommand::Common(EngineCommandCommon::Shutdown)) {
            return true;
        }
        if let EngineCommand::Common(EngineCommandCommon::SetTargetFps { fps }) = &cmd {
            self.target_fps = (*fps).clamp(1, 1000);
            self.next_frame_at = std::time::Instant::now();
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
                crate::engine3d::config_3d::static_model_cache::MODEL_GPU_PARTS_PER_FRAME,
            );
        }

        if let Some(playing) = preview_toggle
            && let Some(mut state) = self.state.take()
        {
            self.set_preview_playing(&mut state, playing);
            self.state = Some(state);
        }
        false
    }
}

impl App {
    /// Input de mando y limpieza de teclas al salir de play. Una vez por frame.
    pub fn tick(&mut self) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
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

    /// Procesa un evento de ventana. Devuelve `true` si la ventana pidió cerrarse.
    pub fn handle_window_event(&mut self, event: WindowEvent) -> bool {
        // El avance de simulación se separa del render: el editor renderiza la escena en
        // su propio command buffer y el host standalone lo hace en `render_standalone`.
        if matches!(event, WindowEvent::RedrawRequested) {
            self.frame_update();
            return false;
        }
        let mut preview_toggle = None;
        let mut release_cursor_on_focus_loss = false;
        let mut recapture_cursor_on_focus_gain = false;

        {
            let Some(state) = self.state.as_mut() else {
                return false;
            };

            // ── Eventos de ventana ───────────────────────────────────────────────
            match event {
                WindowEvent::CloseRequested => {
                    return true;
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
                    if pressed && !state.is_preview_playing() {
                        // En modo overlay la ventana del motor es `WS_EX_NOACTIVATE`, así que el
                        // teclado se queda en Electron: sin tomar foco al clicar, ni WASD ni los
                        // atajos del editor (G, R, Numpad-.) llegan nunca al motor.
                        state.window().focus_window();
                    }
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
                                return false;
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
                                return false;
                            }
                            if pressed {
                                if self.entity_grab.is_some() {
                                    finish_entity_grab_confirm(&mut self.entity_grab, state);
                                    self.left_click_pos = None;
                                    return false;
                                }
                                // En modo quick_build_place los clicks son capturados por la herramienta;
                                // no se deben seleccionar entidades ni activar el gizmo.
                                let is_placement_tool =
                                    crate::engine3d::config_compat::is_editor_placement_tool(
                                        &state.active_tool,
                                    );

                                if is_placement_tool && pressed {
                                    if let Some(cur) = self.last_cursor {
                                        let ctrl_active = self.ctrl_held || query_ctrl_held_os();
                                        state.ctrl_held = ctrl_active;
                                        match &state.active_tool {
                                            crate::engine3d::config_compat::ActiveTool::QuickBuildPlace {
                                                ..
                                            } => {
                                                state.place_quick_build_at_cursor(Some(cur));
                                            }
                                            crate::engine3d::config_compat::ActiveTool::PlacePlaneTool {
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
                                    crate::engine3d::config_compat::is_editor_placement_tool(
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
                                                    crate::engine3d::ipc::send_event(
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
                        MouseButton::Right => {
                            // RMB + arrastre = mirar alrededor en el viewport de edición.
                            self.mouse_right = pressed;
                            if !pressed && !state.is_preview_playing() && self.entity_grab.is_some()
                            {
                                cancel_entity_grab(&mut self.entity_grab, state);
                            }
                        }
                        MouseButton::Middle => {
                            self.mouse_middle = pressed;
                        }
                        _ => {}
                    }
                    if !pressed && matches!(button, MouseButton::Middle | MouseButton::Right) {
                        // Sin esto, el siguiente arrastre hereda el delta del punto donde se soltó.
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
                        return false;
                    }
                    if self.cursor_captured && state.is_play_controller_active() {
                        self.last_cursor = None;
                        return false;
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
                            crate::engine3d::config_compat::is_editor_placement_tool(
                                &state.active_tool,
                            );
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
                                    None if self.mouse_right => {
                                        // RMB: mirar estilo FPS (gira alrededor del ojo, no del pivote).
                                        state.editor_fly_look(dx, dy);
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
                        && !self.mouse_right
                        && self.entity_drag.is_none()
                        && self.entity_grab.is_none()
                    {
                        let is_placement_tool =
                            crate::engine3d::config_compat::is_editor_placement_tool(
                                &state.active_tool,
                            );
                        let is_plane_preview = crate::engine3d::config_compat::is_plane_tool_active(
                            &state.active_tool,
                        );
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
                        crate::engine3d::config_compat::ActiveTool::PlacePlaneTool { .. }
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
                        return false;
                    }

                    let Some(code) = key_code_from_physical(physical_key) else {
                        return false;
                    };

                    // Estado de teclado para el vuelo del viewport de edición. Se registra antes de
                    // los `return` de abajo porque esos consumen el evento, no la tecla.
                    if pressed {
                        self.editor_keys.insert(code);
                    } else {
                        self.editor_keys.remove(&code);
                    }

                    if state.player_ui_edit_key_input(code, pressed, repeat) {
                        return false;
                    }
                    if state.player_ui_text_key_input(
                        code,
                        pressed,
                        repeat,
                        text.as_ref().map(|s| s.as_str()),
                    ) {
                        return false;
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
                                && !crate::engine3d::config_compat::is_editor_placement_tool(
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
                                && !crate::engine3d::config_compat::is_editor_placement_tool(
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
                        // Sin foco no llegan los `KeyUp`: si no se limpian, WASD se queda "pegado"
                        // y la cámara del editor sigue volando aunque se suelten las teclas.
                        self.editor_keys.clear();
                        self.mouse_right = false;
                        self.mouse_middle = false;
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
                        return false;
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
        false
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
        let mut attrs = Window::default_attributes()
            .with_title("RER-ENGINE - Viewport")
            .with_window_icon(rer_engine_core::platform::app_window_icon());

        attrs = attrs
            .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32))
            .with_decorations(true);

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("No se pudo crear la ventana"),
        );

        match pollster::block_on(crate::engine3d::engine::State::new(Arc::clone(&window))) {
            Ok(mut state) => {
                let from_save = std::env::var("RER_3D_START_FROM_SAVE")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false);
                if from_save {
                    state.setup_empty_3d();
                    crate::engine3d::ipc::send_event(&EngineEvent::Ready {
                        gravity: state.physics.gravity_magnitude(),
                    });
                } else {
                    state.setup_default_3d_scene();
                }
                log::info!(
                    "[editor] navegación del viewport: RMB mirar · WASD volar · Space/C subir/bajar · Shift sprint · MMB órbita · Shift+MMB paneo · Ctrl+MMB dolly · rueda zoom · Numpad. encuadrar selección"
                );
                self.state = Some(state);
            }
            Err(e) => {
                log::error!("Inicialización GPU fallida: {e}");
                crate::engine3d::ipc::send_event(&EngineEvent::Error { message: e.message });
            }
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, cmd: EngineCommand) {
        if self.handle_command(cmd) {
            event_loop.exit();
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
        let is_redraw = matches!(event, WindowEvent::RedrawRequested);
        if self.handle_window_event(event) {
            event_loop.exit();
            return;
        }
        if is_redraw {
            self.render_standalone();
        }
    }

    /// Llamado cuando winit ha procesado todos los eventos pendientes del ciclo actual.
    /// Es el único lugar correcto para pedir el siguiente frame en modo Poll.
    /// Usando WaitUntil capamos al FPS objetivo y el CPU puede dormir entre frames.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let frame_duration =
            std::time::Duration::from_nanos(1_000_000_000 / self.target_fps.max(1));

        self.tick();

        let now = std::time::Instant::now();
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
pub fn run() {
    // Logs van a stderr; IPC usa stdout.
    // La capa de validación y el contexto Ash viven ahora en rer_engine_core::vulkan. Sin listar ese
    // módulo, sus [vulkan-validación] (nivel warn) quedan filtrados por rer_engine=warn: son justo
    // los mensajes que delatan un layout de imagen mal declarado.
    const DEFAULT_LOG_FILTER: &str = "rer_engine=warn,rer_engine::engine3d::config_base=info,\
rer_engine::engine3d::config_3d::skin_diag=info,\
rer_engine::engine3d::config_3d::model_asset=info,\
rer_engine::engine3d::config_3d::reflection_settings=info,\
rer_engine::engine3d::engine::commands=info,\
rer_engine::engine3d::reflections=info,\
rer_engine::engine3d::engine::render=info,\
rer_engine_core::vulkan=info";

    rer_engine_core::logging::init(DEFAULT_LOG_FILTER);

    // Canal IPC: hilo stdin → event loop vía EventLoopProxy (despierta el loop inmediatamente)
    let event_loop = EventLoop::<EngineCommand>::with_user_event()
        .build()
        .expect("No se pudo crear EventLoop");
    let proxy = event_loop.create_proxy();
    crate::engine3d::ipc::start_ipc_thread(proxy);

    // ControlFlow se gestiona dinámicamente en about_to_wait con WaitUntil(next_frame).
    // NO usar Poll aquí: Poll + request_redraw en RedrawRequested = busy loop al 100% CPU.

    let gpu = resolve_backend(EngineGpuProfile::ThreeD).label();
    log::info!("Modo standalone (GPU: {gpu})");

    let mut app = App {
        state: None,
        mouse_middle: false,
        mouse_right: false,
        editor_keys: HashSet::new(),
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
        cursor_captured: false,
    };
    event_loop
        .run_app(&mut app)
        .expect("Error en el event loop");
}
