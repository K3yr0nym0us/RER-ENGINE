use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

// ---------------------------------------------------------------------------
// Vértice — layout debe coincidir con el shader (location 0,1,2)
// ---------------------------------------------------------------------------
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal:   [f32; 3],
    pub uv:       [f32; 2],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x3,  // position
        1 => Float32x3,  // normal
        2 => Float32x2,  // uv
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode:    wgpu::VertexStepMode::Vertex,
            attributes:   &Self::ATTRIBS,
        }
    }
}

// ---------------------------------------------------------------------------
// Mesh en GPU
// ---------------------------------------------------------------------------
pub struct Mesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer:  wgpu::Buffer,
    pub index_count:   u32,
}

// ---------------------------------------------------------------------------
// Cubo procedural — siempre disponible sin necesitar un archivo externo
// ---------------------------------------------------------------------------
pub fn create_cube(device: &wgpu::Device) -> Mesh {
    #[rustfmt::skip]
    let vertices: Vec<Vertex> = vec![
        // Front  (z =+0.5)
        Vertex { position: [-0.5, -0.5,  0.5], normal: [ 0.0,  0.0,  1.0], uv: [0.0, 1.0] },
        Vertex { position: [ 0.5, -0.5,  0.5], normal: [ 0.0,  0.0,  1.0], uv: [1.0, 1.0] },
        Vertex { position: [ 0.5,  0.5,  0.5], normal: [ 0.0,  0.0,  1.0], uv: [1.0, 0.0] },
        Vertex { position: [-0.5,  0.5,  0.5], normal: [ 0.0,  0.0,  1.0], uv: [0.0, 0.0] },
        // Back   (z =-0.5)
        Vertex { position: [ 0.5, -0.5, -0.5], normal: [ 0.0,  0.0, -1.0], uv: [0.0, 1.0] },
        Vertex { position: [-0.5, -0.5, -0.5], normal: [ 0.0,  0.0, -1.0], uv: [1.0, 1.0] },
        Vertex { position: [-0.5,  0.5, -0.5], normal: [ 0.0,  0.0, -1.0], uv: [1.0, 0.0] },
        Vertex { position: [ 0.5,  0.5, -0.5], normal: [ 0.0,  0.0, -1.0], uv: [0.0, 0.0] },
        // Top    (y =+0.5)
        Vertex { position: [-0.5,  0.5,  0.5], normal: [ 0.0,  1.0,  0.0], uv: [0.0, 1.0] },
        Vertex { position: [ 0.5,  0.5,  0.5], normal: [ 0.0,  1.0,  0.0], uv: [1.0, 1.0] },
        Vertex { position: [ 0.5,  0.5, -0.5], normal: [ 0.0,  1.0,  0.0], uv: [1.0, 0.0] },
        Vertex { position: [-0.5,  0.5, -0.5], normal: [ 0.0,  1.0,  0.0], uv: [0.0, 0.0] },
        // Bottom (y =-0.5)
        Vertex { position: [-0.5, -0.5, -0.5], normal: [ 0.0, -1.0,  0.0], uv: [0.0, 1.0] },
        Vertex { position: [ 0.5, -0.5, -0.5], normal: [ 0.0, -1.0,  0.0], uv: [1.0, 1.0] },
        Vertex { position: [ 0.5, -0.5,  0.5], normal: [ 0.0, -1.0,  0.0], uv: [1.0, 0.0] },
        Vertex { position: [-0.5, -0.5,  0.5], normal: [ 0.0, -1.0,  0.0], uv: [0.0, 0.0] },
        // Right  (x =+0.5)
        Vertex { position: [ 0.5, -0.5,  0.5], normal: [ 1.0,  0.0,  0.0], uv: [0.0, 1.0] },
        Vertex { position: [ 0.5, -0.5, -0.5], normal: [ 1.0,  0.0,  0.0], uv: [1.0, 1.0] },
        Vertex { position: [ 0.5,  0.5, -0.5], normal: [ 1.0,  0.0,  0.0], uv: [1.0, 0.0] },
        Vertex { position: [ 0.5,  0.5,  0.5], normal: [ 1.0,  0.0,  0.0], uv: [0.0, 0.0] },
        // Left   (x =-0.5)
        Vertex { position: [-0.5, -0.5, -0.5], normal: [-1.0,  0.0,  0.0], uv: [0.0, 1.0] },
        Vertex { position: [-0.5, -0.5,  0.5], normal: [-1.0,  0.0,  0.0], uv: [1.0, 1.0] },
        Vertex { position: [-0.5,  0.5,  0.5], normal: [-1.0,  0.0,  0.0], uv: [1.0, 0.0] },
        Vertex { position: [-0.5,  0.5, -0.5], normal: [-1.0,  0.0,  0.0], uv: [0.0, 0.0] },
    ];

    #[rustfmt::skip]
    let indices: Vec<u32> = vec![
         0,  1,  2,  2,  3,  0,  // Front
         4,  5,  6,  6,  7,  4,  // Back
         8,  9, 10, 10, 11,  8,  // Top
        12, 13, 14, 14, 15, 12,  // Bottom
        16, 17, 18, 18, 19, 16,  // Right
        20, 21, 22, 22, 23, 20,  // Left
    ];

    upload(device, &vertices, &indices, "cube")
}

