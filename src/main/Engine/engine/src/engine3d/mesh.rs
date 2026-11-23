use ash::vk;
use bytemuck::{Pod, Zeroable};

use crate::engine3d::vulkan::{GpuBuffer, SharedGpu};

const VERTEX_USAGE_RAW: vk::BufferUsageFlags = vk::BufferUsageFlags::from_raw(
    vk::BufferUsageFlags::VERTEX_BUFFER.as_raw()
        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS.as_raw()
        | vk::BufferUsageFlags::STORAGE_BUFFER.as_raw()
        | vk::BufferUsageFlags::TRANSFER_DST.as_raw()
        | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR.as_raw(),
);
const INDEX_USAGE_RAW: vk::BufferUsageFlags = vk::BufferUsageFlags::from_raw(
    vk::BufferUsageFlags::INDEX_BUFFER.as_raw()
        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS.as_raw()
        | vk::BufferUsageFlags::STORAGE_BUFFER.as_raw()
        | vk::BufferUsageFlags::TRANSFER_DST.as_raw()
        | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR.as_raw(),
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

    /// Reescribe una malla creada con `upload_dynamic`. Devuelve `false` si los
    /// datos ya no caben en la capacidad reservada (el llamador debe recrearla).
    pub(crate) fn update_dynamic(&mut self, vertices: &[Vertex], indices: &[u32]) -> bool {
        let vertex_bytes = bytemuck::cast_slice(vertices);
        let index_bytes = bytemuck::cast_slice(indices);
        if vertex_bytes.len() as u64 > self.vertex_buffer.size
            || index_bytes.len() as u64 > self.index_buffer.size
        {
            return false;
        }
        if self.vertex_buffer.write(0, vertex_bytes).is_err()
            || self.index_buffer.write(0, index_bytes).is_err()
        {
            return false;
        }
        self.index_count = indices.len() as u32;
        // `clear` + `extend` reutiliza la capacidad: sin esto cada frame de
        // animación asignaría tres `Vec` nuevos para los datos de raytracing.
        self.rt_positions.clear();
        self.rt_positions
            .extend(vertices.iter().map(|vertex| vertex.position));
        self.rt_uvs.clear();
        self.rt_uvs.extend(vertices.iter().map(|vertex| vertex.uv));
        self.rt_indices.clear();
        self.rt_indices.extend_from_slice(indices);
        true
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

/// Cilindro unitario: radio 0.5, de y=-0.5 a y=+0.5.
/// Ocupa la misma caja que `create_cube` para conservar proporciones.
pub fn create_cylinder(gpu: &SharedGpu, segments: u32) -> Mesh {
    let sectors = segments.max(6);
    let tau = std::f32::consts::TAU;
    let r = 0.5_f32;
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // Pared lateral: normal radial, UV envuelto en U.
    for i in 0..sectors {
        let t0 = i as f32 / sectors as f32 * tau;
        let t1 = (i + 1) as f32 / sectors as f32 * tau;
        let base = vertices.len() as u32;
        for (theta, u) in [
            (t0, i as f32 / sectors as f32),
            (t1, (i + 1) as f32 / sectors as f32),
        ] {
            let (sin, cos) = theta.sin_cos();
            vertices.push(Vertex {
                position: [r * cos, -0.5, r * sin],
                normal: [cos, 0.0, sin],
                uv: [u, 0.0],
            });
        }
        for (theta, u) in [
            (t0, i as f32 / sectors as f32),
            (t1, (i + 1) as f32 / sectors as f32),
        ] {
            let (sin, cos) = theta.sin_cos();
            vertices.push(Vertex {
                position: [r * cos, 0.5, r * sin],
                normal: [cos, 0.0, sin],
                uv: [u, 1.0],
            });
        }
        indices.extend_from_slice(&[base, base + 2, base + 3, base, base + 3, base + 1]);
    }

    // Tapas superior (+Y) e inferior (-Y).
    for (y, normal, flip) in [(0.5_f32, 1.0_f32, false), (-0.5_f32, -1.0_f32, true)] {
        let center = vertices.len() as u32;
        vertices.push(Vertex {
            position: [0.0, y, 0.0],
            normal: [0.0, normal, 0.0],
            uv: [0.5, 0.5],
        });
        for i in 0..=sectors {
            let theta = i as f32 / sectors as f32 * tau;
            let (sin, cos) = theta.sin_cos();
            vertices.push(Vertex {
                position: [r * cos, y, r * sin],
                normal: [0.0, normal, 0.0],
                uv: [0.5 + 0.5 * cos, 0.5 + 0.5 * sin],
            });
        }
        for i in 0..sectors {
            if flip {
                indices.extend_from_slice(&[center, center + i + 2, center + i + 1]);
            } else {
                indices.extend_from_slice(&[center, center + i + 1, center + i + 2]);
            }
        }
    }

    upload(gpu, &vertices, &indices, "cylinder", None)
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

/// Terreno generado. El fragmento mezcla hierba, tierra, arcilla, arena, piedra
/// y nieve a partir de la capa base guardada en `tex_layer_pad.w`.
pub const RENDER_KIND_TERRAIN: f32 = 4.0;

// ---------------------------------------------------------------------------
/// Data uploaded per draw instance to the GPU.
///
/// Layout (96 bytes):
///   offset  0..64  → model matrix, column-major (4 × vec4<f32>)
///   offset 64..80  → flag_pad  (x = selection flag, y = alpha, z = render_kind, w = roughness o -1)
///   offset 80..96  → tex_layer_pad  (x = capa en texture_2d_array; y = metallic; z = índice
///                    de reflection probe en el cube array, -1 = sin probe;
///                    en terreno y = tile del atlas de splat)
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

/// Como `upload`, pero con capacidad de sobra para reescribir los buffers cada
/// frame sin crear/destruir memoria Vulkan (lo usa el agua al interpolar ticks).
pub(crate) fn upload_dynamic(
    gpu: &SharedGpu,
    vertices: &[Vertex],
    indices: &[u32],
    label: &str,
) -> Mesh {
    let vertex_capacity = (vertices.len() * 3).max(256);
    let index_capacity = (indices.len() * 3).max(512);
    let vertex_buffer = gpu
        .create_buffer(
            vertex_capacity as u64 * std::mem::size_of::<Vertex>() as u64,
            VERTEX_USAGE_RAW,
            gpu_allocator::MemoryLocation::CpuToGpu,
            &format!("{label}-vbo"),
        )
        .expect("no se pudo crear el buffer dinámico de vértices");
    let index_buffer = gpu
        .create_buffer(
            index_capacity as u64 * std::mem::size_of::<u32>() as u64,
            INDEX_USAGE_RAW,
            gpu_allocator::MemoryLocation::CpuToGpu,
            &format!("{label}-ibo"),
        )
        .expect("no se pudo crear el buffer dinámico de índices");
    let mut mesh = Mesh {
        vertex_buffer,
        index_buffer,
        index_count: 0,
        rt_positions: Vec::new(),
        rt_uvs: Vec::new(),
        rt_indices: Vec::new(),
        rt_index_buffer: None,
    };
    debug_assert!(mesh.update_dynamic(vertices, indices));
    mesh
}
