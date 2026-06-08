//! Primitivas NDC (quads y líneas) para overlays HUD.

use bytemuck::{Pod, Zeroable};

use super::config::UiHudRect;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct NdcVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

impl NdcVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub fn push_quad(
    verts: &mut Vec<NdcVertex>,
    p0: [f32; 3],
    p1: [f32; 3],
    p2: [f32; 3],
    p3: [f32; 3],
    color: [f32; 4],
) {
    verts.push(NdcVertex { position: p0, color });
    verts.push(NdcVertex { position: p1, color });
    verts.push(NdcVertex { position: p2, color });
    verts.push(NdcVertex { position: p0, color });
    verts.push(NdcVertex { position: p2, color });
    verts.push(NdcVertex { position: p3, color });
}

pub fn push_line_segment(
    verts: &mut Vec<NdcVertex>,
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

pub fn push_handle_disc(
    verts: &mut Vec<NdcVertex>,
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

pub fn append_rect_outline(verts: &mut Vec<NdcVertex>, rect: UiHudRect, color: [f32; 4]) {
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

pub fn append_rect_fill(verts: &mut Vec<NdcVertex>, rect: UiHudRect, color: [f32; 4]) {
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
