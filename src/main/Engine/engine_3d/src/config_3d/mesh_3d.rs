// ── Primitivas de malla exclusivas del modo 3D ────────────────────────────────

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use crate::config_3d::model_asset::{
    empty_rgba_placeholder, shared_white_material_texture, MaterialTextureCpu,
};

use crate::config_3d::model_asset::{self, GltfFile};
use crate::config_3d::skin_diag;
use crate::mesh::{upload, Mesh, Vertex};

pub(crate) struct LoadedModelMesh {
    pub(crate) mesh: Mesh,
    pub(crate) rgba: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Forward horizontal en espacio local de la malla (plano XZ), para alinear al jugador FP.
    pub(crate) forward_xz: glam::Vec2,
    /// AABB en espacio local tras normalizar (para cápsula del jugador FP).
    pub(crate) local_bounds: ([f32; 3], [f32; 3]),
}

pub(crate) fn vertex_local_bounds(vertices: &[Vertex]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in vertices {
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
        }
    }
    (min, max)
}

/// Malla parseada en CPU (sin upload GPU); usada por precarga en segundo plano.
pub(crate) struct CpuModelMeshPart {
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) indices: Vec<u32>,
    pub(crate) material_index: u32,
    /// Textura compartida por material (`Arc`); mips precalculados en import GLB.
    pub(crate) texture: Arc<MaterialTextureCpu>,
    pub(crate) forward_xz: glam::Vec2,
    pub(crate) local_bounds: ([f32; 3], [f32; 3]),
    pub(crate) roughness: f32,
    pub(crate) metallic: f32,
    pub(crate) ior: f32,
}

/// Mip-chain 1024² por material (fuentes sin mips en import).
pub(crate) fn prepare_cpu_parts_textures_for_gpu(parts: &mut [CpuModelMeshPart]) {
    let tex_size = crate::texture::TextureArray::TEXTURE_SIZE;
    let mut shared_by_material: HashMap<u32, Arc<MaterialTextureCpu>> = HashMap::new();
    for part in parts.iter_mut() {
        if part
            .texture
            .layer_mips
            .as_ref()
            .is_some_and(|mips| crate::texture::layer_mip_chain_valid_for_array(mips))
        {
            continue;
        }
        let shared = shared_by_material
            .entry(part.material_index)
            .or_insert_with(|| {
                let chain = crate::texture::build_layer_mip_chain_timed(
                    part.texture.effective_rgba().to_vec(),
                    part.texture.width,
                    part.texture.height,
                );
                Arc::new(MaterialTextureCpu {
                    rgba: empty_rgba_placeholder(),
                    width: tex_size,
                    height: tex_size,
                    layer_mips: Some(Arc::new(chain.mips)),
                })
            });
        part.texture = Arc::clone(shared);
    }
}

/// Opciones de precarga (Recursos vs spawn / jugador).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ModelPreloadOptions {
    /// Variante `::play_character` (~1.7 m); solo personajes.
    pub warm_play_character: bool,
}

impl ModelPreloadOptions {
    pub(crate) fn library(category: Option<&str>) -> Self {
        let is_character = category == Some("character");
        Self {
            warm_play_character: is_character,
        }
    }
}

/// Estima hacia dónde "mira" la malla en XZ (tras centrar/normalizar).
pub(crate) fn estimate_mesh_forward_xz(vertices: &[Vertex]) -> glam::Vec2 {
    if vertices.is_empty() {
        return glam::Vec2::new(0.0, 1.0);
    }
    let positions: Vec<[f32; 3]> = vertices.iter().map(|v| v.position).collect();
    estimate_forward_xz_from_positions(&positions)
}

