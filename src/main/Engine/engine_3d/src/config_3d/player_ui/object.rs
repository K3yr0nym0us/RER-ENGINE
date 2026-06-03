//! Objetos HUD poligonales (dibujo por clicks en NDC, relleno en overlay).

use crate::engine::State;
use crate::gizmo::{self, GizmoVertex};
use crate::ipc::{
    send_event, EngineEvent, PlayerUiObjectListItem, SavePlayerUiObjectSnapshot,
};

use crate::platform::query_ctrl_held_os;

use super::config::PlayerUiObject;
use super::edit::{player_ui_grid_steps, snap_ndc_point_to_grid};
use super::hud_layers;
use super::ndc_draw::{push_handle_disc, push_line_segment, push_quad};
use super::text_render::HANDLE_RADIUS;

/// Distancia NDC (con aspecto) para cerrar el polígono al clicar el primer punto.
const CLOSE_THRESHOLD: f32 = 0.028;
const DRAW_CROSS_ARM: f32 = 0.018;
pub(crate) const DEFAULT_OBJECT_FILL: [f32; 4] = [0.28, 0.55, 0.92, 0.72];

#[derive(Clone, Debug)]
pub(crate) struct PlayerUiObjectDrawSession {
    pub points_ndc: Vec<[f32; 2]>,
    pub cursor_ndc: Option<[f32; 2]>,
}

impl State {
    fn player_ui_snap_ndc_from_pixel(&self, px: f32, py: f32) -> [f32; 2] {
        let mut ndc = self.pixel_to_ndc(px, py);
        if self.ctrl_held || query_ctrl_held_os() {
            let vw = self.size.width.max(1) as f32;
            let vh = self.size.height.max(1) as f32;
            let (step_x, step_y) = player_ui_grid_steps(vw, vh);
            ndc = snap_ndc_point_to_grid(ndc[0], ndc[1], step_x, step_y);
        }
        ndc
    }

    pub(crate) fn player_ui_object_draw_active(&self) -> bool {
        self.player_ui_object_draw.is_some()
    }

    pub(crate) fn set_player_ui_object_draw(&mut self, active: bool) {
        if !self.player_ui_edit_active {
            return;
        }
        if active {
            self.player_ui_object_draw = Some(PlayerUiObjectDrawSession {
                points_ndc: Vec::new(),
                cursor_ndc: None,
            });
            self.clear_player_ui_hud_selection();
            self.player_ui_text_editing_id = None;
            self.rebuild_player_ui_object_draw_overlay();
            log::info!("[player-ui] modo dibujo de objeto HUD activado");
        } else {
            self.cancel_player_ui_object_draw();
        }
    }

    pub(crate) fn cancel_player_ui_object_draw(&mut self) {
        let had_points = self
            .player_ui_object_draw
            .as_ref()
            .is_some_and(|s| !s.points_ndc.is_empty());
        if self.player_ui_object_draw.is_none() {
            return;
        }
        if had_points {
            self.push_undo_player_ui_hud();
        }
        self.player_ui_object_draw = None;
        self.player_ui_object_draw_overlay =
            gizmo::build_from_vertices(&self.device, &[]);
        send_event(&EngineEvent::PlayerUiObjectDrawEnded);
        log::info!("[player-ui] dibujo de objeto HUD cancelado");
    }

    pub(crate) fn update_player_ui_object_draw_cursor(&mut self, px: f32, py: f32) {
        if self.player_ui_object_draw.is_none() {
            return;
        }
        let cursor = self.player_ui_snap_ndc_from_pixel(px, py);
        if let Some(session) = self.player_ui_object_draw.as_mut() {
            session.cursor_ndc = Some(cursor);
        }
        self.rebuild_player_ui_object_draw_overlay();
    }

    /// Devuelve `true` si el click fue consumido por el modo dibujo.
    pub(crate) fn handle_player_ui_object_draw_click(&mut self, px: f32, py: f32) -> bool {
        if self.player_ui_object_draw.is_none() {
            return false;
        }
        self.push_undo_player_ui_hud();
        let ndc = self.player_ui_snap_ndc_from_pixel(px, py);
        let vw = self.size.width.max(1) as f32;
        let vh = self.size.height.max(1) as f32;

        let close_polygon = self
            .player_ui_object_draw
            .as_ref()
            .is_some_and(|s| s.points_ndc.len() >= 3 && ndc_near(s.points_ndc[0], ndc, vw, vh));

        if close_polygon {
            let vertices = self
                .player_ui_object_draw
                .as_ref()
                .map(|s| s.points_ndc.clone())
                .unwrap_or_default();
            self.player_ui_object_draw = None;
            self.player_ui_object_draw_overlay =
                gizmo::build_from_vertices(&self.device, &[]);
            self.create_player_ui_object_from_vertices(&vertices);
            return true;
        }

        if let Some(session) = self.player_ui_object_draw.as_mut() {
            session.points_ndc.push(ndc);
            session.cursor_ndc = Some(ndc);
            self.rebuild_player_ui_object_draw_overlay();
        }
        true
    }

