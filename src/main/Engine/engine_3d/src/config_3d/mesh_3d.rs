// ── Primitivas de malla exclusivas del modo 3D ────────────────────────────────

use std::path::Path;

use crate::mesh::{upload, Mesh, Vertex};

pub(crate) struct LoadedModelMesh {
    pub(crate) mesh: Mesh,
    pub(crate) rgba: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Forward horizontal en espacio local de la malla (plano XZ), para alinear al jugador FP.
    pub(crate) forward_xz: glam::Vec2,
}

/// Estima hacia dónde "mira" la malla en XZ (tras centrar/normalizar).
/// Personajes simétricos suelen empatar ±X/±Z; si no hay eje dominante claro, se usa +Z (convención glTF).
pub(crate) fn estimate_mesh_forward_xz(vertices: &[Vertex]) -> glam::Vec2 {
    if vertices.is_empty() {
        return glam::Vec2::new(0.0, 1.0);
    }
    let mut pos_z = 0.0f32;
    let mut neg_z = 0.0f32;
    let mut pos_x = 0.0f32;
    let mut neg_x = 0.0f32;
    for v in vertices {
        let p = &v.position;
        if p[2] > 0.0 {
            pos_z += p[2];
        } else {
            neg_z += -p[2];
        }
        if p[0] > 0.0 {
            pos_x += p[0];
        } else {
            neg_x += -p[0];
        }
    }
    let mut weights = [
        pos_z,
        neg_z,
        pos_x,
        neg_x,
    ];
    weights.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let primary = weights[0];
    let secondary = weights[1].max(1e-8);
    if primary < secondary * 1.08 {
        return glam::Vec2::new(0.0, 1.0);
    }
    if pos_z.max(neg_z) >= pos_x.max(neg_x) {
        if pos_z >= neg_z {
            glam::Vec2::new(0.0, 1.0)
        } else {
            glam::Vec2::new(0.0, -1.0)
        }
    } else if pos_x >= neg_x {
        glam::Vec2::new(1.0, 0.0)
    } else {
        glam::Vec2::new(-1.0, 0.0)
    }
}

/// Despacha por extensión: glTF (`.glb`/`.gltf`) o FBX (`.fbx`).
/// Si `normalize_to_extent` es `Some`, centra la malla en el origen y escala al alto indicado.
pub(crate) fn load_model_file(
    device: &wgpu::Device,
    path: &Path,
    normalize_to_extent: Option<f32>,
) -> Result<Vec<LoadedModelMesh>, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "glb" | "gltf" => load_gltf(device, path, normalize_to_extent),
        "fbx" => load_fbx(device, path, normalize_to_extent),
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