/// Estima forward en XZ desde posiciones (compartido con `model_asset` torso).
pub(crate) fn estimate_forward_xz_from_positions(positions: &[[f32; 3]]) -> glam::Vec2 {
    if positions.is_empty() {
        return glam::Vec2::new(0.0, 1.0);
    }
    let mut pos_z = 0.0f32;
    let mut neg_z = 0.0f32;
    let mut pos_x = 0.0f32;
    let mut neg_x = 0.0f32;
    for p in positions {
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
    let mut weights = [pos_z, neg_z, pos_x, neg_x];
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

/// Combina forward de metadata del nodo con estimación geométrica (glTF/GLB).
pub(crate) fn resolve_mesh_forward_xz(meta: glam::Vec2, geometry_est: glam::Vec2) -> glam::Vec2 {
    let dot = meta.dot(geometry_est);
    if dot < -0.5 {
        return geometry_est;
    }
    if dot < 0.35 {
        return geometry_est;
    }
    meta
}

fn upright_candidates() -> [glam::Quat; 6] {
    use std::f32::consts::FRAC_PI_2;
    use glam::Quat;
    [
        Quat::IDENTITY,
        Quat::from_rotation_x(-FRAC_PI_2),
        Quat::from_rotation_x(FRAC_PI_2),
        Quat::from_rotation_z(-FRAC_PI_2),
        Quat::from_rotation_z(FRAC_PI_2),
        Quat::from_rotation_y(std::f32::consts::PI),
    ]
}

pub(crate) fn aabb_y_extent_score(min: [f32; 3], max: [f32; 3]) -> f32 {
    let sy = (max[1] - min[1]).max(1e-8);
    let sx = max[0] - min[0];
    let sz = max[2] - min[2];
    sy / sx.max(sz).max(1e-8)
}

fn upright_candidate_score(
    min: [f32; 3],
    max: [f32; 3],
    sample_points: &[glam::Vec3],
    cand: glam::Quat,
) -> f32 {
    let mat = glam::Mat4::from_quat(cand);
    let (tmin, tmax) = transform_aabb_corners_for_play(min, max, mat);
    let y_score = aabb_y_extent_score(tmin, tmax);
    if sample_points.is_empty() {
        return y_score;
    }
    let mid_y = (tmax[1] + tmin[1]) * 0.5;
    let avg_y = sample_points
        .iter()
        .map(|p| mat.transform_point3(*p).y)
        .sum::<f32>()
        / sample_points.len() as f32;
    let feet_bonus = if avg_y < mid_y { 0.1 } else { 0.0 };
    y_score + feet_bonus
}

/// AABB + centroide bajo (pies abajo) para desempatar ±90° en Mixamo/GLB.
pub(crate) fn upright_quat_from_vertices_bounds(
    min: [f32; 3],
    max: [f32; 3],
    sample_points: &[glam::Vec3],
) -> glam::Quat {
    let mut best = glam::Quat::IDENTITY;
    let mut best_score = upright_candidate_score(min, max, sample_points, best);
    for cand in upright_candidates() {
        let score = upright_candidate_score(min, max, sample_points, cand);
        if score > best_score + 1e-4 {
            best_score = score;
            best = cand;
        }
    }
    best
}

/// Forward en XZ desde el eje −Z del nodo (convención glTF).
pub(crate) fn forward_xz_from_node_world(world: glam::Mat4) -> glam::Vec2 {
    use glam::Vec3;
    let dir = world.transform_vector3(-Vec3::Z);
    let xz = glam::Vec2::new(dir.x, dir.z);
    if xz.length_squared() < 1e-8 {
        glam::Vec2::new(0.0, 1.0)
    } else {
        xz.normalize()
    }
}

pub(crate) fn transform_aabb_corners_for_play(
    min: [f32; 3],
    max: [f32; 3],
    mat: glam::Mat4,
) -> ([f32; 3], [f32; 3]) {
    use glam::Vec3;
    let corners = [
        Vec3::new(min[0], min[1], min[2]),
        Vec3::new(max[0], min[1], min[2]),
        Vec3::new(min[0], max[1], min[2]),
        Vec3::new(max[0], max[1], min[2]),
        Vec3::new(min[0], min[1], max[2]),
        Vec3::new(max[0], min[1], max[2]),
        Vec3::new(min[0], max[1], max[2]),
        Vec3::new(max[0], max[1], max[2]),
    ];
    let mut out_min = [f32::MAX; 3];
    let mut out_max = [f32::MIN; 3];
    for c in corners {
        let p = mat.transform_point3(c);
        for i in 0..3 {
            out_min[i] = out_min[i].min(p[i]);
            out_max[i] = out_max[i].max(p[i]);
        }
    }
    (out_min, out_max)
}

/// Despacha por extensión: glTF (`.glb`/`.gltf`).
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
        "glb" | "gltf" => {
            if normalize_to_extent.is_some() {
                let file = crate::config_3d::model_asset::import_gltf(path)?;
                load_gltf_preview_from_file(device, file.as_ref(), normalize_to_extent.unwrap())
            } else {
                load_gltf(device, path, normalize_to_extent)
            }
        }
        "fbx" => super::mesh_3d_fbx::load_fbx(device, path, normalize_to_extent),
        other => Err(format!(
            "formato no soportado: .{other} (usa .glb, .gltf o .fbx)"
        )),
    }
}

