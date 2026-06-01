//! Vista de edición de UI del jugador (3D): cámara FPS del panel Cámara sin modo play.

use glam::Vec3;

use crate::config_compat::ActiveTool;
use crate::gizmo::{self, GizmoBuffer, GizmoVertex};
use crate::engine::State;

/// Snapshot del viewport de editor al entrar en edición UI.
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

/// Divisiones de la regla en pantalla (NDC).
const SCREEN_GRID_DIVISIONS: u32 = 24;

/// Uniformes NDC (view_proj + model identidad), mismo layout que crosshair / grid 2D.
const NDC_SCREEN_UNIFORM: [[f32; 4]; 9] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
    [-1.0, -1.0, 0.0, 0.0],
];

impl State {
    /// Vista desde el ojo del jugador (play real o edición UI), sin órbita de editor.
    pub(crate) fn uses_player_fps_viewport(&self) -> bool {
        self.has_play_character()
            && (self.is_play_controller_active() || self.player_ui_edit_active)
    }

    pub(crate) fn rebuild_player_ui_screen_grid(&mut self) {
        self.ui_work_grid_buffer = if self.player_ui_edit_active {
            let buf = build_player_ui_screen_grid(&self.device);
            log::info!(
                "[player-ui] cuadrícula pantalla: {} vértices",
                buf.vertex_count
            );
            buf
        } else {
            gizmo::build_from_vertices(&self.device, &[])
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
            self.active_tool = ActiveTool::None;
            self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
            self.set_player_ui_edit_context(
                scope.map(str::to_string),
                screen_id.map(str::to_string),
            );
            self.rebuild_player_ui_screen_grid();
            self.emit_player_ui_text_boxes_list();
            log::info!("[player-ui] vista de edición activada (cámara jugador, sin play)");
        } else {
            self.player_ui_edit_active = false;
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
            log::info!("[player-ui] vista de edición desactivada");
        }
    }

    /// Regla NDC sobre el viewport (mismo pass que crosshair: `grid_pipeline` + `LineList`).
    pub(crate) fn draw_player_ui_screen_grid(
        &self,
        enc: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) -> u32 {
        if !self.player_ui_edit_active || self.ui_work_grid_buffer.vertex_count == 0 {
            return 0;
        }

        self.queue.write_buffer(
            &self.grid_buffer_uni,
            0,
            bytemuck::cast_slice(&NDC_SCREEN_UNIFORM),
        );

        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("player-ui-screen-grid-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.grid_pipeline);
        pass.set_bind_group(0, &self.grid_bind_group, &[]);
        pass.set_vertex_buffer(0, self.ui_work_grid_buffer.vertex_buffer.slice(..));
        pass.draw(0..self.ui_work_grid_buffer.vertex_count, 0..1);
        1
    }
}

/// Cuadrícula de trabajo en NDC (regla sobre el viewport del jugador).
fn build_player_ui_screen_grid(device: &wgpu::Device) -> GizmoBuffer {
    let mut verts: Vec<GizmoVertex> = Vec::new();

    let extent = 1.0_f32;
    let step = (2.0 * extent) / SCREEN_GRID_DIVISIONS as f32;
    let z = 0.0_f32;

    let gc = [0.42_f32, 0.42, 0.52, 0.72];
    let bc = [0.88_f32, 0.88, 0.92, 0.95];
    let xc = [0.82_f32, 0.32, 0.32, 0.92];
    let yc = [0.32_f32, 0.78, 0.38, 0.92];

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

    let mut t = -extent;
    while t <= extent + 1e-4 {
        if t.abs() > step * 0.45 {
            push_line(&mut verts, [t, -extent, z], [t, extent, z], gc);
            push_line(&mut verts, [-extent, t, z], [extent, t, z], gc);
        }
        t += step;
    }

    gizmo::build_from_vertices(device, &verts)
}

fn push_line(verts: &mut Vec<GizmoVertex>, a: [f32; 3], b: [f32; 3], color: [f32; 4]) {
    verts.push(GizmoVertex { position: a, color });
    verts.push(GizmoVertex { position: b, color });
}
