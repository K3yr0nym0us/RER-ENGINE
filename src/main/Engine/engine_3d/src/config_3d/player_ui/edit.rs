//! Vista de ediciÃƒÂ³n de UI del jugador (3D): cÃƒÂ¡mara FPS del panel CÃƒÂ¡mara sin modo play.

use glam::Vec3;

use crate::config_compat::ActiveTool;
use crate::engine::State;
use crate::gizmo::{self, GizmoBuffer, GizmoVertex};
use crate::ipc::{EngineEvent, send_event};
use crate::vulkan::SharedGpu;

pub(crate) use rer_engine_core::player_ui::grid::{player_ui_grid_steps, snap_ndc_point_to_grid};

/// Snapshot del viewport de editor al entrar en ediciÃƒÂ³n UI.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PlayerUiEditViewportRestore {
    editor_viewport_yaw: f32,
    editor_viewport_pitch: f32,
    editor_viewport_distance: f32,
    editor_orbit_target: Vec3,
    play_camera_eye_position: Vec3,
    camera_yaw: f32,
    camera_pitch: f32,
}

impl State {
    /// Vista desde el ojo del jugador (play real o ediciÃƒÂ³n UI), sin ÃƒÂ³rbita de editor.
    pub(crate) fn uses_player_fps_viewport(&self) -> bool {
        self.has_play_character()
            && (self.is_play_controller_active() || self.player_ui_edit_active)
    }

    pub(crate) fn rebuild_player_ui_screen_grid(&mut self) {
        self.ui_work_grid_buffer = if self.player_ui_edit_active {
            let vw = self.size.width.max(1) as f32;
            let vh = self.size.height.max(1) as f32;
            let buf = build_player_ui_screen_grid(&self.vulkan, vw, vh);
            log::info!(
                "[player-ui] cuadrÃƒÂ­cula pantalla: {} vÃƒÂ©rtices",
                buf.vertex_count
            );
            buf
        } else {
            gizmo::build_from_vertices(&self.vulkan, &[])
        };
    }

    pub(crate) fn apply_player_ui_edit_mode(
        &mut self,
        active: bool,
        scope: Option<&str>,
        screen_id: Option<&str>,
    ) {
        if active {
            if self.player_ui_edit_active {
                self.set_player_ui_edit_context(
                    scope.map(str::to_string),
                    screen_id.map(str::to_string),
                );
                self.emit_player_ui_text_boxes_list();
                return;
            }
        } else if !self.player_ui_edit_active {
            return;
        }

        if active {
            self.player_ui_edit_restore = Some(PlayerUiEditViewportRestore {
                editor_viewport_yaw: self.editor_viewport_yaw,
                editor_viewport_pitch: self.editor_viewport_pitch,
                editor_viewport_distance: self.editor_viewport_distance,
                editor_orbit_target: self.editor_orbit_target,
                play_camera_eye_position: self.play_camera_eye_position,
                camera_yaw: self.camera.yaw,
                camera_pitch: self.camera.pitch,
            });

            self.player_ui_edit_active = true;
            self.suspend_world_picking_for_ui_edit();
            self.active_tool = ActiveTool::None;
            self.tool_overlay_buffer = gizmo::build_from_vertices(&self.vulkan, &[]);
            self.set_player_ui_edit_context(
                scope.map(str::to_string),
                screen_id.map(str::to_string),
            );
            self.rebuild_player_ui_screen_grid();
            self.emit_player_ui_text_boxes_list();
            log::info!("[player-ui] vista de ediciÃƒÂ³n activada (cÃƒÂ¡mara jugador, sin play)");
        } else {
            self.player_ui_edit_active = false;
            self.cancel_player_ui_object_draw();
            self.clear_player_ui_text_interaction();
            self.set_player_ui_edit_context(None, None);
            if let Some(restore) = self.player_ui_edit_restore.take() {
                self.editor_viewport_yaw = restore.editor_viewport_yaw;
                self.editor_viewport_pitch = restore.editor_viewport_pitch;
                self.editor_viewport_distance = restore.editor_viewport_distance;
                self.editor_orbit_target = restore.editor_orbit_target;
                self.play_camera_eye_position = restore.play_camera_eye_position;
                self.camera.yaw = restore.camera_yaw;
                self.camera.pitch = restore.camera_pitch;
                self.sync_editor_camera_entity_from_viewport();
            }
            self.rebuild_player_ui_screen_grid();
            log::info!("[player-ui] vista de ediciÃƒÂ³n desactivada");
        }
    }

    /// En edición UI el ratón solo interactúa con el HUD; sin hover/selección del mundo 3D.
    pub(crate) fn suspend_world_picking_for_ui_edit(&mut self) {
        if self.hovered_entity.take().is_some() {
            send_event(&EngineEvent::EntityUnhovered);
        }
        self.hovered_gizmo_axis = None;
        self.active_gizmo_axis = None;
        if self.selected_entity.is_some() || !self.selected_entities.is_empty() {
            self.selected_entity = None;
            self.selected_entities.clear();
            send_event(&EngineEvent::EntityDeselected);
        }
    }
}

/// CuadrÃƒÂ­cula de trabajo en NDC (regla sobre el viewport del jugador).
fn build_player_ui_screen_grid(gpu: &SharedGpu, viewport_w: f32, viewport_h: f32) -> GizmoBuffer {
    let mut verts: Vec<GizmoVertex> = Vec::new();

    let extent = 1.0_f32;
    let (step_x, step_y) = player_ui_grid_steps(viewport_w, viewport_h);
    let z = 0.0_f32;

    let gc = [0.42_f32, 0.42, 0.52, 0.38];
    let bc = [0.88_f32, 0.88, 0.92, 0.55];
    let xc = [0.82_f32, 0.32, 0.32, 0.50];
    let yc = [0.32_f32, 0.78, 0.38, 0.50];

    let corners = [
        [-extent, extent, z],
        [extent, extent, z],
        [extent, -extent, z],
        [-extent, -extent, z],
    ];
    for i in 0..4_usize {
        push_line(&mut verts, corners[i], corners[(i + 1) % 4], bc);
    }
    push_line(&mut verts, [-extent, 0.0, z], [extent, 0.0, z], xc);
    push_line(&mut verts, [0.0, -extent, z], [0.0, extent, z], yc);

    let mut x = -extent;
    while x <= extent + 1e-4 {
        if x.abs() > step_x * 0.45 {
            push_line(&mut verts, [x, -extent, z], [x, extent, z], gc);
        }
        x += step_x;
    }
    let mut y = -extent;
    while y <= extent + 1e-4 {
        if y.abs() > step_y * 0.45 {
            push_line(&mut verts, [-extent, y, z], [extent, y, z], gc);
        }
        y += step_y;
    }

    gizmo::build_lines_from_vertices(gpu, &verts)
}

fn push_line(verts: &mut Vec<GizmoVertex>, a: [f32; 3], b: [f32; 3], color: [f32; 4]) {
    verts.push(GizmoVertex { position: a, color });
    verts.push(GizmoVertex { position: b, color });
}