    fn create_player_ui_object_from_vertices(&mut self, vertices: &[[f32; 2]]) {
        if vertices.len() < 3 {
            return;
        }
        let Some(key) = self.player_ui_screen_key() else {
            return;
        };

        let id = self.player_ui_text_next_id;
        self.player_ui_text_next_id = self.player_ui_text_next_id.saturating_add(1);

        let texts = self.player_ui_text_boxes.get(&key).map(|v| v.as_slice());
        let buttons = self.player_ui_buttons.get(&key).map(|v| v.as_slice());
        let images = self.player_ui_images.get(&key).map(|v| v.as_slice());
        let objects = self.player_ui_objects.get(&key).map(|v| v.as_slice());
        let z_index = hud_layers::next_z_index_for_screen(texts, buttons, images, objects);

        let entry = PlayerUiObject {
            id,
            vertices: vertices.to_vec(),
            fill_color: DEFAULT_OBJECT_FILL,
            z_index,
            locked: false,
        };

        self.player_ui_objects.entry(key).or_default().push(entry);
        self.player_ui_selected_object_id = Some(id);
        self.player_ui_selected_text_id = None;
        self.player_ui_selected_button_id = None;
        self.player_ui_selected_image_id = None;
        self.rebuild_player_ui_overlay();
        self.emit_player_ui_text_boxes_list();
        send_event(&EngineEvent::PlayerUiObjectAdded {
            id,
            vertex_count: vertices.len() as u32,
        });
        log::info!(
            "[player-ui] objeto HUD creado: id={} vértices={}",
            id,
            vertices.len()
        );
    }

    pub(crate) fn remove_player_ui_object(&mut self, id: u32) -> bool {
        let Some(key) = self.player_ui_screen_key() else {
            return false;
        };
        if !self
            .player_ui_objects
            .get(&key)
            .is_some_and(|list| list.iter().any(|o| o.id == id))
        {
            return false;
        }
        self.push_undo_player_ui_hud();
        let Some(list) = self.player_ui_objects.get_mut(&key) else {
            return false;
        };
        list.retain(|o| o.id != id);
        if self.player_ui_selected_object_id == Some(id) {
            self.player_ui_selected_object_id = None;
        }
        self.player_ui_text_drag = None;
        self.rebuild_player_ui_overlay();
        self.emit_player_ui_text_boxes_list();
        send_event(&EngineEvent::PlayerUiObjectRemoved { id });
        log::info!("[player-ui] objeto HUD eliminado: id={id}");
        true
    }

    pub(crate) fn import_player_ui_objects_from_save(
        &mut self,
        objects: &[SavePlayerUiObjectSnapshot],
    ) {
        self.player_ui_objects.clear();
        let mut max_id = 0u32;
        for snap in objects {
            if snap.vertices.len() < 3 {
                continue;
            }
            max_id = max_id.max(snap.id);
            let key = format!("{}:{}", snap.scope, snap.screen_id);
            self.player_ui_objects.entry(key).or_default().push(PlayerUiObject {
                id: snap.id,
                vertices: snap.vertices.clone(),
                fill_color: snap.fill_color,
                z_index: snap.z_index,
                locked: snap.locked,
            });
        }
        if max_id > 0 {
            self.player_ui_text_next_id = self
                .player_ui_text_next_id
                .max(max_id.saturating_add(1));
        }
        if self.player_ui_edit_active {
            self.rebuild_player_ui_overlay();
        }
        log::info!(
            "[player-ui] importados {} objetos HUD desde .save",
            objects.len()
        );
    }

    pub(crate) fn rebuild_player_ui_object_draw_overlay(&mut self) {
        let verts = self
            .player_ui_object_draw
            .as_ref()
            .map(|s| build_object_draw_overlay_verts(&s.points_ndc, s.cursor_ndc))
            .unwrap_or_default();
        self.player_ui_object_draw_overlay =
            gizmo::build_from_vertices(&self.device, &verts);
    }