/// Esfera UV unitaria (radio 0.5) para el icono del sol.
pub fn create_uv_sphere(device: &wgpu::Device, segments: u32) -> Mesh {
    let rings = segments.max(4);
    let sectors = segments.max(6);
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for ring in 0..=rings {
        let v = ring as f32 / rings as f32;
        let phi = v * std::f32::consts::PI;
        let sin_phi = phi.sin();
        let cos_phi = phi.cos();

        for sector in 0..=sectors {
            let u = sector as f32 / sectors as f32;
            let theta = u * std::f32::consts::TAU;
            let sin_theta = theta.sin();
            let cos_theta = theta.cos();

            let nx = sin_phi * cos_theta;
            let ny = cos_phi;
            let nz = sin_phi * sin_theta;

            vertices.push(Vertex {
                position: [nx * 0.5, ny * 0.5, nz * 0.5],
                normal: [nx, ny, nz],
                uv: [u, v],
            });
        }
    }

    let stride = sectors + 1;
    for ring in 0..rings {
        for sector in 0..sectors {
            let cur = ring * stride + sector;
            let next = cur + stride;
            indices.extend_from_slice(&[cur, next, cur + 1, cur + 1, next, next + 1]);
        }
    }

    upload(device, &vertices, &indices, "uv-sphere")
}

/// Quad unitario en el plano XY (Z=0), normal +Z; para overlays HUD texturizados.
pub fn create_unit_quad_xy(device: &wgpu::Device) -> Mesh {
    #[rustfmt::skip]
    let vertices: Vec<Vertex> = vec![
        Vertex { position: [-0.5, -0.5, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 1.0] },
        Vertex { position: [ 0.5, -0.5, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 1.0] },
        Vertex { position: [ 0.5,  0.5, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 0.0] },
        Vertex { position: [-0.5,  0.5, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 0.0] },
    ];
    let indices: Vec<u32> = vec![0, 1, 2, 2, 3, 0];
    upload(device, &vertices, &indices, "unit-quad-xy")
}

// ---------------------------------------------------------------------------
// Per-instance data for the instanced rendering pipeline
// ---------------------------------------------------------------------------

/// Tooltips/HUD en PNG: UV completas, sin Lambert; alpha = tex.a × flag_pad.y (shader ≥ 2.5).
pub const RENDER_KIND_HUD_OVERLAY: f32 = 3.0;

// ---------------------------------------------------------------------------
/// Data uploaded per draw instance to the GPU.
///
/// Layout (96 bytes):
///   offset  0..64  → model matrix, column-major (4 × vec4<f32>)
///   offset 64..80  → flag_pad  (x = selection flag, yzw = unused)
///   offset 80..96  → uv_rect   [u_min, v_min, u_max, v_max] en el atlas
///
/// Matches WGSL `InstanceInput` locations 3..8.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceData {
    pub model:    [[f32; 4]; 4],
    pub flag_pad: [f32; 4],   // x: estado selección/hover, y: alpha multiplicador
    pub uv_rect:  [f32; 4],   // sub-región del texture atlas [u_min, v_min, u_max, v_max]
}

impl InstanceData {
    const ATTRIBS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        3 => Float32x4,  // model col 0
        4 => Float32x4,  // model col 1
        5 => Float32x4,  // model col 2
        6 => Float32x4,  // model col 3
        7 => Float32x4,  // flag_pad
        8 => Float32x4,  // uv_rect  ← nuevo
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode:    wgpu::VertexStepMode::Instance,
            attributes:   &Self::ATTRIBS,
        }
    }

    pub fn new(model: glam::Mat4, flag: f32, uv_rect: [f32; 4]) -> Self {
        Self {
            model:    model.to_cols_array_2d(),
            flag_pad: [flag, 1.0, 0.0, 0.0],
            uv_rect,
        }
    }
}

// ---------------------------------------------------------------------------
// Loader de archivos .glb / .gltf
// ---------------------------------------------------------------------------
// Helper: sube vértices e índices a la GPU
// ---------------------------------------------------------------------------
pub(crate) fn upload(device: &wgpu::Device, vertices: &[Vertex], indices: &[u32], label: &str) -> Mesh {
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label:    Some(&format!("{label}-vbo")),
        contents: bytemuck::cast_slice(vertices),
        usage:    wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label:    Some(&format!("{label}-ibo")),
        contents: bytemuck::cast_slice(indices),
        usage:    wgpu::BufferUsages::INDEX,
    });
    Mesh { vertex_buffer, index_buffer, index_count: indices.len() as u32 }
}
