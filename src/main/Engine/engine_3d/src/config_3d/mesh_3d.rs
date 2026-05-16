// ── Primitivas de malla exclusivas del modo 3D ────────────────────────────────

use std::path::Path;

use crate::mesh::{upload, Mesh, Vertex};

pub(crate) struct LoadedModelMesh {
    pub(crate) mesh: Mesh,
    pub(crate) rgba: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// Despacha por extensión: glTF (`.glb`/`.gltf`) o FBX (`.fbx`).
pub(crate) fn load_model_file(
    device: &wgpu::Device,
    path: &Path,
) -> Result<Vec<LoadedModelMesh>, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "glb" | "gltf" => load_gltf(device, path),
        "fbx" => load_fbx(device, path),
        other => Err(format!(
            "formato no soportado: .{other} (usa .glb, .gltf o .fbx)"
        )),
    }
}

fn white_pixel() -> (Vec<u8>, u32, u32) {
    (vec![255, 255, 255, 255], 1, 1)
}

fn decode_image_bytes(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Some((rgba.into_raw(), w, h))
}

fn load_image_path(path: &Path) -> (Vec<u8>, u32, u32) {
    match image::open(path) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            (rgba.into_raw(), w, h)
        }
        Err(_) => white_pixel(),
    }
}

fn load_gltf(device: &wgpu::Device, path: &Path) -> Result<Vec<LoadedModelMesh>, String> {
    let (doc, buffers, images) = gltf::import(path).map_err(|e| format!("gltf error: {e}"))?;

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
                .map(|((pos, norm), uv)| Vertex {
                    position: pos,
                    normal: norm,
                    uv,
                })
                .collect();

            let (rgba, width, height) = if let Some(img_idx) = primitive
                .material()
                .pbr_metallic_roughness()
                .base_color_texture()
                .map(|info| info.texture().source().index())
            {
                if let Some(img_data) = images.get(img_idx) {
                    use gltf::image::Format;
                    let pixels = match img_data.format {
                        Format::R8G8B8 => img_data
                            .pixels
                            .chunks_exact(3)
                            .flat_map(|p| [p[0], p[1], p[2], 255u8])
                            .collect(),
                        Format::R8G8B8A8 => img_data.pixels.clone(),
                        _ => vec![255, 255, 255, 255],
                    };
                    (pixels, img_data.width, img_data.height)
                } else {
                    white_pixel()
                }
            } else {
                white_pixel()
            };

            meshes.push(LoadedModelMesh {
                mesh: upload(device, &vertices, &indices, "gltf-mesh"),
                rgba,
                width,
                height,
            });
        }
    }

    if meshes.is_empty() {
        return Err("el archivo glTF no contiene mallas".into());
    }

    Ok(meshes)
}

/// Centra la malla en el origen y escala a ~`target_extent` unidades de mundo.
fn center_and_normalize_vertices(vertices: &mut [Vertex], target_extent: f32) {
    if vertices.is_empty() {
        return;
    }
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in vertices.iter() {
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
        }
    }
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let extent = (max[0] - min[0])
        .max(max[1] - min[1])
        .max(max[2] - min[2]);
    let scale = if extent > 1e-5 {
        (target_extent / extent).clamp(0.001, 50.0)
    } else {
        1.0
    };
    for v in vertices.iter_mut() {
        v.position[0] = (v.position[0] - center[0]) * scale;
        v.position[1] = (v.position[1] - center[1]) * scale;
        v.position[2] = (v.position[2] - center[2]) * scale;
    }
}

fn push_fbx_mesh_triangles(mesh: &ufbx::Mesh, vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>) {
    let mut tri_indices = vec![0u32; mesh.max_face_triangles * 3];

    let face_iter: Box<dyn Iterator<Item = u32>> = if !mesh.material_parts.is_empty() {
        Box::new(mesh.material_parts.iter().flat_map(|part| {
            part.face_indices.iter().copied()
        }))
    } else {
        Box::new((0..mesh.faces.len() as u32).collect::<Vec<_>>().into_iter())
    };

    for face_index in face_iter {
        let face = mesh.faces[face_index as usize];
        let num_tris = mesh.triangulate_face(&mut tri_indices, face);
        let corner_count = num_tris as usize * 3;
        for &index in &tri_indices[..corner_count] {
            let ix = index as usize;
            let pos = ufbx::get_vertex_vec3(&mesh.vertex_position, ix);
            let norm = ufbx::get_vertex_vec3(&mesh.vertex_normal, ix);
            let uv = ufbx::get_vertex_vec2(&mesh.vertex_uv, ix);
            vertices.push(Vertex {
                position: [pos.x as f32, pos.y as f32, pos.z as f32],
                normal: [norm.x as f32, norm.y as f32, norm.z as f32],
                uv: [uv.x as f32, uv.y as f32],
            });
            indices.push(vertices.len() as u32 - 1);
        }
    }
}