struct GltfRawPrim {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

/// Lee una primitiva glTF aplicando la matriz `world` del nodo al que pertenece
/// (importante para respetar la metadata de orientación que muchos exportadores
/// dejan en los nodos, p. ej. la corrección Z-up→Y-up en Blender que mete una
/// rotación 90° en el `Armature`/root).
fn read_gltf_primitive(
    primitive: &gltf::Primitive,
    world: glam::Mat4,
    buffers: &[gltf::buffer::Data],
    images: &[gltf::image::Data],
) -> Result<GltfRawPrim, String> {
    use glam::{Mat3, Vec3};

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

    // Para normales: usar la 3x3 del mundo y normalizar. Para rotación pura o
    // escala uniforme es exacto; con escala no uniforme queda algo distorsionado
    // pero aceptable (alternativa correcta sería inverse-transpose).
    let world3 = Mat3::from_mat4(world);

    let vertices: Vec<Vertex> = positions
        .into_iter()
        .zip(normals)
        .zip(uvs)
        .map(|((pos, norm), uv)| {
            let wp = world.transform_point3(Vec3::from(pos));
            let wn = (world3 * Vec3::from(norm)).normalize_or_zero();
            Vertex {
                position: wp.to_array(),
                normal: wn.to_array(),
                uv,
            }
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

    Ok(GltfRawPrim {
        vertices,
        indices,
        rgba,
        width,
        height,
    })
}

/// Recorre el árbol de nodos acumulando la transformación de mundo. Para cada
/// nodo con mesh, hornea esa matriz en los vértices de sus primitivas.
fn walk_gltf_node(
    node: gltf::Node,
    parent_world: glam::Mat4,
    buffers: &[gltf::buffer::Data],
    images: &[gltf::image::Data],
    out: &mut Vec<GltfRawPrim>,
) -> Result<(), String> {
    use glam::{Mat4, Quat, Vec3};

    let local = match node.transform() {
        gltf::scene::Transform::Matrix { matrix } => Mat4::from_cols_array_2d(&matrix),
        gltf::scene::Transform::Decomposed {
            translation,
            rotation,
            scale,
        } => Mat4::from_scale_rotation_translation(
            Vec3::from(scale),
            Quat::from_array(rotation),
            Vec3::from(translation),
        ),
    };
    let world = parent_world * local;

    if let Some(mesh) = node.mesh() {
        for primitive in mesh.primitives() {
            out.push(read_gltf_primitive(&primitive, world, buffers, images)?);
        }
    }

    for child in node.children() {
        walk_gltf_node(child, world, buffers, images, out)?;
    }
    Ok(())
}

fn load_gltf(
    device: &wgpu::Device,
    path: &Path,
    normalize_to_extent: Option<f32>,
) -> Result<Vec<LoadedModelMesh>, String> {
    use glam::Mat4;

    let (doc, buffers, images) = gltf::import(path).map_err(|e| format!("gltf error: {e}"))?;

    let mut prims: Vec<GltfRawPrim> = Vec::new();

    // Preferir la escena por defecto; si no hay escenas (raro), caer a iterar
    // meshes con matriz identidad para no romper archivos antiguos.
    if let Some(scene) = doc.default_scene().or_else(|| doc.scenes().next()) {
        for root in scene.nodes() {
            walk_gltf_node(root, Mat4::IDENTITY, &buffers, &images, &mut prims)?;
        }
    } else {
        for mesh in doc.meshes() {
            for primitive in mesh.primitives() {
                prims.push(read_gltf_primitive(&primitive, Mat4::IDENTITY, &buffers, &images)?);
            }
        }
    }

    if prims.is_empty() {
        return Err("el archivo glTF no contiene mallas".into());
    }

    if let Some(extent) = normalize_to_extent {
        for p in prims.iter_mut() {
            normalize_vertices_centered_height(&mut p.vertices, extent);
        }
    }

    let mut meshes = Vec::with_capacity(prims.len());
    for p in prims {
        let forward_xz = estimate_mesh_forward_xz(&p.vertices);
        meshes.push(LoadedModelMesh {
            mesh: upload(device, &p.vertices, &p.indices, "gltf-mesh"),
            rgba: p.rgba,
            width: p.width,
            height: p.height,
            forward_xz,
        });
    }
    Ok(meshes)
}

/// Centra la malla en el origen y escala a `target_height` en Y (mismo pivot que el cubo placeholder).
fn normalize_vertices_centered_height(vertices: &mut [Vertex], target_height: f32) {
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
    let height = (max[1] - min[1]).max(1e-5);
    let scale = (target_height / height).clamp(0.001, 50.0);
    let cx = (min[0] + max[0]) * 0.5;
    let cy = (min[1] + max[1]) * 0.5;
    let cz = (min[2] + max[2]) * 0.5;
    for v in vertices.iter_mut() {
        v.position[0] = (v.position[0] - cx) * scale;
        v.position[1] = (v.position[1] - cy) * scale;
        v.position[2] = (v.position[2] - cz) * scale;
    }
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

/// Convierte una `ufbx::Matrix` (3×4 column-major, f64) a `glam::Mat4` afín.
fn ufbx_matrix_to_mat4(m: &ufbx::Matrix) -> glam::Mat4 {
    glam::Mat4::from_cols(
        glam::Vec4::new(m.m00 as f32, m.m10 as f32, m.m20 as f32, 0.0),
        glam::Vec4::new(m.m01 as f32, m.m11 as f32, m.m21 as f32, 0.0),
        glam::Vec4::new(m.m02 as f32, m.m12 as f32, m.m22 as f32, 0.0),
        glam::Vec4::new(m.m03 as f32, m.m13 as f32, m.m23 as f32, 1.0),
    )
}

fn push_fbx_mesh_triangles(
    mesh: &ufbx::Mesh,
    world: glam::Mat4,
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
) {
    use glam::{Mat3, Vec3};

    let mut tri_indices = vec![0u32; mesh.max_face_triangles * 3];
    // Mismo criterio que en glTF: 3×3 del world para normales (correcto con
    // rotación pura o escala uniforme; suficientemente bueno para FBX típicos).
    let world3 = Mat3::from_mat4(world);

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
            let wp = world.transform_point3(Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32));
            let wn = (world3 * Vec3::new(norm.x as f32, norm.y as f32, norm.z as f32))
                .normalize_or_zero();
            vertices.push(Vertex {
                position: wp.to_array(),
                normal: wn.to_array(),
                uv: [uv.x as f32, uv.y as f32],
            });
            indices.push(vertices.len() as u32 - 1);
        }
    }
}

fn load_fbx(
    device: &wgpu::Device,
    path: &Path,
    normalize_to_extent: Option<f32>,
) -> Result<Vec<LoadedModelMesh>, String> {
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
    // Iteramos NODOS (no `scene.meshes` directo) para aplicar la matriz de
    // mundo de cada nodo a la geometría: muchos exportadores dejan la pose
    // correcta (p. ej. corrección Y-up / orientación del Armature) en los
    // nodos, y la geometría local viene "acostada" si la ignoramos.
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut rgba = white_pixel().0;
    let mut tex_w = 1u32;
    let mut tex_h = 1u32;
    let mut texture_picked = false;

    for node in scene.nodes.iter() {
        let Some(mesh_ref) = node.mesh.as_ref() else {
            continue;
        };
        let mesh: &ufbx::Mesh = mesh_ref.as_ref();

        if !texture_picked {
            if let Some(mat) = mesh.materials.first().map(|m| m.as_ref()) {
                if let Some(tex) = texture_from_material(mat) {
                    let (r, w, h) = texture_rgba(tex, fbx_dir);
                    rgba = r;
                    tex_w = w;
                    tex_h = h;
                    texture_picked = true;
                }
            }
        }

        let world = ufbx_matrix_to_mat4(&node.geometry_to_world);
        push_fbx_mesh_triangles(mesh, world, &mut vertices, &mut indices);
    }

    if vertices.is_empty() {
        return Err("el archivo FBX no contiene mallas".into());
    }

    center_and_normalize_vertices(&mut vertices, normalize_to_extent.unwrap_or(1.8));
    let forward_xz = estimate_mesh_forward_xz(&vertices);

    Ok(vec![LoadedModelMesh {
        mesh: upload(device, &vertices, &indices, "fbx-mesh"),
        rgba,
        width: tex_w,
        height: tex_h,
        forward_xz,
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