/// Precarga en hilo: un solo `gltf::import` para malla estática en editor.
pub(crate) fn preload_model_cpu_bundle(
    path: &Path,
    options: ModelPreloadOptions,
) -> Result<
    (
        Vec<CpuModelMeshPart>,
        Option<Arc<model_asset::ModelAsset>>,
        Option<Vec<CpuModelMeshPart>>,
    ),
    String,
> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "glb" | "gltf" => {
            let file = model_asset::import_gltf(path)?;
            let parts = load_gltf_cpu_from_file(file.as_ref(), None)?;
            let play_parts = if options.warm_play_character {
                Some(load_gltf_cpu_from_file(
                    file.as_ref(),
                    Some(crate::config_3d::character_anchor::PLAY_CHARACTER_BODY_HEIGHT),
                )?)
            } else {
                None
            };
            let anim_asset = if model_asset::gltf_needs_model_asset(file.as_ref()) {
                let label = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("model");
                match model_asset::load_model_asset_from_gltf(file.as_ref(), None) {
                    Some(asset) => Some(asset),
                    None => {
                        skin_diag::log_skinned_unavailable(
                            label,
                            "load_model_asset_from_gltf devolvió None",
                        );
                        None
                    }
                }
            } else {
                None
            };
            Ok((parts, anim_asset, play_parts))
        }
        "fbx" => {
            let parts = super::mesh_3d_fbx::load_fbx_cpu(path, None)?;
            let play_parts = if options.warm_play_character {
                Some(super::mesh_3d_fbx::load_fbx_cpu(
                    path,
                    Some(crate::config_3d::character_anchor::PLAY_CHARACTER_BODY_HEIGHT),
                )?)
            } else {
                None
            };
            let anim_asset = model_asset::load_model_asset(path, None);
            Ok((parts, anim_asset, play_parts))
        }
        other => Err(format!(
            "formato no soportado: .{other} (usa .glb, .gltf o .fbx)"
        )),
    }
}

fn white_pixel() -> (Vec<u8>, u32, u32) {
    (vec![255, 255, 255, 255], 1, 1)
}

struct GltfRawPrim {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    material_index: u32,
    texture: Arc<MaterialTextureCpu>,
    roughness: f32,
    metallic: f32,
    ior: f32,
}

/// Lee una primitiva glTF aplicando la matriz `world` del nodo al que pertenece
/// (importante para respetar la metadata de orientación que muchos exportadores
/// dejan en los nodos, p. ej. la corrección Z-up→Y-up en Blender que mete una
/// rotación 90° en el `Armature`/root).
fn decode_gltf_material_albedo(
    mat_idx: usize,
    material_albedos: &HashMap<usize, gltf::image::Data>,
) -> (Vec<u8>, u32, u32) {
    if let Some(img) = material_albedos.get(&mat_idx) {
        crate::config_3d::model_asset::gltf_image_data_to_rgba(img)
    } else {
        white_pixel()
    }
}

