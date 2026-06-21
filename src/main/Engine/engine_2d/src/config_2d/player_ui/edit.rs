//! Vista de edición de UI del jugador (2D): cuadrícula NDC sobre el viewport.

use crate::config_2d::ActiveTool;
use crate::gizmo::{self, GizmoBuffer, GizmoVertex};
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};
use rer_engine_shared::player_ui::grid::NDC_SCREEN_UNIFORM;

pub(crate) use rer_engine_shared::player_ui::grid::{
    player_ui_grid_steps, snap_ndc_point_to_grid,
};

impl State {
    pub(crate) fn rebuild_player_ui_screen_grid(&mut self) {
        self.ui_work_grid_buffer = if self.player_ui_edit_active {
            let vw = self.size.width.max(1) as f32;
            let vh = self.size.height.max(1) as f32;
            let buf = build_player_ui_screen_grid(&self.device, vw, vh);
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
            self.player_ui_edit_active = true;
            self.suspend_world_picking_for_ui_edit();
            self.active_tool = ActiveTool::None;
            self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
            self.set_player_ui_edit_context(
                scope.map(str::to_string),
                screen_id.map(str::to_string),
            );
            self.rebuild_player_ui_screen_grid();
            self.emit_player_ui_text_boxes_list();
            log::info!("[player-ui] vista de edición activada (2D)");
        } else {
            self.player_ui_edit_active = false;
            self.cancel_player_ui_object_draw();
            self.clear_player_ui_text_interaction();
            self.set_player_ui_edit_context(None, None);
            self.rebuild_player_ui_screen_grid();
            log::info!("[player-ui] vista de edición desactivada");
        }
    }

    /// Regla NDC sobre el viewport (mismo pass que grid 2D: `grid_pipeline` + `LineList`).
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
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.grid_pipeline);
        pass.set_bind_group(0, &self.grid_bind_group, &[]);
        pass.set_vertex_buffer(0, self.ui_work_grid_buffer.vertex_buffer.slice(..));
        pass.draw(0..self.ui_work_grid_buffer.vertex_count, 0..1);
        1
    }

    /// En edición UI el ratón solo interactúa con el HUD; sin hover/selección del mundo 2D.
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

/// Cuadrícula de trabajo en NDC (regla sobre el viewport).
fn build_player_ui_screen_grid(device: &wgpu::Device, viewport_w: f32, viewport_h: f32) -> GizmoBuffer {
    let mut verts: Vec<GizmoVertex> = Vec::new();

    let extent = 1.0_f32;
    let (step_x, step_y) =
        rer_engine_shared::player_ui::grid::player_ui_grid_steps(viewport_w, viewport_h);
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

    gizmo::build_from_vertices(device, &verts)
}

fn push_line(verts: &mut Vec<GizmoVertex>, a: [f32; 3], b: [f32; 3], color: [f32; 4]) {
    verts.push(GizmoVertex { position: a, color });
    verts.push(GizmoVertex { position: b, color });
}
