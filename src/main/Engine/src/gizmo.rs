// ---------------------------------------------------------------------------
// Gizmos — ejes X/Y/Z visuales
//
// Se renderizan como flechas sólidas 3D usando triángulos. El pipeline ignora
// el depth buffer para que siempre sean visibles encima de la geometría.
// ---------------------------------------------------------------------------

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GizmoVertex {
    pub position: [f32; 3],
    pub color:    [f32; 4],
}

impl GizmoVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode:    wgpu::VertexStepMode::Vertex,
            attributes:   &Self::ATTRIBS,
        }
    }
}

pub struct GizmoBuffer {
    pub vertex_buffer: wgpu::Buffer,
    pub vertex_count:  u32,
}

fn push_tri(verts: &mut Vec<GizmoVertex>, a: [f32; 3], b: [f32; 3], c: [f32; 3], color: [f32; 4]) {
    verts.push(GizmoVertex { position: a, color });
    verts.push(GizmoVertex { position: b, color });
    verts.push(GizmoVertex { position: c, color });
}

fn push_quad(verts: &mut Vec<GizmoVertex>, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3], color: [f32; 4]) {
    push_tri(verts, a, b, c, color);
    push_tri(verts, a, c, d, color);
}

fn add_arrow_x(verts: &mut Vec<GizmoVertex>, length: f32, color: [f32; 4]) {
    let tip = length;
    let base = length * 0.77;
    let shaft = length * 0.022;
    let head = length * 0.072;

    let p000 = [0.0, -shaft, -shaft];
    let p001 = [0.0, -shaft,  shaft];
    let p010 = [0.0,  shaft, -shaft];
    let p011 = [0.0,  shaft,  shaft];
    let p100 = [base, -shaft, -shaft];
    let p101 = [base, -shaft,  shaft];
    let p110 = [base,  shaft, -shaft];
    let p111 = [base,  shaft,  shaft];

    push_quad(verts, p000, p100, p110, p010, color);
    push_quad(verts, p001, p011, p111, p101, color);
    push_quad(verts, p000, p001, p101, p100, color);
    push_quad(verts, p010, p110, p111, p011, color);
    push_quad(verts, p000, p010, p011, p001, color);
    push_quad(verts, p100, p101, p111, p110, color);

    let b0 = [base, -head, -head];
    let b1 = [base, -head,  head];
    let b2 = [base,  head,  head];
    let b3 = [base,  head, -head];
    let apex = [tip, 0.0, 0.0];
    push_tri(verts, b0, b1, apex, color);
    push_tri(verts, b1, b2, apex, color);
    push_tri(verts, b2, b3, apex, color);
    push_tri(verts, b3, b0, apex, color);
}

fn add_arrow_y(verts: &mut Vec<GizmoVertex>, length: f32, color: [f32; 4]) {
    let tip = length;
    let base = length * 0.77;
    let shaft = length * 0.022;
    let head = length * 0.072;

    let p000 = [-shaft, 0.0, -shaft];
    let p001 = [-shaft, 0.0,  shaft];
    let p010 = [ shaft, 0.0, -shaft];
    let p011 = [ shaft, 0.0,  shaft];
    let p100 = [-shaft, base, -shaft];
    let p101 = [-shaft, base,  shaft];
    let p110 = [ shaft, base, -shaft];
    let p111 = [ shaft, base,  shaft];

    push_quad(verts, p000, p100, p110, p010, color);
    push_quad(verts, p001, p011, p111, p101, color);
    push_quad(verts, p000, p001, p101, p100, color);
    push_quad(verts, p010, p110, p111, p011, color);
    push_quad(verts, p000, p010, p011, p001, color);
    push_quad(verts, p100, p101, p111, p110, color);

    let b0 = [-head, base, -head];
    let b1 = [-head, base,  head];
    let b2 = [ head, base,  head];
    let b3 = [ head, base, -head];
    let apex = [0.0, tip, 0.0];
    push_tri(verts, b0, b1, apex, color);
    push_tri(verts, b1, b2, apex, color);
    push_tri(verts, b2, b3, apex, color);
    push_tri(verts, b3, b0, apex, color);
}

fn add_arrow_z(verts: &mut Vec<GizmoVertex>, length: f32, color: [f32; 4]) {
    let tip = length;
    let base = length * 0.77;
    let shaft = length * 0.022;
    let head = length * 0.072;

    let p000 = [-shaft, -shaft, 0.0];
    let p001 = [-shaft,  shaft, 0.0];
    let p010 = [ shaft, -shaft, 0.0];
    let p011 = [ shaft,  shaft, 0.0];
    let p100 = [-shaft, -shaft, base];
    let p101 = [-shaft,  shaft, base];
    let p110 = [ shaft, -shaft, base];
    let p111 = [ shaft,  shaft, base];

    push_quad(verts, p000, p100, p110, p010, color);
    push_quad(verts, p001, p011, p111, p101, color);
    push_quad(verts, p000, p001, p101, p100, color);
    push_quad(verts, p010, p110, p111, p011, color);
    push_quad(verts, p000, p010, p011, p001, color);
    push_quad(verts, p100, p101, p111, p110, color);

    let b0 = [-head, -head, base];
    let b1 = [-head,  head, base];
    let b2 = [ head,  head, base];
    let b3 = [ head, -head, base];
    let apex = [0.0, 0.0, tip];
    push_tri(verts, b0, b1, apex, color);
    push_tri(verts, b1, b2, apex, color);
    push_tri(verts, b2, b3, apex, color);
    push_tri(verts, b3, b0, apex, color);
}

pub fn build_axes(device: &wgpu::Device, length: f32) -> GizmoBuffer {
    let mut verts = Vec::new();
    add_arrow_x(&mut verts, length, [1.0, 0.18, 0.18, 1.0]);
    add_arrow_y(&mut verts, length, [0.18, 1.0, 0.18, 1.0]);
    add_arrow_z(&mut verts, length, [0.18, 0.55, 1.0, 1.0]);

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gizmo-vbo"),
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    });

    GizmoBuffer { vertex_buffer, vertex_count: verts.len() as u32 }
}

/// Creates a GizmoBuffer from arbitrary pre-built line vertices (tool overlays, etc.).
pub fn build_from_vertices(device: &wgpu::Device, verts: &[GizmoVertex]) -> GizmoBuffer {
    // Always allocate at least one vertex so the buffer is valid.
    let data: &[u8] = if verts.is_empty() {
        &[0u8; std::mem::size_of::<GizmoVertex>()]
    } else {
        bytemuck::cast_slice(verts)
    };
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label:    Some("tool-overlay-vbo"),
        contents: data,
        usage:    wgpu::BufferUsages::VERTEX,
    });
    GizmoBuffer { vertex_buffer, vertex_count: verts.len() as u32 }
}
