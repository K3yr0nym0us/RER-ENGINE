use ash::vk;
use bytemuck::{Pod, Zeroable};

use crate::vulkan::{GpuBuffer, SharedGpu};

/// `0x0008_0000` es `VK_BUFFER_USAGE_ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_BIT_KHR`.
const AS_INPUT: vk::BufferUsageFlags = vk::BufferUsageFlags::from_raw(0x0008_0000);
const VERTEX_USAGE_RAW: vk::BufferUsageFlags = vk::BufferUsageFlags::from_raw(
    vk::BufferUsageFlags::VERTEX_BUFFER.as_raw()
        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS.as_raw()
        | vk::BufferUsageFlags::STORAGE_BUFFER.as_raw()
        | vk::BufferUsageFlags::TRANSFER_DST.as_raw()
        | AS_INPUT.as_raw(),
);
const INDEX_USAGE_RAW: vk::BufferUsageFlags = vk::BufferUsageFlags::from_raw(
    vk::BufferUsageFlags::INDEX_BUFFER.as_raw()
        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS.as_raw()
        | vk::BufferUsageFlags::STORAGE_BUFFER.as_raw()
        | vk::BufferUsageFlags::TRANSFER_DST.as_raw()
        | AS_INPUT.as_raw(),
);

// ---------------------------------------------------------------------------
// Vértice — layout debe coincidir con el shader (location 0,1,2)
// ---------------------------------------------------------------------------
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

// ---------------------------------------------------------------------------
// Vértice skinned — layout espejado de `shaders/raster/skinned.vert` (locations 0..4)
// ---------------------------------------------------------------------------
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SkinnedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub joints: [u32; 4],
    pub weights: [f32; 4],
}

// ---------------------------------------------------------------------------
// Mesh en GPU
// ---------------------------------------------------------------------------
pub struct Mesh {
    pub vertex_buffer: GpuBuffer,
    pub index_buffer: GpuBuffer,
    pub index_count: u32,
    /// Posiciones locales + índices para BLAS/BVH (RT v2).
    pub rt_positions: Vec<[f32; 3]>,
    pub rt_uvs: Vec<[f32; 2]>,
    pub rt_indices: Vec<u32>,
    /// Índices RT en GPU cuando difieren del draw (p. ej. suelo single-sided).
    pub rt_index_buffer: Option<GpuBuffer>,
}

impl Mesh {
    pub fn rt_index_count(&self) -> u32 {
        self.rt_indices.len() as u32
    }

    pub fn rt_index_buffer(&self) -> &GpuBuffer {
        self.rt_index_buffer.as_ref().unwrap_or(&self.index_buffer)
    }
}

// ---------------------------------------------------------------------------
// Cubo procedural — siempre disponible sin necesitar un archivo externo
// ---------------------------------------------------------------------------
pub fn create_cube(gpu: &SharedGpu) -> Mesh {
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

    upload(gpu, &vertices, &indices, "cube", None)
}

/// Esfera UV unitaria (radio 0.5) para el icono del sol.
pub fn create_uv_sphere(gpu: &SharedGpu, segments: u32) -> Mesh {
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

    upload(gpu, &vertices, &indices, "uv-sphere", None)
}

/// Quad en XY (normal +Z), doble cara; muros/triggers del editor (sin grosor visible).
pub fn create_unit_wall_quad_xy(gpu: &SharedGpu) -> Mesh {
    #[rustfmt::skip]
    let vertices: Vec<Vertex> = vec![
        Vertex { position: [-0.5, -0.5, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 1.0] },
        Vertex { position: [ 0.5, -0.5, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 1.0] },
        Vertex { position: [ 0.5,  0.5, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 0.0] },
        Vertex { position: [-0.5,  0.5, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 0.0] },
    ];
    #[rustfmt::skip]
    let indices: Vec<u32> = vec![
        0, 1, 2, 2, 3, 0,
        0, 2, 1, 2, 0, 3,
    ];
    upload(gpu, &vertices, &indices, "unit-wall-quad-xy", None)
}

/// Quad unitario en el plano XY (Z=0), normal +Z; para overlays HUD texturizados.
pub fn create_unit_quad_xy(gpu: &SharedGpu) -> Mesh {
    #[rustfmt::skip]
    let vertices: Vec<Vertex> = vec![
        Vertex { position: [-0.5, -0.5, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 1.0] },
        Vertex { position: [ 0.5, -0.5, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 1.0] },
        Vertex { position: [ 0.5,  0.5, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 0.0] },
        Vertex { position: [-0.5,  0.5, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 0.0] },
    ];
    let indices: Vec<u32> = vec![0, 1, 2, 2, 3, 0];
    upload(gpu, &vertices, &indices, "unit-quad-xy", None)
}

// ---------------------------------------------------------------------------
// Per-instance data for the instanced rendering pipeline
// ---------------------------------------------------------------------------

/// Rama legacy de la ruta `texture_2d_array` del pipeline forward. PNG de UI en pantalla: solo
/// `screen_hud_image`.
#[allow(dead_code)]
pub const RENDER_KIND_HUD_OVERLAY: f32 = 3.0;