fn read_gltf_primitive(
    primitive: &gltf::Primitive,
    world: glam::Mat4,
    buffers: &[gltf::buffer::Data],
    material_textures: &HashMap<u32, Arc<MaterialTextureCpu>>,
    material_albedos: &HashMap<usize, gltf::image::Data>,
    albedo_cache: &mut HashMap<usize, Arc<MaterialTextureCpu>>,
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

    let material = primitive.material();
    let mat_idx = material.index().unwrap_or(0);
    let pbr = material.pbr_metallic_roughness();
    let roughness = pbr.roughness_factor();
    let metallic = pbr.metallic_factor();
    let ior = if metallic > 0.5 { 0.0 } else { 1.5 };

    let texture = resolve_gltf_primitive_texture(
        mat_idx,
        material_textures,
        material_albedos,
        albedo_cache,
    );

    Ok(GltfRawPrim {
        vertices,
        indices,
        material_index: mat_idx as u32,
        texture,
        roughness,
        metallic,
        ior,
    })
}

fn resolve_gltf_primitive_texture(
    mat_idx: usize,
    material_textures: &HashMap<u32, Arc<MaterialTextureCpu>>,
    material_albedos: &HashMap<usize, gltf::image::Data>,
    albedo_cache: &mut HashMap<usize, Arc<MaterialTextureCpu>>,
) -> Arc<MaterialTextureCpu> {
    let mat_key = mat_idx as u32;
    if let Some(tex) = material_textures.get(&mat_key) {
        return Arc::clone(tex);
    }
    if let Some(cached) = albedo_cache.get(&mat_idx) {
        return Arc::clone(cached);
    }
    let (rgba_vec, width, height) = decode_gltf_material_albedo(mat_idx, material_albedos);
    if width == 1 && height == 1 && rgba_vec.as_slice() == [255, 255, 255, 255] {
        let white = shared_white_material_texture();
        albedo_cache.insert(mat_idx, Arc::clone(&white));
        return white;
    }
    let arc = Arc::new(MaterialTextureCpu {
        rgba: Arc::from(rgba_vec.into_boxed_slice()),
        width,
        height,
        layer_mips: None,
    });
    albedo_cache.insert(mat_idx, Arc::clone(&arc));
    arc
}

/// Recorre el árbol de nodos acumulando la transformación de mundo. Para cada
/// nodo con mesh, hornea esa matriz en los vértices de sus primitivas.
fn walk_gltf_node(
    node: gltf::Node,
    parent_world: glam::Mat4,
    buffers: &[gltf::buffer::Data],
    material_textures: &HashMap<u32, Arc<MaterialTextureCpu>>,
    material_albedos: &HashMap<usize, gltf::image::Data>,
    albedo_cache: &mut HashMap<usize, Arc<MaterialTextureCpu>>,
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
            out.push(read_gltf_primitive(
                &primitive,
                world,
                buffers,
                material_textures,
                material_albedos,
                albedo_cache,
            )?);
        }
    }

    for child in node.children() {
        walk_gltf_node(
            child,
            world,
            buffers,
            material_textures,
            material_albedos,
            albedo_cache,
            out,
        )?;
    }
    Ok(())
}