fn load_fbx(device: &wgpu::Device, path: &Path) -> Result<Vec<LoadedModelMesh>, String> {
    let mut opts = ufbx::LoadOpts::default();
    opts.generate_missing_normals = true;
    opts.load_external_files = true;
    opts.target_axes = ufbx::CoordinateAxes::right_handed_y_up();
    opts.target_unit_meters = 1.0;
    opts.space_conversion = ufbx::SpaceConversion::ModifyGeometry;

    let scene = ufbx::load_file(
        path.to_str()
            .ok_or_else(|| "ruta FBX no válida (UTF-8)".to_string())?,
        opts,
    )
    .map_err(|e| format!("fbx error: {e:?}"))?;

    let fbx_dir = path.parent().unwrap_or_else(|| Path::new("."));

    // Un solo mesh combinado por archivo (evita piezas sueltas en el origen).
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut rgba = white_pixel().0;
    let mut tex_w = 1u32;
    let mut tex_h = 1u32;

    for mesh in scene.meshes.iter() {
        if vertices.is_empty() {
            if let Some(mat) = mesh.materials.first().map(|m| m.as_ref()) {
                if let Some(tex) = texture_from_material(mat) {
                    let (r, w, h) = texture_rgba(tex, fbx_dir);
                    rgba = r;
                    tex_w = w;
                    tex_h = h;
                }
            }
        }
        push_fbx_mesh_triangles(mesh, &mut vertices, &mut indices);
    }

    if vertices.is_empty() {
        return Err("el archivo FBX no contiene mallas".into());
    }

    center_and_normalize_vertices(&mut vertices, 1.8);

    Ok(vec![LoadedModelMesh {
        mesh: upload(device, &vertices, &indices, "fbx-mesh"),
        rgba,
        width: tex_w,
        height: tex_h,
    }])
}

fn texture_from_material<'a>(material: &'a ufbx::Material) -> Option<&'a ufbx::Texture> {
    if material.pbr.base_color.texture_enabled {
        if let Some(tex) = material.pbr.base_color.texture.as_ref() {
            return Some(tex.as_ref());
        }
    }
    if material.fbx.diffuse_color.texture_enabled {
        if let Some(tex) = material.fbx.diffuse_color.texture.as_ref() {
            return Some(tex.as_ref());
        }
    }
    material
        .textures
        .first()
        .map(|entry| entry.texture.as_ref())
}

fn texture_rgba(texture: &ufbx::Texture, fbx_dir: &Path) -> (Vec<u8>, u32, u32) {
    if !texture.content.is_empty() {
        if let Some(decoded) = decode_image_bytes(&texture.content) {
            return decoded;
        }
    }

    let candidates = [
        texture.absolute_filename.as_ref(),
        texture.filename.as_ref(),
        texture.relative_filename.as_ref(),
    ];

    for name in candidates {
        if name.is_empty() {
            continue;
        }
        let path = Path::new(name);
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            fbx_dir.join(path)
        };
        if resolved.is_file() {
            return load_image_path(&resolved);
        }
    }

    white_pixel()
}

pub(crate) fn create_ground_plane(device: &wgpu::Device) -> Mesh {
    const SEGMENTS: u32 = 20;
    const SIZE: f32 = 40.0;
    const UV_SCALE: f32 = 1.0;

    let half = SIZE / 2.0;
    let step = SIZE / SEGMENTS as f32;

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for z in 0..=SEGMENTS {
        for x in 0..=SEGMENTS {
            let px = -half + x as f32 * step;
            let pz = -half + z as f32 * step;
            let u = (x as f32 / SEGMENTS as f32) * UV_SCALE;
            let v = (z as f32 / SEGMENTS as f32) * UV_SCALE;
            vertices.push(Vertex {
                position: [px, 0.0, pz],
                normal: [0.0, 1.0, 0.0],
                uv: [u, v],
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
