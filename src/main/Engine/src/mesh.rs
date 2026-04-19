use std::path::Path;

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

// ---------------------------------------------------------------------------
// Loader de archivos .glb / .gltf
// Devuelve (meshes, textures_data) donde cada mesh tiene un índice opcional
// de textura base color en el Vec de imágenes.
// ---------------------------------------------------------------------------
pub struct GltfMesh {
    pub mesh:      Mesh,
    /// Índice en el Vec<gltf::image::Data> devuelto por gltf::import
    pub tex_index: Option<usize>,
}

pub fn load_glb(
    device:  &wgpu::Device,
    path:    &Path,
) -> Result<(Vec<GltfMesh>, Vec<gltf::image::Data>), String> {
    let (doc, buffers, images) =
        gltf::import(path).map_err(|e| format!("gltf error: {e}"))?;

    let mut meshes = Vec::new();

    for mesh in doc.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buf| Some(&buffers[buf.index()]));

            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .ok_or_else(|| "primitiva sin posiciones".to_string())?
                .collect();

            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|n| n.collect())
                .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|tc| tc.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

            let indices: Vec<u32> = reader
                .read_indices()
                .map(|i| i.into_u32().collect())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());

            let vertices: Vec<Vertex> = positions
                .into_iter()
                .zip(normals)
                .zip(uvs)
                .map(|((pos, norm), uv)| Vertex { position: pos, normal: norm, uv })
                .collect();

            // Extraer índice de la textura base color del material
            let tex_index = primitive
                .material()
                .pbr_metallic_roughness()
                .base_color_texture()
                .map(|info| info.texture().source().index());

            meshes.push(GltfMesh {
                mesh:  upload(device, &vertices, &indices, "glb-mesh"),
                tex_index,
            });
        }
    }

    if meshes.is_empty() {
        return Err("el archivo .glb no contiene mallas".into());
    }

    Ok((meshes, images))
}

// ---------------------------------------------------------------------------
// Helper: sube vértices e índices a la GPU
// ---------------------------------------------------------------------------
fn upload(device: &wgpu::Device, vertices: &[Vertex], indices: &[u32], label: &str) -> Mesh {
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