// ---------------------------------------------------------------------------
/// Data uploaded per draw instance to the GPU.
///
/// Layout (96 bytes):
///   offset  0..64  → model matrix, column-major (4 × vec4<f32>)
///   offset 64..80  → flag_pad  (x = selection flag, y = alpha, z = render_kind, w = roughness o -1)
///   offset 80..96  → tex_layer_pad  (x = capa en texture_2d_array; y = metallic; z = índice
///                    de reflection probe en el cube array, -1 = sin probe)
///
/// Coincide con las locations 3..8 de `InstanceInput` en `shaders/raster/forward.vert`.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceData {
    pub model: [[f32; 4]; 4],
    pub flag_pad: [f32; 4], // x: selección/hover, y: alpha, z: render_kind, w: roughness (-1 = default)
    pub tex_layer_pad: [f32; 4], // x: capa del array; y: metallic; o UV rect en HUD
}

impl InstanceData {
    pub fn new(model: glam::Mat4, flag: f32, tex_layer: u32) -> Self {
        Self {
            model: model.to_cols_array_2d(),
            flag_pad: [flag, 1.0, 0.0, -1.0],
            // z = índice de reflection probe (-1 = sin probe → entorno procedural).
            tex_layer_pad: [tex_layer as f32, 0.0, -1.0, 0.0],
        }
    }
}

/// Misma disposición que `InstanceData`; locations 5..10 para el pipeline skinned.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SkinnedInstanceData {
    pub model: [[f32; 4]; 4],
    pub flag_pad: [f32; 4],
    pub tex_layer_pad: [f32; 4],
}

impl SkinnedInstanceData {
    pub fn from_instance(inst: &InstanceData) -> Self {
        Self {
            model: inst.model,
            flag_pad: inst.flag_pad,
            tex_layer_pad: inst.tex_layer_pad,
        }
    }
}

// ---------------------------------------------------------------------------
// Loader de archivos .glb / .gltf
// ---------------------------------------------------------------------------
// Helper: sube vértices e índices a la GPU
// ---------------------------------------------------------------------------
pub struct SkinnedMesh {
    pub vertex_buffer: GpuBuffer,
    pub index_buffer: GpuBuffer,
    pub index_count: u32,
}

pub(crate) fn upload_skinned(
    gpu: &SharedGpu,
    vertices: &[SkinnedVertex],
    indices: &[u32],
    label: &str,
) -> SkinnedMesh {
    let vertex_buffer = gpu
        .create_buffer_with_data(
            bytemuck::cast_slice(vertices),
            VERTEX_USAGE_RAW,
            gpu_allocator::MemoryLocation::CpuToGpu,
            &format!("{label}-skinned-vbo"),
        )
        .expect("no se pudo crear el buffer de vértices skinned");
    let index_buffer = gpu
        .create_buffer_with_data(
            bytemuck::cast_slice(indices),
            INDEX_USAGE_RAW,
            gpu_allocator::MemoryLocation::CpuToGpu,
            &format!("{label}-skinned-ibo"),
        )
        .expect("no se pudo crear el buffer de índices skinned");
    SkinnedMesh {
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
    }
}

pub(crate) fn upload(
    gpu: &SharedGpu,
    vertices: &[Vertex],
    indices: &[u32],
    label: &str,
    rt_indices: Option<&[u32]>,
) -> Mesh {
    let rt_indices_vec: Vec<u32> = rt_indices
        .map(|s| s.to_vec())
        .unwrap_or_else(|| indices.to_vec());
    let rt_positions: Vec<[f32; 3]> = vertices.iter().map(|v| v.position).collect();
    let rt_uvs: Vec<[f32; 2]> = vertices.iter().map(|v| v.uv).collect();
    let vertex_buffer = gpu
        .create_buffer_with_data(
            bytemuck::cast_slice(vertices),
            VERTEX_USAGE_RAW,
            gpu_allocator::MemoryLocation::CpuToGpu,
            &format!("{label}-vbo"),
        )
        .expect("no se pudo crear el buffer de vértices");
    let index_buffer = gpu
        .create_buffer_with_data(
            bytemuck::cast_slice(indices),
            INDEX_USAGE_RAW,
            gpu_allocator::MemoryLocation::CpuToGpu,
            &format!("{label}-ibo"),
        )
        .expect("no se pudo crear el buffer de índices");
    let rt_index_buffer = if rt_indices.is_some() && rt_indices_vec.len() != indices.len() {
        Some(
            gpu.create_buffer_with_data(
                bytemuck::cast_slice(&rt_indices_vec),
                INDEX_USAGE_RAW,
                gpu_allocator::MemoryLocation::CpuToGpu,
                &format!("{label}-rt-ibo"),
            )
            .expect("no se pudo crear el buffer de índices RT"),
        )
    } else {
        None
    };
    Mesh {
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
        rt_positions,
        rt_uvs,
        rt_indices: rt_indices_vec,
        rt_index_buffer,
    }
}
