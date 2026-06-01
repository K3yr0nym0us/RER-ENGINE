//! Primitivas NDC (quads y líneas) para overlays HUD.

use crate::gizmo::GizmoVertex;

use super::config::UiHudRect;

pub(crate) fn push_quad(
    verts: &mut Vec<GizmoVertex>,
    p0: [f32; 3],
    p1: [f32; 3],
    p2: [f32; 3],
    p3: [f32; 3],
    color: [f32; 4],
) {
    verts.push(GizmoVertex { position: p0, color });
    verts.push(GizmoVertex { position: p1, color });
    verts.push(GizmoVertex { position: p2, color });
    verts.push(GizmoVertex { position: p0, color });
    verts.push(GizmoVertex { position: p2, color });
    verts.push(GizmoVertex { position: p3, color });
}

pub(crate) fn push_line_segment(
    verts: &mut Vec<GizmoVertex>,
    a: [f32; 3],
    b: [f32; 3],
    color: [f32; 4],
) {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return;
    }
    let t = 0.0014;
    let nx = -dy / len * t;
    let ny = dx / len * t;
    push_quad(
        verts,
        [a[0] + nx, a[1] + ny, 0.0],
        [b[0] + nx, b[1] + ny, 0.0],
        [b[0] - nx, b[1] - ny, 0.0],
        [a[0] - nx, a[1] - ny, 0.0],
        color,
    );
}

pub(crate) fn push_handle_disc(
    verts: &mut Vec<GizmoVertex>,
    cx: f32,
    cy: f32,
    radius: f32,
    color: [f32; 4],
) {
    let segments = 10;
    for i in 0..segments {
        let a0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
        let p0 = [cx + a0.cos() * radius, cy + a0.sin() * radius, 0.0];
        let p1 = [cx + a1.cos() * radius, cy + a1.sin() * radius, 0.0];
        push_quad(verts, [cx, cy, 0.0], p0, p1, [cx, cy, 0.0], color);
    }
}

pub(crate) fn append_rect_outline(
    verts: &mut Vec<GizmoVertex>,
    rect: UiHudRect,
    color: [f32; 4],
) {
    let hw = rect.width * 0.5;
    let hh = rect.height * 0.5;
    let x0 = rect.center_x - hw;
    let y0 = rect.center_y - hh;
    let x1 = rect.center_x + hw;
    let y1 = rect.center_y + hh;
    push_line_segment(verts, [x0, y0, 0.0], [x1, y0, 0.0], color);
    push_line_segment(verts, [x1, y0, 0.0], [x1, y1, 0.0], color);
    push_line_segment(verts, [x1, y1, 0.0], [x0, y1, 0.0], color);
    push_line_segment(verts, [x0, y1, 0.0], [x0, y0, 0.0], color);
}

pub(crate) fn append_rect_fill(verts: &mut Vec<GizmoVertex>, rect: UiHudRect, color: [f32; 4]) {
    let hw = rect.width * 0.5;
    let hh = rect.height * 0.5;
    push_quad(
        verts,
        [rect.center_x - hw, rect.center_y - hh, 0.0],
        [rect.center_x + hw, rect.center_y - hh, 0.0],
        [rect.center_x + hw, rect.center_y + hh, 0.0],
        [rect.center_x - hw, rect.center_y + hh, 0.0],
        color,
    );
}