/// Una sola malla (nodo principal) para sustituir jugador FP — evita subir toda la escena a GPU.
pub(crate) fn load_gltf_preview_from_file(
    device: &wgpu::Device,
    file: &crate::config_3d::model_asset::GltfFile,
    normalize_to_extent: f32,
) -> Result<Vec<LoadedModelMesh>, String> {
    let node_ix = crate::config_3d::model_asset::gltf_primary_mesh_node_index(file)
        .ok_or_else(|| "el archivo glTF no contiene mallas".to_string())?;
    let scene = file
        .doc
        .default_scene()
        .or_else(|| file.doc.scenes().next())
        .ok_or_else(|| "glTF sin escena".to_string())?;
    let scene_parents = crate::config_3d::model_asset::build_gltf_node_parents(&scene);
    let world =
        crate::config_3d::model_asset::world_matrix_for_gltf_node_index(
            &file.doc,
            &scene_parents,
            node_ix,
        );
    let node = file
        .doc
        .nodes()
        .nth(node_ix)
        .ok_or_else(|| "nodo glTF inválido".to_string())?;
    let mesh = node
        .mesh()
        .ok_or_else(|| "nodo sin mesh".to_string())?;

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut rgba = white_pixel().0;
    let mut tex_w = 1u32;
    let mut tex_h = 1u32;

    let mut albedo_cache: HashMap<usize, Arc<MaterialTextureCpu>> = HashMap::new();
    for primitive in mesh.primitives() {
        let prim = read_gltf_primitive(
            &primitive,
            world,
            &file.buffers,
            &file.material_textures,
            &file.material_smallest_albedos,
            &mut albedo_cache,
        )?;
        let base = vertices.len() as u32;
        vertices.extend(prim.vertices);
        indices.extend(prim.indices.iter().map(|i| i + base));
        if tex_w <= 1 && prim.texture.width > 1 {
            rgba = prim.texture.effective_rgba().to_vec();
            tex_w = prim.texture.width;
            tex_h = prim.texture.height;
        }
    }

    if vertices.is_empty() {
        return Err("el archivo glTF no contiene geometría".into());
    }

    let (min, max) = vertex_bounds(&vertices);
    let sample: Vec<glam::Vec3> = vertices
        .iter()
        .map(|v| glam::Vec3::from_array(v.position))
        .collect();
    let upright = upright_quat_from_vertices_bounds(min, max, &sample);
    apply_quat_to_vertices(&mut vertices, upright);
    // Jugador FP: pies en Y=0 (Godot CharacterBody3D + cápsula).
    normalize_vertices_height_feet_pivot(&mut vertices, normalize_to_extent);
    recenter_vertices_to_local_feet(&mut vertices);
    let meta_fwd = forward_xz_from_node_world(world);
    let est_fwd = estimate_mesh_forward_xz(&vertices);
    let forward_xz = resolve_mesh_forward_xz(meta_fwd, est_fwd);

    let local_bounds = vertex_local_bounds(&vertices);

    Ok(vec![LoadedModelMesh {
        mesh: upload(device, &vertices, &indices, "gltf-preview", None),
        rgba,
        width: tex_w,
        height: tex_h,
        forward_xz,
        local_bounds,
    }])
}