    pub(crate) fn draw_player_ui_object_draw_overlay(
        &self,
        enc: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) -> u32 {
        if !self.player_ui_edit_active
            || self.player_ui_object_draw_overlay.vertex_count == 0
        {
            return 0;
        }
        const NDC_IDENTITY: [[f32; 4]; 9] = [
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
        self.queue.write_buffer(
            &self.gizmo_buffer_uni,
            0,
            bytemuck::cast_slice(&NDC_IDENTITY),
        );
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("player-ui-object-draw-overlay-pass"),
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
        pass.set_pipeline(&self.gizmo_pipeline);
        pass.set_bind_group(0, &self.gizmo_bind_group, &[]);
        pass.set_vertex_buffer(
            0,
            self.player_ui_object_draw_overlay.vertex_buffer.slice(..),
        );
        pass.draw(
            0..self.player_ui_object_draw_overlay.vertex_count,
            0..1,
        );
        1
    }
}

pub(crate) fn list_objects_for_event(state: &State, key: &str) -> Vec<PlayerUiObjectListItem> {
    state
        .player_ui_objects
        .get(key)
        .map(|list| {
            list.iter()
                .map(|o| PlayerUiObjectListItem {
                    id: o.id,
                    vertex_count: o.vertices.len() as u32,
                    z_index: o.z_index,
                    locked: o.locked,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn ndc_near(a: [f32; 2], b: [f32; 2], vw: f32, vh: f32) -> bool {
    let aspect = (vw.max(1.0)) / vh.max(1.0);
    let dx = (a[0] - b[0]) * aspect;
    let dy = a[1] - b[1];
    dx * dx + dy * dy <= CLOSE_THRESHOLD * CLOSE_THRESHOLD
}

fn push_point_cross(verts: &mut Vec<GizmoVertex>, x: f32, y: f32, color: [f32; 4]) {
    let z = 0.0_f32;
    push_line_segment(
        verts,
        [x - DRAW_CROSS_ARM, y, z],
        [x + DRAW_CROSS_ARM, y, z],
        color,
    );
    push_line_segment(
        verts,
        [x, y - DRAW_CROSS_ARM, z],
        [x, y + DRAW_CROSS_ARM, z],
        color,
    );
}

fn build_object_draw_overlay_verts(
    pts: &[[f32; 2]],
    cursor: Option<[f32; 2]>,
) -> Vec<GizmoVertex> {
    let cross_color = [1.0_f32, 1.0, 1.0, 1.0];
    let line_color = [1.0_f32, 0.75, 0.0, 1.0];
    let handle_color = [1.0_f32, 0.85, 0.2, 0.95];
    let z = 0.0_f32;
    let mut verts = Vec::new();

    for (i, p) in pts.iter().enumerate() {
        let [x, y] = *p;
        if i == 0 {
            push_handle_disc(&mut verts, x, y, HANDLE_RADIUS, handle_color);
        } else {
            push_point_cross(&mut verts, x, y, cross_color);
        }
    }

    for i in 0..pts.len().saturating_sub(1) {
        let [ax, ay] = pts[i];
        let [bx, by] = pts[i + 1];
        push_line_segment(
            &mut verts,
            [ax, ay, z],
            [bx, by, z],
            line_color,
        );
    }

    if let (Some(last), Some(cur)) = (pts.last().copied(), cursor) {
        push_line_segment(&mut verts, [last[0], last[1], z], [cur[0], cur[1], z], line_color);
        if pts.len() >= 2 {
            let first = pts[0];
            push_line_segment(
                &mut verts,
                [first[0], first[1], z],
                [cur[0], cur[1], z],
                line_color,
            );
        }
    }

    if let Some([cx, cy]) = cursor {
        push_point_cross(&mut verts, cx, cy, cross_color);
    }

    verts
}

pub(crate) fn append_polygon_fill(
    verts: &mut Vec<GizmoVertex>,
    points: &[[f32; 2]],
    color: [f32; 4],
) {
    if points.len() < 3 {
        return;
    }
    let z = 0.0_f32;
    let p0 = points[0];
    for i in 1..points.len().saturating_sub(1) {
        let p1 = points[i];
        let p2 = points[i + 1];
        push_quad(
            verts,
            [p0[0], p0[1], z],
            [p1[0], p1[1], z],
            [p2[0], p2[1], z],
            [p0[0], p0[1], z],
            color,
        );
    }
}

pub(crate) fn append_object_gizmo_verts(
    verts: &mut Vec<GizmoVertex>,
    objects: &[PlayerUiObject],
    selected: Option<u32>,
) {
    for obj in objects {
        append_polygon_fill(verts, &obj.vertices, obj.fill_color);
        if selected == Some(obj.id) {
            let outline = [1.0_f32, 0.85, 0.2, 0.95];
            let n = obj.vertices.len();
            if n >= 2 {
                for i in 0..n {
                    let a = obj.vertices[i];
                    let b = obj.vertices[(i + 1) % n];
                    push_line_segment(verts, [a[0], a[1], 0.0], [b[0], b[1], 0.0], outline);
                }
            }
        }
    }
}

pub(crate) fn polygon_centroid(vertices: &[[f32; 2]]) -> [f32; 2] {
    if vertices.is_empty() {
        return [0.0, 0.0];
    }
    let n = vertices.len() as f32;
    let sx: f32 = vertices.iter().map(|v| v[0]).sum();
    let sy: f32 = vertices.iter().map(|v| v[1]).sum();
    [sx / n, sy / n]
}

pub(crate) fn point_in_polygon(ndc: [f32; 2], vertices: &[[f32; 2]]) -> bool {
    let n = vertices.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let (x, y) = (ndc[0], ndc[1]);
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (vertices[i][0], vertices[i][1]);
        let (xj, yj) = (vertices[j][0], vertices[j][1]);
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi + 1e-12) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}
