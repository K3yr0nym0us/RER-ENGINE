// ── Primitivas de malla exclusivas del modo 3D ────────────────────────────────
//
// Contiene:
//  · GltfMesh          — resultado de parsear un .glb/.gltf
//  · load_glb          — carga un .glb/.gltf desde disco y lo sube a la GPU
//  · create_ground_plane — plano subdividido 40×40 u en XZ (escena por defecto)

use std::path::Path;

use crate::mesh::{upload, Mesh, Vertex};

// ---------------------------------------------------------------------------
// Resultado de carga de un archivo .glb / .gltf
// ---------------------------------------------------------------------------
pub(crate) struct GltfMesh {
    pub(crate) mesh:      Mesh,
    /// Índice en el Vec<gltf::image::Data> devuelto por gltf::import.
    pub(crate) tex_index: Option<usize>,
}

// ---------------------------------------------------------------------------
// Loader .glb / .gltf
// ---------------------------------------------------------------------------
pub(crate) fn load_glb(
    device: &wgpu::Device,
    path:   &Path,
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

            let tex_index = primitive
                .material()
                .pbr_metallic_roughness()
                .base_color_texture()
                .map(|info| info.texture().source().index());

            meshes.push(GltfMesh {
                mesh: upload(device, &vertices, &indices, "glb-mesh"),
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
// Plano de suelo procedural — escenario base 3D (primera persona)
//
// Genera una malla subdividida de SIZE × SIZE unidades centrada en el origen,
// orientada en el plano XZ (normal +Y). Las UVs se tilan con UV_SCALE.
// ---------------------------------------------------------------------------
pub(crate) fn create_ground_plane(device: &wgpu::Device) -> Mesh {
    const SEGMENTS: u32 = 20;
    const SIZE:     f32 = 40.0;
    const UV_SCALE: f32 = 20.0;

    let half = SIZE / 2.0;
    let step = SIZE / SEGMENTS as f32;

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices:  Vec<u32>    = Vec::new();

    for z in 0..=SEGMENTS {
        for x in 0..=SEGMENTS {
            let px = -half + x as f32 * step;
            let pz = -half + z as f32 * step;
            let u  = (x as f32 / SEGMENTS as f32) * UV_SCALE;
            let v  = (z as f32 / SEGMENTS as f32) * UV_SCALE;
            vertices.push(Vertex {
                position: [px, 0.0, pz],
                normal:   [0.0, 1.0, 0.0],
                uv:       [u, v],
            });
        }
    }

    let stride = SEGMENTS + 1;
    for z in 0..SEGMENTS {
        for x in 0..SEGMENTS {
            let tl = z * stride + x;
            let tr = tl + 1;
            let bl = tl + stride;
            let br = bl + 1;
            indices.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
        }
    }

    upload(device, &vertices, &indices, "ground-plane")
}