fn load_gltf(
    device: &wgpu::Device,
    path: &Path,
    normalize_to_extent: Option<f32>,
) -> Result<Vec<LoadedModelMesh>, String> {
    use glam::Mat4;

    let file = model_asset::import_gltf(path)?;
    let doc = &file.doc;
    let buffers = &file.buffers;
    let material_albedos = &file.material_smallest_albedos;
    let material_textures = &file.material_textures;

    let mut prims: Vec<GltfRawPrim> = Vec::new();
    let mut albedo_cache: HashMap<usize, Arc<MaterialTextureCpu>> = HashMap::new();

    // Preferir la escena por defecto; si no hay escenas (raro), caer a iterar
    // meshes con matriz identidad para no romper archivos antiguos.
    if let Some(scene) = doc.default_scene().or_else(|| doc.scenes().next()) {
        for root in scene.nodes() {
            walk_gltf_node(
                root,
                Mat4::IDENTITY,
                buffers,
                material_textures,
                material_albedos,
                &mut albedo_cache,
                &mut prims,
            )?;
        }
    } else {
        for mesh in doc.meshes() {
            for primitive in mesh.primitives() {
                prims.push(read_gltf_primitive(
                    &primitive,
                    Mat4::IDENTITY,
                    buffers,
                    material_textures,
                    material_albedos,
                    &mut albedo_cache,
                )?);
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
        let local_bounds = vertex_local_bounds(&p.vertices);
        meshes.push(LoadedModelMesh {
            mesh: upload(device, &p.vertices, &p.indices, "gltf-mesh", None),
            rgba: p.texture.effective_rgba().to_vec(),
            width: p.texture.width,
            height: p.texture.height,
            forward_xz,
            local_bounds,
        });
    }
    Ok(meshes)
}

pub(crate) fn load_gltf_cpu_from_file(
    file: &GltfFile,
    normalize_to_extent: Option<f32>,
) -> Result<Vec<CpuModelMeshPart>, String> {
    use glam::Mat4;

    let doc = &file.doc;
    let buffers = &file.buffers;
    let material_albedos = &file.material_smallest_albedos;
    let material_textures = &file.material_textures;

    let mut prims: Vec<GltfRawPrim> = Vec::new();
    let mut albedo_cache: HashMap<usize, Arc<MaterialTextureCpu>> = HashMap::new();

    if let Some(scene) = doc.default_scene().or_else(|| doc.scenes().next()) {
        for root in scene.nodes() {
            walk_gltf_node(
                root,
                Mat4::IDENTITY,
                buffers,
                material_textures,
                material_albedos,
                &mut albedo_cache,
                &mut prims,
            )?;
        }
    } else {
        for mesh in doc.meshes() {
            for primitive in mesh.primitives() {
                prims.push(read_gltf_primitive(
                    &primitive,
                    Mat4::IDENTITY,
                    buffers,
                    material_textures,
                    material_albedos,
                    &mut albedo_cache,
                )?);
            }
        }
    }

    if prims.is_empty() {
        return Err("el archivo glTF no contiene mallas".into());
    }

    if let Some(extent) = normalize_to_extent {
        for p in prims.iter_mut() {
            normalize_vertices_height_feet_pivot(&mut p.vertices, extent);
            recenter_vertices_to_local_feet(&mut p.vertices);
        }
    }

    Ok(prims
        .into_iter()
        .map(|p| CpuModelMeshPart {
            forward_xz: estimate_mesh_forward_xz(&p.vertices),
            local_bounds: vertex_local_bounds(&p.vertices),
            vertices: p.vertices,
            indices: p.indices,
            material_index: p.material_index,
            texture: p.texture,
            roughness: p.roughness,
            metallic: p.metallic,
            ior: p.ior,
        })
        .collect())
}

fn vertex_bounds(vertices: &[Vertex]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in vertices {
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
        }
    }
    (min, max)
}

fn apply_quat_to_vertices(vertices: &mut [Vertex], q: glam::Quat) {
    use glam::{Mat3, Vec3};
    let m = Mat3::from_quat(q);
    for v in vertices.iter_mut() {
        let p = Vec3::from_array(v.position);
        let n = Vec3::from_array(v.normal);
        v.position = (m * p).to_array();
        let nn = (m * n).normalize_or_zero();
        if nn.length_squared() > 1e-8 {
            v.normal = nn.to_array();
        }
    }
}

/// Escala por altura Y y deja los pies en Y=0; centra solo en X/Z (colocación en suelo).
pub(crate) fn normalize_vertices_height_feet_pivot(vertices: &mut [Vertex], target_height: f32) {
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
    let min_y = min[1];
    let cz = (min[2] + max[2]) * 0.5;
    for v in vertices.iter_mut() {
        v.position[0] = (v.position[0] - cx) * scale;
        v.position[1] = (v.position[1] - min_y) * scale;
        v.position[2] = (v.position[2] - cz) * scale;
    }
}

/// Centra la malla en el origen y escala a `target_height` en Y (mismo pivot que el cubo placeholder).
/// Asegura pies en Y=0 y centro XZ en origen (pivote = `Transform.position`).
pub(crate) fn recenter_vertices_to_local_feet(vertices: &mut [Vertex]) {
    if vertices.is_empty() {
        return;
    }
    let (min, max) = vertex_local_bounds(vertices);
    let shift = glam::Vec3::new(
        (min[0] + max[0]) * 0.5,
        min[1],
        (min[2] + max[2]) * 0.5,
    );
    if shift.length_squared() < 1e-8 {
        return;
    }
    for v in vertices.iter_mut() {
        let p = glam::Vec3::from_array(v.position) - shift;
        v.position = p.to_array();
    }
}

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

/// Extensión del plano en X/Z en espacio local (antes de `Transform.scale`).
pub(crate) const GROUND_PLANE_MESH_EXTENT: f32 = 40.0;

/// Baldosas de 8px en la textura checker 128×128 → 16 cuadros por unidad UV.
pub(crate) const GROUND_CHECKER_TILES_PER_UV: f32 = 16.0;

/// UV del suelo para que cada baldosín del checker mida `cell_size` metros en mundo
/// (tras `sync_ground_plane_to_world_bounds`: escala = límites / `GROUND_PLANE_MESH_EXTENT`).
pub(crate) fn create_ground_plane(
    device: &wgpu::Device,
    world_width: f32,
    world_depth: f32,
    cell_size: f32,
) -> Mesh {
    const SEGMENTS: u32 = 20;
    const SIZE: f32 = GROUND_PLANE_MESH_EXTENT;

    let half = SIZE / 2.0;
    let step = SIZE / SEGMENTS as f32;
    let cell = cell_size.max(0.05);
    let tiles = GROUND_CHECKER_TILES_PER_UV;

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for z in 0..=SEGMENTS {
        for x in 0..=SEGMENTS {
            let px = -half + x as f32 * step;
            let pz = -half + z as f32 * step;
            let u = (px + half) / SIZE * world_width / cell / tiles;
            let v = (pz + half) / SIZE * world_depth / cell / tiles;
            vertices.push(Vertex {
                position: [px, 0.0, pz],
                normal: [0.0, 1.0, 0.0],
                uv: [u, v],
            });
        }
    }

    let stride = SEGMENTS + 1;
    let mut rt_indices: Vec<u32> = Vec::new();
    for z in 0..SEGMENTS {
        for x in 0..SEGMENTS {
            let tl = z * stride + x;
            let tr = tl + 1;
            let bl = tl + stride;
            let br = bl + 1;
            indices.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
            // Cara opuesta: visible desde arriba (FPS) y desde abajo sin depender del cull global.
            indices.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
            // RT: solo cara superior (normal +Y) para evitar hits duplicados.
            rt_indices.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
        }
    }

    upload(device, &vertices, &indices, "ground-plane", Some(&rt_indices))
}

/// Disco en XZ (radio local = `GROUND_PLANE_MESH_EXTENT / 2`); escalar uniforme al radio del mundo.
pub(crate) fn create_ground_disk(
    device: &wgpu::Device,
    world_radius: f32,
    cell_size: f32,
) -> Mesh {
    const SEGMENTS: u32 = 96;
    let local_radius = GROUND_PLANE_MESH_EXTENT * 0.5;
    let cell = cell_size.max(0.05);
    let tiles = GROUND_CHECKER_TILES_PER_UV;
    let diameter = world_radius.max(0.1) * 2.0;
    let size = GROUND_PLANE_MESH_EXTENT;

    let uv_at = |px: f32, pz: f32| -> [f32; 2] {
        let u = (px + local_radius) / size * diameter / cell / tiles;
        let v = (pz + local_radius) / size * diameter / cell / tiles;
        [u, v]
    };

    let mut vertices: Vec<Vertex> = Vec::with_capacity((SEGMENTS + 2) as usize);
    let mut indices: Vec<u32> = Vec::new();
    let mut rt_indices: Vec<u32> = Vec::new();

    vertices.push(Vertex {
        position: [0.0, 0.0, 0.0],
        normal: [0.0, 1.0, 0.0],
        uv: uv_at(0.0, 0.0),
    });

    for i in 0..=SEGMENTS {
        let a = (i as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
        let px = a.cos() * local_radius;
        let pz = a.sin() * local_radius;
        vertices.push(Vertex {
            position: [px, 0.0, pz],
            normal: [0.0, 1.0, 0.0],
            uv: uv_at(px, pz),
        });
    }

    for i in 0..SEGMENTS {
        let i0 = 1 + i;
        let i1 = 1 + i + 1;
        indices.extend_from_slice(&[0, i0, i1]);
        indices.extend_from_slice(&[0, i1, i0]);
        rt_indices.extend_from_slice(&[0, i0, i1]);
    }

    upload(device, &vertices, &indices, "ground-disk", Some(&rt_indices))
}
