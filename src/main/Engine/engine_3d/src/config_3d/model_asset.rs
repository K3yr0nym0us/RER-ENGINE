//! Parseo de skeleton, malla skinned y clips de animación (paso aparte de `mesh_3d`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use rayon::prelude::*;

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::config_3d::skin_diag;
use crate::mesh::SkinnedVertex;

pub const MAX_JOINTS: usize = 512;

#[derive(Clone, Debug)]
pub struct ModelClipInfo {
    pub name: String,
    pub duration_s: f32,
    pub fps: f32,
}

#[derive(Clone, Debug)]
pub enum AnimProperty {
    Translation,
    Rotation,
    Scale,
}

#[derive(Clone, Debug)]
pub struct AnimKeyframe {
    pub time: f32,
    pub translation: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>, // xyzw
    pub scale: Option<[f32; 3]>,
}

#[derive(Clone, Debug)]
pub struct AnimChannel {
    pub joint_index: usize,
    pub property: AnimProperty,
    pub keyframes: Vec<AnimKeyframe>,
}

#[derive(Clone, Debug)]
pub struct AnimationClip {
    pub name: String,
    pub duration_s: f32,
    pub fps: f32,
    pub channels: Vec<AnimChannel>,
}

#[derive(Clone, Debug)]
pub struct SkinnedMeshData {
    pub vertices: Vec<SkinnedVertex>,
    pub indices: Vec<u32>,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Una pieza de malla skinned (glTF suele traer varias: cuerpo, ropa, accesorios).
#[derive(Clone, Debug)]
pub struct SkinnedMeshPart {
    /// Nombre del nodo glTF (p. ej. Body, Hair) para filtrar forward del jugador.
    pub name: String,
    /// Índice de material glTF para textura por pieza.
    pub material_index: u32,
    pub mesh: SkinnedMeshData,
    /// Mundo del nodo de malla en bind (referencia / IBM fallback).
    pub mesh_bind_world: Mat4,
    /// `geometry_to_bone` por hueso para esta pieza (identidad si el hueso no influye aquí).
    pub inverse_bind: Vec<[[f32; 4]; 4]>,
}

#[derive(Clone, Debug)]
pub struct ModelAsset {
    #[allow(dead_code)]
    pub path: String,
    pub joint_parents: Vec<Option<usize>>,
    #[allow(dead_code)]
    pub inverse_bind: Vec<[[f32; 4]; 4]>,
    /// Pose local de bind por joint (base antes de mezclar clips).
    pub bind_local: Vec<Mat4>,
    /// Mundo del padre de escena fuera del skin (p. ej. Armature).
    pub joint_prefix_world: Vec<Mat4>,
    pub joint_names: Vec<String>,
    pub parts: Vec<SkinnedMeshPart>,
    pub clips: Vec<AnimationClip>,
    /// Centrado/escala aplicados a la malla skinned (misma convención que `mesh_3d`).
    pub mesh_normalize: Mat4,
    /// Forward estimado en XZ (metadata de nodo + geometría).
    pub facing_forward_xz: glam::Vec2,
    /// Índice de nodo de escena por joint (`skin.joints`).
    pub joint_gltf_nodes: Vec<usize>,
    /// glTF: padre en la escena y local de bind (cadena hasta raíz, estilo Godot/Unreal).
    pub gltf_scene_parents: HashMap<usize, usize>,
    pub gltf_bind_node_local: HashMap<usize, Mat4>,
    /// Bind pose en IBM (nodos con TRS≈id); globals = inverse(IBM), no jerarquía de nodos.
    pub bind_pose_from_ibm: bool,
}

/// Textura CPU compartida por material (RGBA + mips precalculados una sola vez).
#[derive(Clone)]
pub struct MaterialTextureCpu {
    /// Solo se usa cuando `layer_mips` es `None`. Con mips precalculados, leer `effective_rgba()`.
    pub rgba: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
    pub layer_mips: Option<Arc<Vec<Vec<u8>>>>,
}

impl MaterialTextureCpu {
    /// Layer base listo para GPU / preview. Evita duplicar el buffer 4K cuando hay mips.
    pub(crate) fn effective_rgba(&self) -> &[u8] {
        self.layer_mips
            .as_ref()
            .and_then(|mips| mips.first())
            .map(Vec::as_slice)
            .unwrap_or(&self.rgba)
    }
}

pub(crate) fn empty_rgba_placeholder() -> Arc<[u8]> {
    static EMPTY: OnceLock<Arc<[u8]>> = OnceLock::new();
    EMPTY
        .get_or_init(|| Arc::from(&[][..]))
        .clone()
}

pub(crate) fn shared_white_material_texture() -> Arc<MaterialTextureCpu> {
    static WHITE: OnceLock<Arc<MaterialTextureCpu>> = OnceLock::new();
    WHITE
        .get_or_init(|| {
            Arc::new(MaterialTextureCpu {
                rgba: Arc::from([255u8, 255, 255, 255].as_slice()),
                width: 1,
                height: 1,
                layer_mips: None,
            })
        })
        .clone()
}

/// Decodifica albedos por material y genera mip chain 1024² una vez (modo editor).
pub(crate) fn build_material_textures_with_mips(
    doc: &gltf::Document,
    albedos: &HashMap<usize, gltf::image::Data>,
    model_path: Option<&str>,
) -> HashMap<u32, Arc<MaterialTextureCpu>> {
    let tex_size = crate::texture::TextureArray::TEXTURE_SIZE;
    let mut material_indices: Vec<usize> = albedos.keys().copied().collect();
    material_indices.sort_unstable();

    let jobs: Vec<(usize, String)> = material_indices
        .iter()
        .map(|&mi| {
            let label = doc
                .materials()
                .nth(mi)
                .and_then(|m| m.name())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("sin nombre")
                .to_string();
            (mi, label)
        })
        .collect();

    let built: Vec<(u32, Arc<MaterialTextureCpu>, u64, u64)> = jobs
        .par_iter()
        .map(|(mi, label)| {
            let img = &albedos[mi];
            let (rgba_vec, w, h) = gltf_image_data_to_rgba(img);
            let chain = crate::texture::build_layer_mip_chain_timed(rgba_vec, w, h);
            if let Some(path) = model_path {
                log::debug!(
                    "[model_tex] {path} Material {mi} ({label}) -> resize: {} ms | mips: {} ms \
                     ({}x{} -> {tex_size}²)",
                    chain.resize_ms,
                    chain.mip_ms,
                    w,
                    h
                );
            }
            let tex = Arc::new(MaterialTextureCpu {
                rgba: empty_rgba_placeholder(),
                width: tex_size,
                height: tex_size,
                layer_mips: Some(Arc::new(chain.mips)),
            });
            (
                *mi as u32,
                tex,
                chain.resize_ms,
                chain.mip_ms,
            )
        })
        .collect();

    let mut total_resize_ms = 0u64;
    let mut total_mip_ms = 0u64;
    let mut out = HashMap::with_capacity(built.len());
    for (mi, tex, resize_ms, mip_ms) in built {
        total_resize_ms += resize_ms;
        total_mip_ms += mip_ms;
        out.insert(mi, tex);
    }
    if let Some(path) = model_path {
        log::debug!(
            "[model_tex] {path} resize+mip: {} textura/s | resize {total_resize_ms} ms | mip {total_mip_ms} ms",
            out.len()
        );
    }
    out
}

fn import_gltf_texture_layers(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    base: Option<&Path>,
    model_path: &str,
) -> (
    HashMap<usize, gltf::image::Data>,
    HashMap<u32, Arc<MaterialTextureCpu>>,
) {
    let material_smallest_albedos =
        crate::config_3d::gltf_texture_load::import_material_smallest_albedos_profiled(
            doc,
            buffers,
            base,
            Some(model_path),
        );
    let material_textures = build_material_textures_with_mips(
        doc,
        &material_smallest_albedos,
        Some(model_path),
    );
    (material_smallest_albedos, material_textures)
}

/// glTF/GLB ya parseado (evita repetir `gltf::import` en la misma carga).
pub struct GltfFile {
    pub path: String,
    pub doc: gltf::Document,
    pub buffers: Vec<gltf::buffer::Data>,
    /// Variante embebida más pequeña por índice de material.
    pub material_smallest_albedos: HashMap<usize, gltf::image::Data>,
    /// RGBA + mips compartidos por `material_index` (precalculados en import).
    pub material_textures: HashMap<u32, Arc<MaterialTextureCpu>>,
}

fn gltf_import_cache_key(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| PathBuf::from(path))
        .display()
        .to_string()
}

fn gltf_import_cache() -> &'static Mutex<HashMap<String, Arc<GltfFile>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<GltfFile>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Invalida el parseo glTF en memoria (p. ej. tras reemplazar el archivo en disco).
pub fn invalidate_gltf_import_cache(path: &Path) {
    let key = gltf_import_cache_key(path);
    if let Ok(mut cache) = gltf_import_cache().lock() {
        cache.remove(&key);
    }
}

fn import_gltf_uncached(path: &Path) -> Result<GltfFile, String> {
    let gltf = gltf::Gltf::open(path).map_err(|e| format!("gltf error: {e}"))?;
    let base = path.parent();
    let doc = gltf.document;
    let blob = gltf.blob;
    let buffers = gltf::import_buffers(&doc, base, blob)
        .map_err(|e| format!("error importando buffers glTF: {e}"))?;
    let model_path = path.display().to_string();
    let (material_smallest_albedos, material_textures) =
        import_gltf_texture_layers(&doc, &buffers, base, &model_path);
    Ok(GltfFile {
        path: model_path,
        doc,
        buffers,
        material_smallest_albedos,
        material_textures,
    })
}

/// Importa glTF/GLB una sola vez por ruta canónica (decode + resize + mips compartidos).
pub fn import_gltf(path: &Path) -> Result<Arc<GltfFile>, String> {
    let key = gltf_import_cache_key(path);
    if let Ok(cache) = gltf_import_cache().lock() {
        if let Some(hit) = cache.get(&key) {
            let label = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&key);
            log::debug!("[GLTF_IMPORT] {label} cache=hit");
            return Ok(Arc::clone(hit));
        }
    }
    let label = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&key);
    log::debug!("[GLTF_IMPORT] {label} cache=cold");
    let file = Arc::new(import_gltf_uncached(path)?);
    if let Ok(mut cache) = gltf_import_cache().lock() {
        cache.insert(key, Arc::clone(&file));
    }
    Ok(file)
}

/// Convierte `gltf::image::Data` a RGBA8 en CPU (malla / skinned).
pub fn gltf_image_data_to_rgba(img: &gltf::image::Data) -> (Vec<u8>, u32, u32) {
    use gltf::image::Format;
    let pixels = match img.format {
        Format::R8G8B8 => img
            .pixels
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255u8])
            .collect(),
        Format::R8G8B8A8 => img.pixels.clone(),
        _ => vec![255, 255, 255, 255],
    };
    (pixels, img.width, img.height)
}

pub use super::model_asset_fbx::fbx_skinned_play_forward_xz;

/// Props estáticos (p. ej. rocas) no necesitan `ModelAsset` (skinning/animación).
pub fn gltf_needs_model_asset(file: &GltfFile) -> bool {
    file.doc.skins().next().is_some() || file.doc.animations().next().is_some()
}

/// Metadatos de clips glTF/GLB sin cargar malla skinned.
pub fn list_gltf_clip_infos(path: &Path) -> Vec<ModelClipInfo> {
    let Ok(file) = import_gltf(path) else {
        return Vec::new();
    };
    list_gltf_clip_infos_from_file(file.as_ref())
}

pub fn list_gltf_clip_infos_from_file(file: &GltfFile) -> Vec<ModelClipInfo> {
    let doc = &file.doc;
    let buffers = &file.buffers;

    let mut out = Vec::new();
    for anim in doc.animations() {
        let name = anim
            .name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Animation_{}", anim.index()));
        let mut max_t = 0.0f32;
        for channel in anim.channels() {
            let reader = channel.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()));
            if let Some(inputs) = reader.read_inputs() {
                for t in inputs {
                    max_t = max_t.max(t);
                }
            }
        }
        out.push(ModelClipInfo {
            name,
            duration_s: max_t.max(1.0 / 30.0),
            fps: 30.0,
        });
    }
    out
}

/// Lista clips embebidos según extensión (FBX o glTF/GLB).
pub fn list_model_clip_infos(path: &Path) -> Vec<ModelClipInfo> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "fbx" => super::model_asset_fbx::list_fbx_clip_infos(path),
        "glb" | "gltf" => list_gltf_clip_infos(path),
        _ => Vec::new(),
    }
}

pub fn load_model_asset(path: &Path, normalize_to_extent: Option<f32>) -> Option<Arc<ModelAsset>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "glb" | "gltf" => import_gltf(path)
            .ok()
            .and_then(|file| load_gltf_asset_from_file(file.as_ref(), normalize_to_extent)),
        "fbx" => super::model_asset_fbx::load_fbx_asset(path, normalize_to_extent),
        _ => None,
    }
}

pub fn load_model_asset_from_gltf(
    file: &GltfFile,
    normalize_to_extent: Option<f32>,
) -> Option<Arc<ModelAsset>> {
    load_gltf_asset_from_file(file, normalize_to_extent)
}

/// Global de un joint glTF recorriendo la escena (Godot / Khronos: `globalTransform(joint)`).
pub(crate) fn gltf_scene_global_for_joint(
    joint_node: usize,
    joint_locals: &[Mat4],
    joint_gltf_nodes: &[usize],
    scene_parents: &HashMap<usize, usize>,
    bind_node_local: &HashMap<usize, Mat4>,
) -> Mat4 {
    let node_to_joint: HashMap<usize, usize> = joint_gltf_nodes
        .iter()
        .enumerate()
        .map(|(ji, &ni)| (ni, ji))
        .collect();
    let mut chain = vec![joint_node];
    let mut cur = joint_node;
    let mut guard = 0usize;
    while let Some(&parent) = scene_parents.get(&cur) {
        guard += 1;
        if guard > 512 {
            break;
        }
        chain.push(parent);
        cur = parent;
    }
    chain.reverse();
    let mut world = Mat4::IDENTITY;
    for idx in chain {
        let local = if let Some(&ji) = node_to_joint.get(&idx) {
            joint_locals.get(ji).copied().unwrap_or(Mat4::IDENTITY)
        } else {
            bind_node_local
                .get(&idx)
                .copied()
                .unwrap_or(Mat4::IDENTITY)
        };
        world *= local;
    }
    world
}

pub(crate) fn compute_gltf_joint_worlds(
    joint_gltf_nodes: &[usize],
    joint_locals: &[Mat4],
    scene_parents: &HashMap<usize, usize>,
    bind_node_local: &HashMap<usize, Mat4>,
) -> Vec<Mat4> {
    joint_gltf_nodes
        .iter()
        .map(|&node_ix| {
            gltf_scene_global_for_joint(
                node_ix,
                joint_locals,
                joint_gltf_nodes,
                scene_parents,
                bind_node_local,
            )
        })
        .collect()
}

fn gltf_skin_joint_node_indices(skin: &gltf::Skin) -> Vec<usize> {
    skin.joints().map(|n| n.index()).collect()
}

/// Origen de un hueso en el esqueleto unificado (útil para diagnosticar anchors rígidos).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum GltfJointOrigin {
    SkinJoint,
    SkeletonRoot,
    SceneAncestor,
    AnimNode,
}

impl GltfJointOrigin {
    fn label(self) -> &'static str {
        match self {
            Self::SkinJoint => "skin_joint",
            Self::SkeletonRoot => "skeleton_root",
            Self::SceneAncestor => "scene_ancestor",
            Self::AnimNode => "anim_node",
        }
    }
}

/// Esqueleto unificado: varios `skin` en un GLB comparten nodos o usan listas distintas (Godot los fusiona).
struct GltfUnifiedSkeleton {
    node_to_unified: HashMap<usize, usize>,
    joint_gltf_nodes: Vec<usize>,
    joint_names: Vec<String>,
    joint_origins: Vec<GltfJointOrigin>,
    skin_count: usize,
    /// Nodos únicos en todos los skins (sin truncar).
    raw_unique_joints: usize,
    /// Nodos que no cupieron por MAX_JOINTS.
    joints_truncated: usize,
}

/// Nodo con malla skinned y el `skin` glTF a usar (propio o heredado del padre).
struct SkinnedMeshNode {
    node_index: usize,
    skin_index: usize,
    inherited_skin: bool,
}

fn build_gltf_unified_skeleton(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    skinned_nodes: &[SkinnedMeshNode],
) -> Option<GltfUnifiedSkeleton> {
    let mut skin_stats: HashMap<usize, (usize, Vec<usize>)> = HashMap::new();
    for entry in skinned_nodes {
        let skin = doc.skins().nth(entry.skin_index)?;
        let joint_nodes = gltf_skin_joint_node_indices(&skin);
        let stat = skin_stats
            .entry(entry.skin_index)
            .or_insert_with(|| (0, joint_nodes));
        stat.0 += gltf_skinned_vertex_count(doc, buffers, entry.node_index);
    }
    if skin_stats.is_empty() {
        return None;
    }

    let mut skins_ordered: Vec<(usize, usize, Vec<usize>)> = skin_stats
        .into_iter()
        .map(|(idx, (verts, joints))| (idx, verts, joints))
        .collect();
    skins_ordered.sort_by(|a, b| {
        b.1
            .cmp(&a.1)
            .then(b.2.len().cmp(&a.2.len()))
    });

    let mut ordered_unique: Vec<usize> = Vec::new();
    let mut seen: HashSet<usize> = HashSet::new();
    for (_skin_idx, _verts, joint_nodes) in &skins_ordered {
        for &node_ix in joint_nodes {
            if seen.insert(node_ix) {
                ordered_unique.push(node_ix);
            }
        }
    }
    let raw_unique_joints = ordered_unique.len();
    let joints_truncated = raw_unique_joints.saturating_sub(MAX_JOINTS);

    let mut node_to_unified: HashMap<usize, usize> = HashMap::new();
    let mut joint_gltf_nodes: Vec<usize> = Vec::new();
    let mut joint_names: Vec<String> = Vec::new();
    let mut joint_origins: Vec<GltfJointOrigin> = Vec::new();

    for &node_ix in ordered_unique.iter().take(MAX_JOINTS) {
        let ui = joint_gltf_nodes.len();
        node_to_unified.insert(node_ix, ui);
        joint_gltf_nodes.push(node_ix);
        let name = doc
            .nodes()
            .nth(node_ix)
            .and_then(|n| n.name())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("joint_{ui}"));
        joint_names.push(name);
        joint_origins.push(GltfJointOrigin::SkinJoint);
    }

    if joint_gltf_nodes.is_empty() {
        return None;
    }
    if joints_truncated > 0 {
        log::warn!(
            "[model_asset] glTF: esqueleto truncado {raw_unique_joints} → {MAX_JOINTS} huesos ({joints_truncated} omitidos)"
        );
    }

    Some(GltfUnifiedSkeleton {
        node_to_unified,
        joint_gltf_nodes,
        joint_names,
        joint_origins,
        skin_count: skins_ordered.len(),
        raw_unique_joints,
        joints_truncated,
    })
}

fn remap_gltf_vertex_joints_to_unified(
    mesh: &mut SkinnedMeshData,
    skin: &gltf::Skin,
    node_to_unified: &HashMap<usize, usize>,
    drops: &mut skin_diag::RemapDropCollector,
) {
    let skin_joint_nodes: Vec<usize> = gltf_skin_joint_node_indices(skin);
    for (vi, v) in mesh.vertices.iter_mut().enumerate() {
        let mut joints = v.joints;
        let mut weights = v.weights;
        for slot in 0..4 {
            if weights[slot] <= 1e-6 {
                continue;
            }
            let si = v.joints[slot] as usize;
            let Some(&node_ix) = skin_joint_nodes.get(si) else {
                drops.record(
                    vi,
                    slot,
                    v.joints[slot],
                    None,
                    weights[slot],
                    skin_diag::RemapDropReason::SkinJointIndexOob,
                );
                weights[slot] = 0.0;
                joints[slot] = 0;
                continue;
            };
            let Some(ui) = node_to_unified.get(&node_ix).copied() else {
                drops.record(
                    vi,
                    slot,
                    v.joints[slot],
                    Some(node_ix),
                    weights[slot],
                    skin_diag::RemapDropReason::NotInUnifiedSkeleton,
                );
                weights[slot] = 0.0;
                joints[slot] = 0;
                continue;
            };
            joints[slot] = ui as u32;
        }
        let sum: f32 = weights.iter().sum();
        if sum > 1e-6 {
            for w in weights.iter_mut() {
                *w /= sum;
            }
        } else {
            weights = [1.0, 0.0, 0.0, 0.0];
            joints = [0, 0, 0, 0];
        }
        v.joints = joints;
        v.weights = weights;
    }
}

fn part_inverse_bind_for_gltf_skin(
    skin: &gltf::Skin,
    buffers: &[gltf::buffer::Data],
    node_to_unified: &HashMap<usize, usize>,
    joint_count: usize,
    _mesh_bind_world: Mat4,
    doc: &gltf::Document,
    scene_parents: &HashMap<usize, usize>,
) -> (Vec<[[f32; 4]; 4]>, u32) {
    let mut ibm = vec![[[0.0; 4]; 4]; joint_count];
    let skin_joint_nodes = gltf_skin_joint_node_indices(skin);
    let skin_reader = skin.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()));
    let mut ibm_fallback = 0u32;
    if let Some(iter) = skin_reader.read_inverse_bind_matrices() {
        for (si, m) in iter.enumerate() {
            let Some(&node_ix) = skin_joint_nodes.get(si) else {
                continue;
            };
            let Some(&ui) = node_to_unified.get(&node_ix) else {
                continue;
            };
            ibm[ui] = m;
        }
    }
    for &node_ix in &skin_joint_nodes {
        let Some(&ui) = node_to_unified.get(&node_ix) else {
            continue;
        };
        if ibm[ui] == [[0.0; 4]; 4] {
            let joint_world =
                world_matrix_for_gltf_node_index(doc, scene_parents, node_ix);
            ibm[ui] = joint_world.inverse().to_cols_array_2d();
            ibm_fallback += 1;
        }
    }
    (ibm, ibm_fallback)
}

/// Nodos que anima el GLB pero no están en `skin.joints` (p. ej. raíz Armature).
fn extend_gltf_unified_with_anim_nodes(
    doc: &gltf::Document,
    unified: &mut GltfUnifiedSkeleton,
) {
    for anim in doc.animations() {
        for channel in anim.channels() {
            let node_ix = channel.target().node().index();
            if unified.node_to_unified.contains_key(&node_ix) {
                continue;
            }
            if unified.joint_gltf_nodes.len() >= MAX_JOINTS {
                return;
            }
            let ui = unified.joint_gltf_nodes.len();
            unified.node_to_unified.insert(node_ix, ui);
            unified.joint_gltf_nodes.push(node_ix);
            let name = doc
                .nodes()
                .nth(node_ix)
                .and_then(|n| n.name())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("anim_node_{ui}"));
            unified.joint_names.push(name);
            unified.joint_origins.push(GltfJointOrigin::AnimNode);
        }
    }
}
fn extend_gltf_unified_with_scene_ancestors(
    doc: &gltf::Document,
    node_parents: &HashMap<usize, usize>,
    unified: &mut GltfUnifiedSkeleton,
) -> usize {
    let mut to_add: Vec<usize> = Vec::new();
    for &joint_node in &unified.joint_gltf_nodes {
        let mut cur = joint_node;
        for _ in 0..512 {
            let Some(&parent) = node_parents.get(&cur) else {
                break;
            };
            if !unified.node_to_unified.contains_key(&parent) {
                to_add.push(parent);
            }
            cur = parent;
        }
    }
    to_add.sort_unstable();
    to_add.dedup();
    let mut added = 0usize;
    for node_ix in to_add {
        if unified.joint_gltf_nodes.len() >= MAX_JOINTS {
            break;
        }
        if unified.node_to_unified.contains_key(&node_ix) {
            continue;
        }
        let ui = unified.joint_gltf_nodes.len();
        unified.node_to_unified.insert(node_ix, ui);
        unified.joint_gltf_nodes.push(node_ix);
        let name = doc
            .nodes()
            .nth(node_ix)
            .and_then(|n| n.name())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("ancestor_{ui}"));
        unified.joint_names.push(name);
        unified.joint_origins.push(GltfJointOrigin::SceneAncestor);
        added += 1;
    }
    added
}

fn extend_gltf_unified_with_skin_skeleton_roots(
    doc: &gltf::Document,
    unified: &mut GltfUnifiedSkeleton,
) -> usize {
    let mut added = 0usize;
    for skin in doc.skins() {
        let Some(skeleton) = skin.skeleton() else {
            continue;
        };
        let node_ix = skeleton.index();
        if unified.node_to_unified.contains_key(&node_ix) {
            continue;
        }
        if unified.joint_gltf_nodes.len() >= MAX_JOINTS {
            break;
        }
        let ui = unified.joint_gltf_nodes.len();
        unified.node_to_unified.insert(node_ix, ui);
        unified.joint_gltf_nodes.push(node_ix);
        let name = doc
            .nodes()
            .nth(node_ix)
            .and_then(|n| n.name())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("skeleton_root_{ui}"));
        unified.joint_names.push(name);
        unified.joint_origins.push(GltfJointOrigin::SkeletonRoot);
        added += 1;
    }
    added
}

fn collect_gltf_scene_chain_nodes(
    joint_gltf_nodes: &[usize],
    scene_parents: &HashMap<usize, usize>,
) -> HashSet<usize> {
    let mut nodes = HashSet::new();
    for &joint_node in joint_gltf_nodes {
        let mut cur = joint_node;
        let mut guard = 0usize;
        nodes.insert(cur);
        while let Some(&parent) = scene_parents.get(&cur) {
            guard += 1;
            if guard > 512 {
                break;
            }
            nodes.insert(parent);
            cur = parent;
        }
    }
    nodes
}

/// Índice del nodo con mesh más grande (preview / sustituto de malla estática).
pub(crate) fn gltf_primary_mesh_node_index(file: &GltfFile) -> Option<usize> {
    let scene = file.doc.default_scene().or_else(|| file.doc.scenes().next())?;
    let mut mesh_nodes: Vec<usize> = Vec::new();
    for root in scene.nodes() {
        collect_gltf_mesh_node_indices(root, &mut mesh_nodes);
    }
    mesh_nodes
        .into_iter()
        .max_by_key(|&idx| gltf_mesh_vertex_count(&file.doc, &file.buffers, idx))
        .filter(|&idx| gltf_mesh_vertex_count(&file.doc, &file.buffers, idx) > 0)
}

fn collect_gltf_mesh_node_indices(node: gltf::Node, out: &mut Vec<usize>) {
    if node.mesh().is_some() {
        out.push(node.index());
    }
    for child in node.children() {
        collect_gltf_mesh_node_indices(child, out);
    }
}

fn gltf_mesh_vertex_count(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    node_index: usize,
) -> usize {
    let node = match doc.nodes().nth(node_index) {
        Some(n) => n,
        None => return 0,
    };
    let mesh = match node.mesh() {
        Some(m) => m,
        None => return 0,
    };
    mesh.primitives()
        .filter_map(|prim| {
            prim.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()))
                .read_positions()
                .map(|p| p.count())
        })
        .sum()
}

pub(crate) fn build_gltf_node_parents(scene: &gltf::Scene) -> HashMap<usize, usize> {
    let mut parents = HashMap::new();
    fn walk(node: gltf::Node, parent: Option<usize>, out: &mut HashMap<usize, usize>) {
        let idx = node.index();
        if let Some(p) = parent {
            out.insert(idx, p);
        }
        for child in node.children() {
            walk(child, Some(idx), out);
        }
    }
    for root in scene.nodes() {
        walk(root, None, &mut parents);
    }
    parents
}

/// Padres de **todos** los nodos del GLB (no solo los alcanzables desde `scene`).
/// Los `skin.joints` pueden colgar fuera del árbol de la escena por defecto.
pub(crate) fn build_gltf_all_node_parents(doc: &gltf::Document) -> HashMap<usize, usize> {
    let mut parents = HashMap::new();
    for node in doc.nodes() {
        let idx = node.index();
        for child in node.children() {
            parents.insert(child.index(), idx);
        }
    }
    parents
}

pub(crate) fn world_matrix_for_gltf_node_index(
    doc: &gltf::Document,
    node_parents: &HashMap<usize, usize>,
    target: usize,
) -> Mat4 {
    let mut chain = vec![target];
    let mut cur = target;
    let mut guard = 0usize;
    while let Some(&parent) = node_parents.get(&cur) {
        guard += 1;
        if guard > 512 {
            break;
        }
        chain.push(parent);
        cur = parent;
    }
    chain.reverse();
    let mut world = Mat4::IDENTITY;
    for idx in chain {
        if let Some(n) = doc.nodes().nth(idx) {
            world *= node_local_matrix(&n);
        }
    }
    world
}

pub(crate) fn node_local_matrix(node: &gltf::Node) -> Mat4 {
    match node.transform() {
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
    }
}

/// Transform crudo del nodo glTF tal como lo expone la crate (Matrix vs Decomposed).
#[derive(Clone, Copy, Debug)]
pub(crate) struct GltfNodeTransformRaw {
    pub storage:           &'static str,
    pub translation:       [f32; 3],
    pub rotation:          [f32; 4],
    pub scale:             [f32; 3],
    pub matrix_cols:       [[f32; 4]; 4],
    pub trs_is_identity:   bool,
    pub matrix_is_identity: bool,
}

fn vec3_is_zero(v: [f32; 3], eps: f32) -> bool {
    v.iter().all(|c| c.abs() < eps)
}

fn quat_is_identity(q: [f32; 4], eps: f32) -> bool {
    (q[0].abs() < eps && q[1].abs() < eps && q[2].abs() < eps && (q[3] - 1.0).abs() < eps)
        || (q[0].abs() < eps && q[1].abs() < eps && q[2].abs() < eps && (q[3] + 1.0).abs() < eps)
}

pub(crate) fn mat4_is_identity(m: Mat4, eps: f32) -> bool {
    m.to_cols_array()
        .iter()
        .zip(Mat4::IDENTITY.to_cols_array())
        .all(|(a, b)| (a - b).abs() < eps)
}

pub(crate) fn gltf_node_transform_raw(node: &gltf::Node) -> GltfNodeTransformRaw {
    const EPS: f32 = 1e-5;
    match node.transform() {
        gltf::scene::Transform::Matrix { matrix } => {
            let m = Mat4::from_cols_array_2d(&matrix);
            let (s, r, t) = m.to_scale_rotation_translation();
            let translation = t.to_array();
            let rotation = r.to_array();
            let scale = s.to_array();
            GltfNodeTransformRaw {
                storage: "Matrix",
                translation,
                rotation,
                scale,
                matrix_cols: matrix,
                trs_is_identity: vec3_is_zero(translation, EPS)
                    && quat_is_identity(rotation, EPS)
                    && scale.iter().all(|c| (c - 1.0).abs() < EPS),
                matrix_is_identity: mat4_is_identity(m, EPS),
            }
        }
        gltf::scene::Transform::Decomposed {
            translation,
            rotation,
            scale,
        } => {
            let translation = Vec3::from(translation).to_array();
            let rotation = rotation;
            let scale = Vec3::from(scale).to_array();
            let m = Mat4::from_scale_rotation_translation(
                Vec3::from_array(scale),
                Quat::from_array(rotation),
                Vec3::from_array(translation),
            );
            GltfNodeTransformRaw {
                storage: "Decomposed",
                translation,
                rotation,
                scale,
                matrix_cols: m.to_cols_array_2d(),
                trs_is_identity: vec3_is_zero(translation, EPS)
                    && quat_is_identity(rotation, EPS)
                    && scale.iter().all(|c| (c - 1.0).abs() < EPS),
                matrix_is_identity: mat4_is_identity(m, EPS),
            }
        }
    }
}

/// Simula un loader que ignora `node.matrix` (hipótesis de bug).
pub(crate) fn gltf_node_local_ignore_matrix(node: &gltf::Node) -> Mat4 {
    match node.transform() {
        gltf::scene::Transform::Matrix { .. } => Mat4::IDENTITY,
        gltf::scene::Transform::Decomposed {
            translation,
            rotation,
            scale,
        } => Mat4::from_scale_rotation_translation(
            Vec3::from(scale),
            Quat::from_array(rotation),
            Vec3::from(translation),
        ),
    }
}

/// Inversa segura de IBM (evita NaN con matrices cero; no filtra por determinante).
pub(crate) fn mat4_from_ibm_inverse(ibm: &[[f32; 4]; 4]) -> Option<Mat4> {
    if ibm.iter().flatten().all(|&c| c == 0.0) {
        return None;
    }
    let m = Mat4::from_cols_array_2d(ibm);
    if !m.to_cols_array().iter().all(|v| v.is_finite()) {
        return None;
    }
    let inv = m.inverse();
    if !inv.to_cols_array().iter().all(|v| v.is_finite()) {
        return None;
    }
    Some(inv)
}

/// global[j] = inverse(IBM[j]) × (local[j] × inverse(bind_local[j]))
pub(crate) fn gltf_bind_globals_from_ibm(
    inverse_bind: &[[[f32; 4]; 4]],
    bind_local: &[Mat4],
    locals: &[Mat4],
) -> Vec<Mat4> {
    let n = locals
        .len()
        .min(bind_local.len())
        .min(inverse_bind.len());
    (0..n)
        .map(|ji| {
            let rel = locals[ji] * bind_local[ji].inverse();
            mat4_from_ibm_inverse(&inverse_bind[ji])
                .map(|bind_global| bind_global * rel)
                .unwrap_or(Mat4::IDENTITY)
        })
        .collect()
}

fn skinned_mesh_extent(vertices: &[SkinnedVertex]) -> f32 {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for v in vertices {
        if !v.position.iter().all(|c| c.is_finite()) {
            continue;
        }
        any = true;
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
        }
    }
    if any {
        (Vec3::from_array(max) - Vec3::from_array(min)).length()
    } else {
        0.0
    }
}

fn skeleton_extent_from_globals(globals: &[Mat4]) -> f32 {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for g in globals {
        let p = g.transform_point3(Vec3::ZERO);
        if !p.is_finite() {
            continue;
        }
        any = true;
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    if any {
        (Vec3::from_array(max) - Vec3::from_array(min)).length()
    } else {
        0.0
    }
}

/// Extent del esqueleto en el mismo espacio que la malla dibujada (post-`mesh_normalize`).
pub(crate) fn skeleton_extent_mesh_space(globals: &[Mat4], mesh_normalize: Mat4) -> f32 {
    if mesh_normalize == Mat4::IDENTITY {
        return skeleton_extent_from_globals(globals);
    }
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for g in globals {
        let p = mesh_normalize.transform_point3(g.transform_point3(Vec3::ZERO));
        if !p.is_finite() {
            continue;
        }
        any = true;
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    if any {
        (Vec3::from_array(max) - Vec3::from_array(min)).length()
    } else {
        0.0
    }
}

/// Estadísticas compartidas entre detector y `[SKIN_XFORM]`.
pub(crate) fn skin_bind_hierarchy_ibm_stats(
    hierarchy_globals: &[Mat4],
    inverse_bind: &[[[f32; 4]; 4]],
) -> (usize, usize, f32) {
    let joint_count = hierarchy_globals.len().min(inverse_bind.len());
    let mut mismatch = 0usize;
    let mut ibm_pos = 0usize;
    for ji in 0..joint_count {
        let engine_t = hierarchy_globals[ji].transform_point3(Vec3::ZERO);
        let Some(ibm_inv) = mat4_from_ibm_inverse(&inverse_bind[ji]) else {
            continue;
        };
        let ibm_t = ibm_inv.transform_point3(Vec3::ZERO);
        if ibm_t.length() > 1e-2 {
            ibm_pos += 1;
        }
        if engine_t.length() < 1e-3 && ibm_t.length() > 1e-2 {
            mismatch += 1;
        }
    }
    let skel_span = skeleton_extent_from_globals(hierarchy_globals);
    (mismatch, ibm_pos, skel_span)
}

/// Detecta rigs FBX→GLB donde la bind pose vive en IBM y los nodos tienen TRS≈identidad.
pub(crate) fn detect_bind_pose_from_ibm(
    hierarchy_globals: &[Mat4],
    inverse_bind: &[[[f32; 4]; 4]],
    mesh_vertices: &[SkinnedVertex],
) -> bool {
    let joint_count = hierarchy_globals.len().min(inverse_bind.len());
    if joint_count == 0 {
        return false;
    }

    let mesh_span = skinned_mesh_extent(mesh_vertices);
    let (mismatch, ibm_pos, skel_span) =
        skin_bind_hierarchy_ibm_stats(hierarchy_globals, inverse_bind);

    if skel_span < 0.001 && mesh_span > 0.1 {
        return true;
    }

    if ibm_pos == 0 {
        return false;
    }

    if mismatch >= 20 && mismatch * 2 >= ibm_pos {
        return true;
    }

    if mismatch * 2 >= joint_count {
        return true;
    }

    false
}

const EDITOR_IBM_SKINNED_TARGET_EXTENT: f32 = 1.8;

/// Escala mallas FBX→GLB gigantes (~100 m) a ~1.8 m en editor.
/// También ajusta IBM: `IBM' = IBM × inverse(norm)` para que malla y skinning compartan espacio.
fn editor_normalize_oversized_ibm_skinned(parts: &mut [SkinnedMeshPart]) -> bool {
    let Some((min, max)) = gltf_skinned_parts_bounds(parts) else {
        return false;
    };
    let span = Vec3::from_array(max) - Vec3::from_array(min);
    if span.length() <= 5.0 {
        return false;
    }
    let scale = EDITOR_IBM_SKINNED_TARGET_EXTENT / span.length();
    let center = (Vec3::from_array(min) + Vec3::from_array(max)) * 0.5;
    let norm = Mat4::from_scale(Vec3::splat(scale)) * Mat4::from_translation(-center);
    let inv_norm = norm.inverse();
    for part in parts.iter_mut() {
        apply_normalize_to_skinned_vertices(&mut part.mesh.vertices, norm);
        for ibm in part.inverse_bind.iter_mut() {
            let m = Mat4::from_cols_array_2d(ibm);
            *ibm = (m * inv_norm).to_cols_array_2d();
        }
    }
    log::info!(
        "[SKIN_XFORM] editor normalize | extent={:.3}m → {:.3}m scale={scale:.6} \
         (vértices + IBM ajustados; mesh_normalize=IDENTITY)",
        span.length(),
        EDITOR_IBM_SKINNED_TARGET_EXTENT,
    );
    true
}

fn load_gltf_asset_from_file(
    file: &GltfFile,
    normalize_to_extent: Option<f32>,
) -> Option<Arc<ModelAsset>> {
    let label = std::path::Path::new(&file.path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&file.path);
    let variant = match normalize_to_extent {
        None => "editor",
        Some(h) if (h - crate::config_3d::character_anchor::PLAY_CHARACTER_BODY_HEIGHT).abs()
            < 0.01 =>
        {
            "play_character"
        }
        Some(_) => "normalized",
    };
    log::debug!("[LOAD_GLTF_SKINNED] {label} variant={variant}");

    let doc = &file.doc;
    let buffers = &file.buffers;
    let scene = doc.default_scene().or_else(|| doc.scenes().next())?;
    let scene_parents = build_gltf_node_parents(&scene);
    let all_node_parents = build_gltf_all_node_parents(&doc);

    let mut skinned_nodes: Vec<SkinnedMeshNode> = Vec::new();
    for root in scene.nodes() {
        collect_gltf_skinned_mesh_nodes(root, None, &mut skinned_nodes);
    }
    if skinned_nodes.is_empty() {
        if doc.skins().next().is_some() {
            skin_diag::log_skinned_unavailable(
                label,
                "glTF tiene skins pero ningún nodo mesh+skin reconocido",
            );
        }
        return None;
    }

    let model_label = file.path.clone();
    let label = std::path::Path::new(&model_label)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&model_label)
        .to_string();
    let _skins = unique_skin_count(&skinned_nodes);

    let mut unified = build_gltf_unified_skeleton(&doc, &buffers, &skinned_nodes)?;
    let bones_skin_before_extend = unified.joint_gltf_nodes.len();
    let skin_joint_set: HashSet<usize> = unified.joint_gltf_nodes.iter().copied().collect();
    let skeleton_roots_added = extend_gltf_unified_with_skin_skeleton_roots(&doc, &mut unified);
    let ancestors_added =
        extend_gltf_unified_with_scene_ancestors(&doc, &all_node_parents, &mut unified);
    extend_gltf_unified_with_anim_nodes(&doc, &mut unified);
    let joint_count = unified.joint_gltf_nodes.len();
    if skeleton_roots_added > 0 || ancestors_added > 0 {
        log::debug!(
            "[SKIN_XFORM] {label} extend | skeleton_roots={skeleton_roots_added} \
             scene_ancestors={ancestors_added} bones_skin={bones_skin_before_extend} \
             bones_runtime={joint_count}"
        );
    }
    let node_to_joint = unified.node_to_unified.clone();
    let joint_gltf_nodes = unified.joint_gltf_nodes.clone();
    let joint_names = unified.joint_names.clone();
    let joint_origins = unified.joint_origins.clone();

    let chain_nodes =
        collect_gltf_scene_chain_nodes(&joint_gltf_nodes, &all_node_parents);
    let mut gltf_bind_node_local = HashMap::new();
    for &node_ix in &chain_nodes {
        if let Some(n) = doc.nodes().nth(node_ix) {
            gltf_bind_node_local.insert(node_ix, node_local_matrix(&n));
        }
    }

    let mut asset_parts: Vec<SkinnedMeshPart> = Vec::new();
    let mut remap_drops = skin_diag::RemapDropCollector::default();
    let mut primary_skin_set = false;
    let mut v0_pre_remap: Option<SkinnedVertex> = None;
    let mut mesh_nodes_loaded_skinned: HashSet<usize> = HashSet::new();
    for entry in &skinned_nodes {
        let node_index = entry.node_index;
        let node = match doc.nodes().nth(node_index) {
            Some(n) => n,
            None => continue,
        };
        let node_skin = node
            .skin()
            .or_else(|| doc.skins().nth(entry.skin_index))?;
        let mesh = match node.mesh() {
            Some(m) => m,
            None => continue,
        };
        let mesh_bind_world =
            world_matrix_for_gltf_node_index(&doc, &scene_parents, node_index);
        let (part_ibm, _ibm_fb) = part_inverse_bind_for_gltf_skin(
            &node_skin,
            &buffers,
            &node_to_joint,
            joint_count,
            mesh_bind_world,
            &doc,
            &all_node_parents,
        );
        if !primary_skin_set {
            primary_skin_set = true;
        }
        let part_name = node
            .name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("part_{}", asset_parts.len()));
        let primitives: Vec<_> = mesh.primitives().collect();
        for (prim_i, primitive) in primitives.iter().enumerate() {
            let Some(mut data) = read_skinned_gltf_primitive(
                primitive,
                &buffers,
                &file.material_smallest_albedos,
            ) else {
                log::warn!(
                    "[SKIN_ORDER] {label} primitiva {part_name} p{prim_i} omitida (sin POSITION/JOINTS_0/WEIGHTS_0)"
                );
                continue;
            };
            if v0_pre_remap.is_none() && !data.vertices.is_empty() {
                v0_pre_remap = Some(data.vertices[0].clone());
            }
            remap_gltf_vertex_joints_to_unified(
                &mut data,
                &node_skin,
                &node_to_joint,
                &mut remap_drops,
            );
            let material_index = primitive.material().index().unwrap_or(0) as u32;
            let name = if primitives.len() > 1 {
                format!("{part_name}_m{material_index}_p{prim_i}")
            } else {
                part_name.clone()
            };
            let mut part = SkinnedMeshPart {
                name,
                material_index,
                mesh: data,
                mesh_bind_world,
                inverse_bind: part_ibm.clone(),
            };
            bake_gltf_mesh_bind_world(&mut part);
            asset_parts.push(part);
            mesh_nodes_loaded_skinned.insert(node_index);
        }
    }

    if asset_parts.is_empty() {
        return None;
    }

    let bind_local: Vec<Mat4> = joint_gltf_nodes
        .iter()
        .map(|&node_ix| {
            doc.nodes()
                .nth(node_ix)
                .map(|n| node_local_matrix(&n))
                .unwrap_or(Mat4::IDENTITY)
        })
        .collect();

    let globals = compute_gltf_joint_worlds(
        &joint_gltf_nodes,
        &bind_local,
        &all_node_parents,
        &gltf_bind_node_local,
    );

    let ibm_for_detect = asset_parts
        .first()
        .map(|p| p.inverse_bind.clone())
        .unwrap_or_else(|| vec![[[0.0; 4]; 4]; joint_count]);

    let bind_pose_from_ibm = asset_parts.first().map_or(false, |first| {
        detect_bind_pose_from_ibm(&globals, &ibm_for_detect, &first.mesh.vertices)
    });

    let default_ibm = asset_parts
        .first()
        .map(|p| p.inverse_bind.clone())
        .unwrap_or_else(|| vec![[[0.0; 4]; 4]; joint_count]);

    let mut runtime_globals = if bind_pose_from_ibm {
        gltf_bind_globals_from_ibm(&default_ibm, &bind_local, &bind_local)
    } else {
        globals.clone()
    };

    let rigid_ibm = asset_parts.first().map(|p| p.inverse_bind.clone());
    let loaded_skinned_nodes: HashSet<usize> =
        skinned_nodes.iter().map(|n| n.node_index).collect();
    let mut scene_mesh_nodes: Vec<(usize, String)> = Vec::new();
    for root in scene.nodes() {
        collect_gltf_scene_mesh_nodes(root, &mut scene_mesh_nodes);
    }
    let mut rigid_parts_loaded = 0usize;
    let mut rigid_parts_skipped = 0usize;
    let mut mesh_nodes_loaded_rigid: HashSet<usize> = HashSet::new();
    let mut mesh_nodes_rigid_no_joint: HashSet<usize> = HashSet::new();
    let mut rigid_bind_diags: Vec<RigidBindDiag> = Vec::new();
    let mut rigid_bind_diag_nodes: HashSet<usize> = HashSet::new();
    let mut rigid_spatial_fallback_count = 0usize;
    let mut rigid_candidate_count = 0usize;
    if let Some(ibm) = rigid_ibm {
        for (node_index, node_name) in &scene_mesh_nodes {
            if loaded_skinned_nodes.contains(node_index) {
                continue;
            }
            rigid_candidate_count += 1;
            let (mesh_bind_world, mesh_world_ref) =
                gltf_rigid_mesh_spatial_reference(&doc, &buffers, &scene_parents, *node_index);
            log_rigid_world_probe(&label, node_name, *node_index, mesh_bind_world, mesh_world_ref);
            let Some(trace) = trace_rigid_anchor_for_node(
                &label,
                node_name,
                *node_index,
                mesh_world_ref,
                &all_node_parents,
                &node_to_joint,
                &skin_joint_set,
                &joint_origins,
                &joint_names,
                &runtime_globals,
                Some(&doc),
            ) else {
                log::warn!(
                    "[SKIN_LOAD] {label} malla «{node_name}» sin skin ni joint ancestro — omitida"
                );
                rigid_parts_skipped += 1;
                mesh_nodes_rigid_no_joint.insert(*node_index);
                continue;
            };
            let joint_ui = trace.anchor_ui;
            if trace.spatial_fallback {
                rigid_spatial_fallback_count += 1;
            }
            if rigid_bind_diag_nodes.insert(*node_index) {
                let parent_ix = all_node_parents.get(node_index).copied();
                rigid_bind_diags.push(RigidBindDiag {
                    mesh_name: node_name.clone(),
                    gltf_node: *node_index,
                    parent_node_ix: parent_ix,
                    parent_node_name: parent_ix
                        .map(|p| gltf_node_display_name(&doc, p))
                        .unwrap_or_else(|| "-".to_string()),
                    trace,
                    mesh_bind_world,
                    mesh_world_ref,
                });
            }
            let node = match doc.nodes().nth(*node_index) {
                Some(n) => n,
                None => continue,
            };
            let mesh = match node.mesh() {
                Some(m) => m,
                None => continue,
            };
            let primitives: Vec<_> = mesh.primitives().collect();
            for (prim_i, primitive) in primitives.iter().enumerate() {
                let Some(data) = read_gltf_rigid_primitive(
                    primitive,
                    &buffers,
                    &file.material_smallest_albedos,
                    joint_ui as u32,
                ) else {
                    log::warn!(
                        "[SKIN_LOAD] {label} primitiva rígida {node_name} p{prim_i} omitida (sin POSITION)"
                    );
                    continue;
                };
                let material_index = primitive.material().index().unwrap_or(0) as u32;
                let name = if primitives.len() > 1 {
                    format!("{node_name}_rigid_m{material_index}_p{prim_i}")
                } else {
                    format!("{node_name}_rigid")
                };
                let mut part = SkinnedMeshPart {
                    name,
                    material_index,
                    mesh: data,
                    mesh_bind_world,
                    inverse_bind: ibm.clone(),
                };
                bake_gltf_mesh_bind_world(&mut part);
                asset_parts.push(part);
                rigid_parts_loaded += 1;
                mesh_nodes_loaded_rigid.insert(*node_index);
            }
        }
        if rigid_candidate_count > 0 {
            log::info!(
                "[RIGID_NEAREST] {label} resumen fallback_espacial={rigid_spatial_fallback_count}/{rigid_candidate_count} \
                 omitidas={rigid_parts_skipped} skin_joint_set={}",
                skin_joint_set.len(),
            );
        }
    }

    log_gltf_mesh_classification_audit(
        &label,
        &doc,
        &buffers,
        &scene,
        &scene_mesh_nodes,
        &skinned_nodes,
        &all_node_parents,
        &node_to_joint,
        &joint_gltf_nodes,
        &joint_names,
        &mesh_nodes_loaded_skinned,
        &mesh_nodes_loaded_rigid,
        &mesh_nodes_rigid_no_joint,
        &skin_joint_set,
        &joint_origins,
        &runtime_globals,
        unified.skin_count,
        joint_count,
    );

    remap_drops.log_if_any(&label);

    let mut mesh_normalize = if let Some(target_height) = normalize_to_extent {
        let joint_positions = bind_joint_world_positions(
            &joint_gltf_nodes,
            &bind_local,
            &all_node_parents,
            &gltf_bind_node_local,
        );
        let upright = upright_quat_for_gltf_play_character(
            &asset_parts,
            &joint_names,
            &joint_gltf_nodes,
            &bind_local,
            &all_node_parents,
            &gltf_bind_node_local,
        )?;
        let (height, center) =
            gltf_play_bind_height_and_center(&asset_parts, &joint_positions, upright)?;
        let scale = (target_height / height).clamp(0.001, 50.0);
        let mut norm =
            Mat4::from_scale(Vec3::splat(scale)) * Mat4::from_translation(-center) * Mat4::from_quat(upright);
        log::info!(
            "[model_asset] glTF play scale: height={height:.4} scale={scale:.6} target={target_height:.3}",
        );
        for part in asset_parts.iter_mut() {
            apply_normalize_to_skinned_vertices(&mut part.mesh.vertices, norm);
        }
        if let Some(bind_center) = gltf_play_bind_pose_aabb_center(
            &asset_parts,
            &joint_gltf_nodes,
            &bind_local,
            &scene_parents,
            &gltf_bind_node_local,
            norm,
        ) {
            if bind_center.length_squared() > 1e-10 {
                let fix = Mat4::from_translation(-bind_center);
                for part in asset_parts.iter_mut() {
                    apply_normalize_to_skinned_vertices(&mut part.mesh.vertices, fix);
                }
                norm = fix * norm;
                log::info!(
                    "[model_asset] glTF play bind-pose center: ({:.4}, {:.4}, {:.4})",
                    bind_center.x,
                    bind_center.y,
                    bind_center.z
                );
            }
        }
        norm
    } else {
        Mat4::IDENTITY
    };

    if bind_pose_from_ibm && normalize_to_extent.is_none() {
        if editor_normalize_oversized_ibm_skinned(&mut asset_parts) {
            mesh_normalize = Mat4::IDENTITY;
            let default_ibm = asset_parts
                .first()
                .map(|p| p.inverse_bind.clone())
                .unwrap_or_else(|| default_ibm.clone());
            runtime_globals = gltf_bind_globals_from_ibm(&default_ibm, &bind_local, &bind_local);
        }
    }

    if bind_pose_from_ibm {
        log::info!(
            "[SKIN_XFORM] {label} bind_pose_from_ibm=true | skel_span_hier={:.3}m \
             skel_span_runtime={:.3}m skel_span_mesh={:.3}m mesh_span={:.3}m",
            skeleton_extent_from_globals(&globals),
            skeleton_extent_from_globals(&runtime_globals),
            skeleton_extent_mesh_space(&runtime_globals, mesh_normalize),
            skinned_mesh_extent(
                asset_parts
                    .first()
                    .map(|p| p.mesh.vertices.as_slice())
                    .unwrap_or(&[]),
            ),
        );
    }

    log_rigid_bind_diagnostics(
        &label,
        bind_pose_from_ibm,
        ancestors_added,
        &globals,
        &runtime_globals,
        &joint_names,
        &rigid_bind_diags,
    );

    if primary_skin_set {
        if let Some(first) = asset_parts.first() {
            skin_diag::log_skin_transform_probe(
                &label,
                &doc,
                &joint_gltf_nodes,
                &joint_names,
                &bind_local,
                &globals,
                &runtime_globals,
                &first.inverse_bind,
                &all_node_parents,
                &first.mesh.vertices,
                bones_skin_before_extend,
                joint_count,
                remap_drops.total,
                bind_pose_from_ibm,
                mesh_normalize,
            );
            skin_diag::log_skin_dual_space_report(
                &label,
                &joint_names,
                &bind_local,
                &globals,
                &runtime_globals,
                &first.inverse_bind,
                mesh_normalize,
            );
        }
    }

    let mut clips = Vec::new();
    for anim in doc.animations() {
        if let Some(clip) = parse_gltf_animation(&anim, &buffers, &node_to_joint) {
            if !clip.channels.is_empty() {
                clips.push(clip);
            }
        }
    }

    log::info!(
        "[SKIN_LOAD] {label} | piezas={} (skinned={} rigid={}) huesos={} \
         mallas_escena={} rigid_omitidas={} vértices={} tex_embebidas={}/{}",
        asset_parts.len(),
        asset_parts.len().saturating_sub(rigid_parts_loaded),
        rigid_parts_loaded,
        joint_count,
        scene_mesh_nodes.len(),
        rigid_parts_skipped,
        asset_parts
            .iter()
            .map(|p| p.mesh.vertices.len())
            .sum::<usize>(),
        asset_parts
            .iter()
            .filter(|p| p.mesh.width > 1 || p.mesh.height > 1)
            .count(),
        asset_parts.len(),
    );
    for (pi, part) in asset_parts.iter().enumerate() {
        log::debug!(
            "[SKIN_LOAD] {label} p{pi} «{}» verts={} tris={} mat={} tex={}x{}",
            part.name,
            part.mesh.vertices.len(),
            part.mesh.indices.len() / 3,
            part.material_index,
            part.mesh.width,
            part.mesh.height,
        );
    }

    log::debug!(
        "[model_asset] glTF skinned: {} skin(s), {} huesos, {} pieza(s) desde {}",
        unified.skin_count,
        joint_count,
        asset_parts.len(),
        file.path
    );

    Some(Arc::new(ModelAsset {
        path: file.path.clone(),
        joint_parents: vec![None; joint_count],
        inverse_bind: default_ibm,
        bind_local,
        joint_prefix_world: vec![Mat4::IDENTITY; joint_count],
        joint_names,
        parts: asset_parts,
        clips,
        mesh_normalize,
        facing_forward_xz: {
            let node_ix = gltf_primary_mesh_node_index(file);
            node_ix
                .map(|ix| {
                    crate::config_3d::mesh_3d::forward_xz_from_node_world(
                        world_matrix_for_gltf_node_index(&doc, &scene_parents, ix),
                    )
                })
                .unwrap_or(glam::Vec2::new(0.0, 1.0))
        },
        joint_gltf_nodes,
        gltf_scene_parents: all_node_parents,
        gltf_bind_node_local,
        bind_pose_from_ibm,
    }))
}

fn unique_skin_count(nodes: &[SkinnedMeshNode]) -> usize {
    let mut seen = HashSet::new();
    for n in nodes {
        seen.insert(n.skin_index);
    }
    seen.len()
}

fn collect_gltf_skinned_mesh_nodes(
    node: gltf::Node,
    inherited_skin: Option<usize>,
    out: &mut Vec<SkinnedMeshNode>,
) {
    let skin_ix = node.skin().map(|s| s.index()).or(inherited_skin);
    if let Some(skin_index) = skin_ix {
        if node.mesh().is_some() {
            out.push(SkinnedMeshNode {
                node_index: node.index(),
                skin_index,
                inherited_skin: node.skin().is_none(),
            });
        }
    }
    let child_inherit = node.skin().map(|s| s.index()).or(inherited_skin);
    for child in node.children() {
        collect_gltf_skinned_mesh_nodes(child, child_inherit, out);
    }
}

fn collect_gltf_scene_mesh_nodes(node: gltf::Node, out: &mut Vec<(usize, String)>) {
    if node.mesh().is_some() {
        let name = node
            .name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("mesh_node_{}", node.index()));
        out.push((node.index(), name));
    }
    for child in node.children() {
        collect_gltf_scene_mesh_nodes(child, out);
    }
}

/// Mapea nodos mesh alcanzables con skin propio o heredado (misma regla que `collect_gltf_skinned_mesh_nodes`).
fn map_gltf_mesh_effective_skin(
    scene: &gltf::Scene,
    out: &mut HashMap<usize, (usize, bool)>,
) {
    fn walk(node: gltf::Node, inherited_skin: Option<usize>, out: &mut HashMap<usize, (usize, bool)>) {
        let skin_ix = node.skin().map(|s| s.index()).or(inherited_skin);
        if node.mesh().is_some() {
            if let Some(skin_index) = skin_ix {
                out.insert(node.index(), (skin_index, node.skin().is_none()));
            }
        }
        let child_inherit = node.skin().map(|s| s.index()).or(inherited_skin);
        for child in node.children() {
            walk(child, child_inherit, out);
        }
    }
    for root in scene.nodes() {
        walk(root, None, out);
    }
}

fn log_gltf_mesh_classification_audit(
    label: &str,
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    scene: &gltf::Scene,
    scene_mesh_nodes: &[(usize, String)],
    skinned_nodes: &[SkinnedMeshNode],
    all_node_parents: &HashMap<usize, usize>,
    node_to_joint: &HashMap<usize, usize>,
    joint_gltf_nodes: &[usize],
    joint_names: &[String],
    loaded_skinned: &HashSet<usize>,
    loaded_rigid: &HashSet<usize>,
    rigid_no_joint: &HashSet<usize>,
    skin_joint_set: &HashSet<usize>,
    joint_origins: &[GltfJointOrigin],
    runtime_globals: &[Mat4],
    skin_count: usize,
    joint_count: usize,
) {
    let mut effective_skin = HashMap::new();
    map_gltf_mesh_effective_skin(scene, &mut effective_skin);
    let scene_parents = build_gltf_node_parents(scene);
    let skinned_in_tree: HashSet<usize> = skinned_nodes.iter().map(|n| n.node_index).collect();
    let doc_skin_total = doc.skins().count();

    let mut count_skinned_tree = 0usize;
    let mut count_rigid_tree = 0usize;
    let mut count_omitida = 0usize;
    let mut skinned_names: Vec<String> = Vec::new();

    for (idx, (node_index, node_name)) in scene_mesh_nodes.iter().enumerate() {
        let gltf_skin_prop = doc
            .nodes()
            .nth(*node_index)
            .and_then(|n| n.skin())
            .map(|s| s.index());

        let (class, skin_index_str, anchor_str, extra) =
            if skinned_in_tree.contains(node_index) {
                count_skinned_tree += 1;
                let entry = skinned_nodes
                    .iter()
                    .find(|e| e.node_index == *node_index);
                let Some(entry) = entry else {
                    log::warn!(
                        "[SKIN_MESH_AUDIT] {label} node={node_index} «{node_name}» \
                         inconsistencia: en skinned_in_tree sin entrada"
                    );
                    continue;
                };
                skinned_names.push(format!(
                    "«{node_name}» skin={}{}",
                    entry.skin_index,
                    if entry.inherited_skin { " heredado" } else { "" }
                ));
                let loaded = if loaded_skinned.contains(node_index) {
                    "si"
                } else {
                    "no_primitivas"
                };
                (
                    "skinned",
                    format!("{}", entry.skin_index),
                    "-".to_string(),
                    format!(
                        "skin_propio={} skin_heredado={} gltf_skin={} cargada={loaded}",
                        !entry.inherited_skin,
                        entry.inherited_skin,
                        gltf_skin_prop
                            .map(|i| i.to_string())
                            .unwrap_or_else(|| "-".to_string()),
                    ),
                )
            } else if let Some(joint_ui) = nearest_unified_joint_for_node(
                label,
                node_name,
                *node_index,
                gltf_rigid_mesh_spatial_reference(doc, buffers, &scene_parents, *node_index).1,
                all_node_parents,
                node_to_joint,
                skin_joint_set,
                joint_origins,
                joint_names,
                runtime_globals,
                Some(doc),
            ) {
                count_rigid_tree += 1;
                let joint_gltf_node = joint_gltf_nodes.get(joint_ui).copied();
                let joint_label = joint_names
                    .get(joint_ui)
                    .map(String::as_str)
                    .unwrap_or("?");
                let loaded = if loaded_rigid.contains(node_index) {
                    "si"
                } else {
                    "no"
                };
                let in_effective = effective_skin.contains_key(node_index);
                (
                    "rigid",
                    "-".to_string(),
                    format!(
                        "ui={joint_ui} «{joint_label}» gltf_node={}",
                        joint_gltf_node
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "?".to_string())
                    ),
                    format!(
                        "skin_en_arbol=false gltf_skin={} cargada={loaded} \
                         motivo=fuera_cadena_skin_desde_raiz in_effective_skin_map={in_effective}",
                        gltf_skin_prop
                            .map(|i| i.to_string())
                            .unwrap_or_else(|| "-".to_string()),
                    ),
                )
            } else {
                count_omitida += 1;
                let motivo = if rigid_no_joint.contains(node_index) {
                    "sin_joint_ancestro"
                } else {
                    "sin_clasificar"
                };
                (
                    "omitida",
                    "-".to_string(),
                    "-".to_string(),
                    format!(
                        "gltf_skin={} motivo={motivo}",
                        gltf_skin_prop
                            .map(|i| i.to_string())
                            .unwrap_or_else(|| "-".to_string()),
                    ),
                )
            };

        log::debug!(
            "[SKIN_MESH_AUDIT] {label} [{:02}/{}] node={node_index} «{node_name}» \
             class={class} skin_index={skin_index_str} anchor={anchor_str} {extra}",
            idx + 1,
            scene_mesh_nodes.len(),
        );
    }

    if !skinned_names.is_empty() {
        log::info!(
            "[SKIN_MESH_AUDIT] {label} piezas_skinned: {}",
            skinned_names.join(", "),
        );
    }

    log::info!(
        "[SKIN_MESH_AUDIT] {label} | mallas={} skins_gltf={doc_skin_total} skins_carga={skin_count} \
         huesos={joint_count} skinned={count_skinned_tree} rigid={count_rigid_tree} omitida={count_omitida} \
         fuera_arbol_skinned={}",
        scene_mesh_nodes.len(),
        scene_mesh_nodes.len().saturating_sub(effective_skin.len()),
    );

    log::debug!(
        "[SKIN_MESH_AUDIT] {label} cargadas GPU: skinned={} rigid={}",
        loaded_skinned.len(),
        loaded_rigid.len(),
    );
}

fn nearest_unified_joint_for_node(
    label: &str,
    mesh_name: &str,
    node_index: usize,
    mesh_world_ref: Vec3,
    scene_parents: &HashMap<usize, usize>,
    node_to_joint: &HashMap<usize, usize>,
    skin_joint_set: &HashSet<usize>,
    joint_origins: &[GltfJointOrigin],
    joint_names: &[String],
    runtime_globals: &[Mat4],
    doc: Option<&gltf::Document>,
) -> Option<usize> {
    trace_rigid_anchor_for_node(
        label,
        mesh_name,
        node_index,
        mesh_world_ref,
        scene_parents,
        node_to_joint,
        skin_joint_set,
        joint_origins,
        joint_names,
        runtime_globals,
        doc,
    )
    .map(|t| t.anchor_ui)
}

fn is_valid_rigid_anchor_origin(origin: GltfJointOrigin) -> bool {
    matches!(
        origin,
        GltfJointOrigin::SkinJoint | GltfJointOrigin::AnimNode
    )
}

struct RigidAnchorChainStep {
    node_ix:   usize,
    name:      String,
    joint_ui:  Option<usize>,
}

#[derive(Clone)]
struct SkinJointInChain {
    gltf_node:       usize,
    ui:              usize,
    name:            String,
    steps_from_mesh: usize,
}

struct RigidAnchorTrace {
    anchor_ui:          usize,
    anchor_gltf_node:   usize,
    anchor_name:        String,
    anchor_origin:      GltfJointOrigin,
    /// Niveles de padres subidos desde la malla hasta el anchor (0 = la propia malla).
    anchor_depth:       usize,
    stopped_at_node:    usize,
    chain:              Vec<RigidAnchorChainStep>,
    reason:             String,
    nearest_skin_joint: Option<SkinJointInChain>,
    spatial_fallback:   bool,
}

struct RigidBindDiag {
    mesh_name:        String,
    gltf_node:        usize,
    parent_node_ix:   Option<usize>,
    parent_node_name: String,
    trace:            RigidAnchorTrace,
    mesh_bind_world:  Mat4,
    /// Centro AABB de la geometría en espacio escena (referencia espacial del fallback).
    mesh_world_ref:   Vec3,
}

fn gltf_node_display_name(doc: &gltf::Document, node_ix: usize) -> String {
    doc.nodes()
        .nth(node_ix)
        .and_then(|n| n.name())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("node_{node_ix}"))
}

fn mat4_world_translation(m: Mat4) -> Vec3 {
    m.transform_point3(Vec3::ZERO)
}

fn format_vec3_xyz(v: Vec3) -> String {
    format!("({:.3},{:.3},{:.3})", v.x, v.y, v.z)
}

/// Centro del AABB de todas las primitivas POSITION del nodo mesh (espacio local del nodo).
fn gltf_mesh_node_local_aabb_center(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    node_index: usize,
) -> Option<Vec3> {
    let node = doc.nodes().nth(node_index)?;
    let mesh = node.mesh()?;
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    let mut any = false;
    for prim in mesh.primitives() {
        let reader = prim.reader(|buf| Some(&buffers[buf.index()]));
        let Some(iter) = reader.read_positions() else {
            continue;
        };
        for p in iter {
            any = true;
            let v = Vec3::from(p);
            min = min.min(v);
            max = max.max(v);
        }
    }
    any.then_some((min + max) * 0.5)
}

/// Matriz bind del nodo + punto de referencia espacial (centro geométrico en espacio escena).
fn gltf_rigid_mesh_spatial_reference(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    node_parents: &HashMap<usize, usize>,
    node_index: usize,
) -> (Mat4, Vec3) {
    let mesh_bind_world = world_matrix_for_gltf_node_index(doc, node_parents, node_index);
    let node_origin = mat4_world_translation(mesh_bind_world);
    let mesh_world_ref = gltf_mesh_node_local_aabb_center(doc, buffers, node_index)
        .map(|local| mesh_bind_world.transform_point3(local))
        .unwrap_or(node_origin);
    (mesh_bind_world, mesh_world_ref)
}

fn log_rigid_world_probe(
    label: &str,
    mesh_name: &str,
    node_index: usize,
    mesh_bind_world: Mat4,
    mesh_world_ref: Vec3,
) {
    let node_origin = mat4_world_translation(mesh_bind_world);
    log::info!(
        "[RIGID_WORLD] {label} mesh=«{mesh_name}» node={node_index} \
         node_origin={} world_pos={}",
        format_vec3_xyz(node_origin),
        format_vec3_xyz(mesh_world_ref),
    );
}

const RIGID_BIND_DETAIL_OFFSET_M: f32 = 0.5;

const RIGID_BIND_DETAIL_KEYWORDS: &[&str] = &[
    "eye", "head", "face", "hair", "helmet", "hat", "glasses", "mask",
];

fn rigid_bind_detail_keyword_mesh(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    RIGID_BIND_DETAIL_KEYWORDS.iter().any(|k| n.contains(k))
}

fn should_log_rigid_bind_detail(mesh_name: &str, offset_m: f32) -> bool {
    offset_m > RIGID_BIND_DETAIL_OFFSET_M || rigid_bind_detail_keyword_mesh(mesh_name)
}

fn log_rigid_bind_detail(
    mesh_name: &str,
    anchor_name: &str,
    anchor_is_joint: bool,
    anchor_depth: usize,
    offset_m: f32,
    world_pos: Vec3,
) {
    log::info!(
        "[RIGID_BIND_DETAIL]\n\
         mesh={mesh_name}\n\
         anchor={anchor_name}\n\
         anchor_is_joint={anchor_is_joint}\n\
         anchor_depth={anchor_depth}\n\
         offset={offset_m:.3}m\n\
         world_pos=({:.2},{:.2},{:.2})",
        world_pos.x,
        world_pos.y,
        world_pos.z,
    );
}

/// Recorre padres desde la malla; el anchor válido es el primer nodo en `node_to_joint`
/// con origen `SkinJoint` o `AnimNode`. `SceneAncestor` y `SkeletonRoot` se ignoran.
/// Si no hay anchor en cadena, fallback espacial al `skin.joints` más cercano (`runtime_globals`).
/// `mesh_world_ref` debe ser el centro AABB de la geometría en espacio escena, no el origen del nodo.
fn trace_rigid_anchor_for_node(
    label: &str,
    mesh_name: &str,
    mesh_node: usize,
    mesh_world_ref: Vec3,
    scene_parents: &HashMap<usize, usize>,
    node_to_joint: &HashMap<usize, usize>,
    skin_joint_set: &HashSet<usize>,
    joint_origins: &[GltfJointOrigin],
    joint_names: &[String],
    runtime_globals: &[Mat4],
    doc: Option<&gltf::Document>,
) -> Option<RigidAnchorTrace> {
    let node_name = |ix: usize| -> String {
        doc.map(|d| gltf_node_display_name(d, ix))
            .unwrap_or_else(|| format!("node_{ix}"))
    };

    let mut chain: Vec<RigidAnchorChainStep> = Vec::new();
    let mut skin_joints_in_chain: Vec<SkinJointInChain> = Vec::new();
    let mut cur = mesh_node;
    for step in 0..512 {
        let name = node_name(cur);
        let joint_ui = node_to_joint.get(&cur).copied();
        if skin_joint_set.contains(&cur) {
            if let Some(ui) = joint_ui {
                skin_joints_in_chain.push(SkinJointInChain {
                    gltf_node: cur,
                    ui,
                    name: name.clone(),
                    steps_from_mesh: step,
                });
            }
        }
        chain.push(RigidAnchorChainStep {
            node_ix: cur,
            name: name.clone(),
            joint_ui,
        });
        if let Some(anchor_ui) = joint_ui {
            let anchor_origin = joint_origins
                .get(anchor_ui)
                .copied()
                .unwrap_or(GltfJointOrigin::SkinJoint);
            if is_valid_rigid_anchor_origin(anchor_origin) {
                let nearest_skin_joint = skin_joints_in_chain.first().cloned();
                let skin_joint_note = match &nearest_skin_joint {
                    Some(sj) if sj.gltf_node == cur => {
                        "coincide con el primer skin.joints en la cadena".to_string()
                    }
                    Some(sj) => format!(
                        "primer skin.joints «{}» ui={} node={} a {} nivel/es arriba",
                        sj.name, sj.ui, sj.gltf_node, sj.steps_from_mesh,
                    ),
                    None => "ningún skin.joints en la cadena de padres".to_string(),
                };
                let reason = if step == 0 {
                    format!(
                        "nodo mesh en node_to_joint ui={anchor_ui} origen={} ({skin_joint_note})",
                        anchor_origin.label(),
                    )
                } else {
                    format!(
                        "anchor en cadena: node={cur} «{name}» ui={anchor_ui} \
                         (+{step} niveles; origen={}; {skin_joint_note})",
                        anchor_origin.label(),
                    )
                };
                log_rigid_graph_trace(
                    label,
                    mesh_name,
                    &chain,
                    &skin_joints_in_chain,
                );
                return Some(RigidAnchorTrace {
                    anchor_ui,
                    anchor_gltf_node: cur,
                    anchor_name: name,
                    anchor_origin,
                    anchor_depth: step,
                    stopped_at_node: cur,
                    chain,
                    reason,
                    nearest_skin_joint,
                    spatial_fallback: false,
                });
            }
        }
        let Some(&parent) = scene_parents.get(&cur) else {
            break;
        };
        cur = parent;
    }

    log_rigid_graph_trace(label, mesh_name, &chain, &skin_joints_in_chain);

    let (anchor_ui, anchor_gltf_node, anchor_name, dist) = match nearest_skin_joint_spatial(
        mesh_world_ref,
        skin_joint_set,
        node_to_joint,
        joint_names,
        runtime_globals,
        doc,
    ) {
        Some(found) => found,
        None => {
            log::warn!(
                "[RIGID_NEAREST] {label} mesh=«{mesh_name}» dist=- joint=- \
                 (fallback falló: skin_joint_set={} sin entrada en node_to_joint)",
                skin_joint_set.len(),
            );
            return None;
        }
    };
    let nearest_skin_joint = Some(SkinJointInChain {
        gltf_node: anchor_gltf_node,
        ui: anchor_ui,
        name: anchor_name.clone(),
        steps_from_mesh: 0,
    });
    log::info!(
        "[RIGID_NEAREST] {label} mesh=«{mesh_name}» joint=«{anchor_name}» dist={dist:.4}m",
    );
    let reason = format!(
        "fallback espacial a skin.joints más cercano → «{anchor_name}» ui={anchor_ui} dist={dist:.4}m",
    );
    Some(RigidAnchorTrace {
        anchor_ui,
        anchor_gltf_node,
        anchor_name,
        anchor_origin: GltfJointOrigin::SkinJoint,
        anchor_depth: chain.len().saturating_sub(1),
        stopped_at_node: mesh_node,
        chain,
        reason,
        nearest_skin_joint,
        spatial_fallback: true,
    })
}

fn log_rigid_graph_trace(
    label: &str,
    mesh_name: &str,
    chain: &[RigidAnchorChainStep],
    skin_joints_in_chain: &[SkinJointInChain],
) {
    let skin_list: Vec<String> = skin_joints_in_chain
        .iter()
        .map(|sj| format!("«{}» ui={} node={} (+{})", sj.name, sj.ui, sj.gltf_node, sj.steps_from_mesh))
        .collect();
    log::debug!(
        "[RIGID_GRAPH] {label} mesh=«{mesh_name}» parent_chain={} \
         skin_joints_in_chain=[{}] intersection_count={}",
        format_rigid_anchor_chain(chain),
        if skin_list.is_empty() {
            "-".to_string()
        } else {
            skin_list.join(", ")
        },
        skin_joints_in_chain.len(),
    );
}

fn nearest_skin_joint_spatial(
    mesh_world_ref: Vec3,
    skin_joint_set: &HashSet<usize>,
    node_to_joint: &HashMap<usize, usize>,
    joint_names: &[String],
    runtime_globals: &[Mat4],
    doc: Option<&gltf::Document>,
) -> Option<(usize, usize, String, f32)> {
    if skin_joint_set.is_empty() {
        return None;
    }
    let mut best: Option<(usize, usize, String, f32)> = None;
    for &gltf_node in skin_joint_set {
        let Some(&ui) = node_to_joint.get(&gltf_node) else {
            continue;
        };
        let joint_m = runtime_globals.get(ui).copied().unwrap_or(Mat4::IDENTITY);
        let joint_pos = mat4_world_translation(joint_m);
        let dist = (mesh_world_ref - joint_pos).length();
        if best.as_ref().map_or(true, |(_, _, _, d)| dist < *d) {
            let name = joint_names
                .get(ui)
                .cloned()
                .or_else(|| doc.map(|d| gltf_node_display_name(d, gltf_node)))
                .unwrap_or_else(|| format!("node_{gltf_node}"));
            best = Some((ui, gltf_node, name, dist));
        }
    }
    best
}

fn format_rigid_anchor_chain(chain: &[RigidAnchorChainStep]) -> String {
    chain
        .iter()
        .map(|s| {
            let joint = s
                .joint_ui
                .map(|ui| format!("joint=ui{ui}"))
                .unwrap_or_else(|| "joint=-".to_string());
            format!("node{}«{}»[{joint}]", s.node_ix, s.name)
        })
        .collect::<Vec<_>>()
        .join(" → ")
}

fn format_skin_joint_in_chain(sj: Option<&SkinJointInChain>) -> String {
    match sj {
        Some(s) => format!(
            "«{}» ui={} node={} (+{} niveles)",
            s.name, s.ui, s.gltf_node, s.steps_from_mesh
        ),
        None => "ninguno en cadena GLTF".to_string(),
    }
}

fn log_rigid_bind_diagnostics(
    label: &str,
    bind_pose_from_ibm: bool,
    _scene_ancestors_added: usize,
    hierarchy_globals: &[Mat4],
    runtime_globals: &[Mat4],
    joint_names: &[String],
    diags: &[RigidBindDiag],
) {
    if diags.is_empty() {
        return;
    }

    let skel_span_hier = skeleton_extent_from_globals(hierarchy_globals);
    let mut offset_max = 0.0f32;
    let mut offset_sum = 0.0f32;
    let mut anchor_counts: HashMap<(usize, String), usize> = HashMap::new();
    let mut anchor_origin_counts: HashMap<GltfJointOrigin, usize> = HashMap::new();
    let mut no_skin_joint_in_chain = 0usize;

    for diag in diags {
        let anchor_name = joint_names
            .get(diag.trace.anchor_ui)
            .cloned()
            .unwrap_or_else(|| diag.trace.anchor_name.clone());
        let mesh_pos = diag.mesh_world_ref;
        let node_origin = mat4_world_translation(diag.mesh_bind_world);
        let joint_hier = hierarchy_globals
            .get(diag.trace.anchor_ui)
            .copied()
            .unwrap_or(Mat4::IDENTITY);
        let joint_pos_hier = mat4_world_translation(joint_hier);
        let joint_rt = runtime_globals
            .get(diag.trace.anchor_ui)
            .copied()
            .unwrap_or(Mat4::IDENTITY);
        let joint_pos_rt = mat4_world_translation(joint_rt);
        let offset_hier = (mesh_pos - joint_pos_hier).length();
        let offset_rt = (mesh_pos - joint_pos_rt).length();
        offset_max = offset_max.max(offset_rt);
        offset_sum += offset_rt;
        *anchor_counts
            .entry((diag.trace.anchor_ui, anchor_name.clone()))
            .or_insert(0) += 1;
        *anchor_origin_counts
            .entry(diag.trace.anchor_origin)
            .or_insert(0) += 1;
        if diag.trace.nearest_skin_joint.is_none() {
            no_skin_joint_in_chain += 1;
        }

        if should_log_rigid_bind_detail(&diag.mesh_name, offset_rt) {
            log_rigid_bind_detail(
                &diag.mesh_name,
                &anchor_name,
                is_valid_rigid_anchor_origin(diag.trace.anchor_origin),
                diag.trace.anchor_depth,
                offset_rt,
                mesh_pos,
            );
        }

        let parent_gltf = diag
            .parent_node_ix
            .map(|p| p.to_string())
            .unwrap_or_else(|| "-".to_string());

        log::debug!(
            "[RIGID_BIND] {label} mesh=«{}» gltf_node={} parent_gltf={parent_gltf} parent=«{}» | \
             cadena={} | anchor=«{anchor_name}» ui={} gltf_node={} origen={} | \
             skin_joint_en_cadena={} | mesh_ref={} node_origin={} anchor_hier={} anchor_rt={} \
             offset_hier={offset_hier:.4}m offset_rt={offset_rt:.4}m | {}",
            diag.mesh_name,
            diag.gltf_node,
            diag.parent_node_name,
            format_rigid_anchor_chain(&diag.trace.chain),
            diag.trace.anchor_ui,
            diag.trace.anchor_gltf_node,
            diag.trace.anchor_origin.label(),
            format_skin_joint_in_chain(diag.trace.nearest_skin_joint.as_ref()),
            format_vec3_xyz(mesh_pos),
            format_vec3_xyz(node_origin),
            format_vec3_xyz(joint_pos_hier),
            format_vec3_xyz(joint_pos_rt),
            diag.trace.reason,
        );
    }

    let n = diags.len() as f32;
    let offset_avg = if n > 0.0 { offset_sum / n } else { 0.0 };
    let scene_ancestor_anchors = anchor_origin_counts
        .get(&GltfJointOrigin::SceneAncestor)
        .copied()
        .unwrap_or(0);
    let dominant = anchor_counts
        .iter()
        .max_by_key(|(_, c)| *c)
        .map(|((ui, name), c)| (*ui, name.clone(), *c));

    if let Some((ui, name, count)) = dominant {
        log::info!(
            "[RIGID_BIND] {label} | rigid={} bind_pose_from_ibm={bind_pose_from_ibm} \
             skel_span_hier={skel_span_hier:.3}m anchor_dominante=«{name}» ui={ui} ({count}/{}) \
             offset_max={offset_max:.3}m offset_avg={offset_avg:.3}m \
             scene_ancestor={scene_ancestor_anchors}/{} sin_skin_joint_en_cadena={no_skin_joint_in_chain}/{}",
            diags.len(),
            diags.len(),
            diags.len(),
            diags.len(),
        );
    }

    if bind_pose_from_ibm {
        log::debug!(
            "[RIGID_BIND] {label} bind_pose_from_ibm | anchor search usa node_to_joint, no IBM; \
             skel_span_hier≈0 → anchor_bind_hier≈(0,0,0); scene_ancestor indica piezas fuera de skin.joints"
        );
    }
}

fn gltf_skinned_vertex_count(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    node_index: usize,
) -> usize {
    let node = match doc.nodes().nth(node_index) {
        Some(n) => n,
        None => return 0,
    };
    let mesh = match node.mesh() {
        Some(m) => m,
        None => return 0,
    };
    mesh.primitives()
        .filter_map(|prim| {
            prim.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()))
                .read_positions()
                .map(|p| p.count())
        })
        .sum()
}

fn read_skinned_gltf_primitive(
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
    material_albedos: &HashMap<usize, gltf::image::Data>,
) -> Option<SkinnedMeshData> {
    let reader = primitive.reader(|buf| Some(&buffers[buf.index()]));
    let positions: Vec<[f32; 3]> = reader.read_positions()?.collect();
    let normals: Vec<[f32; 3]> = reader
        .read_normals()
        .map(|n| n.collect())
        .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);
    let uvs: Vec<[f32; 2]> = reader
        .read_tex_coords(0)
        .map(|tc| tc.into_f32().collect())
        .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);
    let joints_reader = reader.read_joints(0)?;
    let joint_data: Vec<[u16; 4]> = joints_reader.into_u16().collect();
    let weights_reader = reader.read_weights(0)?;
    let weight_data: Vec<[f32; 4]> = weights_reader.into_f32().collect();
    if joint_data.len() != positions.len() || weight_data.len() != positions.len() {
        return None;
    }
    let indices: Vec<u32> = reader
        .read_indices()
        .map(|i| i.into_u32().collect())
        .unwrap_or_else(|| (0..positions.len() as u32).collect());

    let mut vertices = Vec::with_capacity(positions.len());
    for i in 0..positions.len() {
        let j = joint_data.get(i).copied().unwrap_or([0, 0, 0, 0]);
        let (j0, j1, j2, j3) = (j[0] as u32, j[1] as u32, j[2] as u32, j[3] as u32);
        let w = weight_data.get(i).copied().unwrap_or([1.0, 0.0, 0.0, 0.0]);
        let (w0, w1, w2, w3) = (w[0], w[1], w[2], w[3]);
        let sum = w0 + w1 + w2 + w3;
        let (w0, w1, w2, w3) = if sum > 1e-6 {
            (w0 / sum, w1 / sum, w2 / sum, w3 / sum)
        } else {
            (1.0, 0.0, 0.0, 0.0)
        };
        vertices.push(SkinnedVertex {
            position: positions[i],
            normal: normals[i],
            uv: uvs[i],
            joints: [j0, j1, j2, j3],
            weights: [w0, w1, w2, w3],
        });
    }

    let (rgba, width, height) = gltf_texture_rgba(primitive, material_albedos);

    Some(SkinnedMeshData {
        vertices,
        indices,
        rgba,
        width,
        height,
    })
}

/// Malla sin JOINTS_0 (ojos, pestañas, dientes…) anclada 100 % al joint ancestro más cercano.
fn read_gltf_rigid_primitive(
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
    material_albedos: &HashMap<usize, gltf::image::Data>,
    joint_ui: u32,
) -> Option<SkinnedMeshData> {
    let reader = primitive.reader(|buf| Some(&buffers[buf.index()]));
    let positions: Vec<[f32; 3]> = reader.read_positions()?.collect();
    if positions.is_empty() {
        return None;
    }
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
    let vertices: Vec<SkinnedVertex> = positions
        .into_iter()
        .enumerate()
        .map(|(i, position)| SkinnedVertex {
            position,
            normal: normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]),
            uv: uvs.get(i).copied().unwrap_or([0.0, 0.0]),
            joints: [joint_ui, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        })
        .collect();
    let (rgba, width, height) = gltf_texture_rgba(primitive, material_albedos);
    Some(SkinnedMeshData {
        vertices,
        indices,
        rgba,
        width,
        height,
    })
}

fn gltf_texture_rgba(
    primitive: &gltf::Primitive,
    material_albedos: &HashMap<usize, gltf::image::Data>,
) -> (Vec<u8>, u32, u32) {
    let mat_idx = primitive.material().index().unwrap_or(0);
    if let Some(img) = material_albedos.get(&mat_idx) {
        gltf_image_data_to_rgba(img)
    } else {
        (vec![255, 255, 255, 255], 1, 1)
    }
}

fn push_gltf_keyframe(
    keyframes: &mut Vec<AnimKeyframe>,
    property: &AnimProperty,
    t: f32,
    out: [f32; 4],
) {
    let kf = match *property {
        AnimProperty::Translation => AnimKeyframe {
            time: t,
            translation: Some([out[0], out[1], out[2]]),
            rotation: None,
            scale: None,
        },
        AnimProperty::Scale => AnimKeyframe {
            time: t,
            translation: None,
            rotation: None,
            scale: Some([out[0], out[1], out[2]]),
        },
        AnimProperty::Rotation => AnimKeyframe {
            time: t,
            translation: None,
            rotation: Some([out[0], out[1], out[2], out[3]]),
            scale: None,
        },
    };
    keyframes.push(kf);
}

fn parse_gltf_animation(
    anim: &gltf::Animation,
    buffers: &[gltf::buffer::Data],
    node_to_joint: &HashMap<usize, usize>,
) -> Option<AnimationClip> {
    let name = anim
        .name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("Animation_{}", anim.index()));

    let mut channels = Vec::new();
    let mut max_time = 0.0f32;

    for channel in anim.channels() {
        let target_node = channel.target().node().index();
        let joint_index = match node_to_joint.get(&target_node) {
            Some(&ji) => ji,
            None => continue,
        };
        let property = match channel.target().property() {
            gltf::animation::Property::Translation => AnimProperty::Translation,
            gltf::animation::Property::Rotation => AnimProperty::Rotation,
            gltf::animation::Property::Scale => AnimProperty::Scale,
            _ => continue,
        };
        let reader = channel.reader(|b| Some(&buffers[b.index()].0[..]));
        let Some(inputs) = reader.read_inputs() else {
            continue;
        };
        let inputs: Vec<f32> = inputs.collect();
        const MAX_GLTF_KEYS_PER_CHANNEL: usize = 4096;
        let outputs: Vec<[f32; 4]> = match reader.read_outputs() {
            Some(gltf::animation::util::ReadOutputs::Translations(iter))
                if matches!(property, AnimProperty::Translation) =>
            {
                iter.map(|v| [v[0], v[1], v[2], 0.0]).collect()
            }
            Some(gltf::animation::util::ReadOutputs::Scales(iter))
                if matches!(property, AnimProperty::Scale) =>
            {
                iter.map(|v| [v[0], v[1], v[2], 0.0]).collect()
            }
            Some(gltf::animation::util::ReadOutputs::Rotations(
                gltf::animation::util::Rotations::F32(iter),
            )) if matches!(property, AnimProperty::Rotation) => {
                iter.map(|q| [q[0], q[1], q[2], q[3]]).collect()
            }
            _ => continue,
        };

        if inputs.is_empty() || outputs.is_empty() {
            continue;
        }

        let mut keyframes = Vec::new();
        let key_count = inputs.len().min(outputs.len());
        if key_count > MAX_GLTF_KEYS_PER_CHANNEL {
            let step = key_count as f32 / MAX_GLTF_KEYS_PER_CHANNEL as f32;
            for k in 0..MAX_GLTF_KEYS_PER_CHANNEL {
                let i = ((k as f32 * step) as usize).min(key_count - 1);
                let t = inputs[i];
                let out = outputs[i];
                max_time = max_time.max(t);
                push_gltf_keyframe(&mut keyframes, &property, t, out);
            }
        } else {
            for (t, out) in inputs.iter().zip(outputs.iter()) {
                max_time = max_time.max(*t);
                push_gltf_keyframe(&mut keyframes, &property, *t, *out);
            }
        }
        keyframes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
        channels.push(AnimChannel {
            joint_index,
            property,
            keyframes,
        });
    }

    let duration_s = max_time.max(1.0 / 30.0);
    let fps = 30.0;

    Some(AnimationClip {
        name,
        duration_s,
        fps,
        channels,
    })
}

pub(crate) fn model_asset_bind_pose_bounds(asset: &ModelAsset) -> Option<([f32; 3], [f32; 3])> {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for part in &asset.parts {
        let Some((pmin, pmax)) = skinned_mesh_bounds(&part.mesh.vertices) else {
            continue;
        };
        any = true;
        for i in 0..3 {
            min[i] = min[i].min(pmin[i]);
            max[i] = max[i].max(pmax[i]);
        }
    }
    any.then_some((min, max))
}

pub(crate) fn apply_normalize_to_skinned_vertices(vertices: &mut [SkinnedVertex], norm: Mat4) {
    transform_skinned_vertices(vertices, norm);
}

fn transform_skinned_vertices(vertices: &mut [SkinnedVertex], transform: Mat4) {
    let normal_xform = transform.inverse().transpose();
    for v in vertices.iter_mut() {
        v.position = transform
            .transform_point3(Vec3::from_array(v.position))
            .to_array();
        let n = normal_xform
            .transform_vector3(Vec3::from_array(v.normal))
            .normalize_or_zero();
        if n.length_squared() > 1e-8 {
            v.normal = n.to_array();
        }
    }
}

/// Hornea la transform del nodo mesh en vértices, normales e IBM (espacio escena = joints + paleta Khronos).
fn bake_gltf_mesh_bind_world(part: &mut SkinnedMeshPart) {
    let bind = part.mesh_bind_world;
    if bind == Mat4::IDENTITY {
        return;
    }
    let inv_bind = bind.inverse();
    transform_skinned_vertices(&mut part.mesh.vertices, bind);
    for ibm in part.inverse_bind.iter_mut() {
        *ibm = (Mat4::from_cols_array_2d(ibm) * inv_bind).to_cols_array_2d();
    }
    part.mesh_bind_world = Mat4::IDENTITY;
}

fn gltf_skinned_parts_bounds(parts: &[SkinnedMeshPart]) -> Option<([f32; 3], [f32; 3])> {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for part in parts {
        let Some((pmin, pmax)) = skinned_mesh_bounds(&part.mesh.vertices) else {
            continue;
        };
        any = true;
        for i in 0..3 {
            min[i] = min[i].min(pmin[i]);
            max[i] = max[i].max(pmax[i]);
        }
    }
    any.then_some((min, max))
}

fn gltf_skinned_sample_points(parts: &[SkinnedMeshPart]) -> Vec<Vec3> {
    parts
        .iter()
        .flat_map(|part| {
            part.mesh
                .vertices
                .iter()
                .map(|v| Vec3::from_array(v.position))
        })
        .collect()
}

fn find_joint_index_by_name(joint_names: &[String], needles: &[&str]) -> Option<usize> {
    for needle in needles {
        let needle = needle.to_ascii_lowercase();
        if let Some(ix) = joint_names
            .iter()
            .position(|name| name.to_ascii_lowercase().contains(&needle))
        {
            return Some(ix);
        }
    }
    None
}

fn bind_joint_world_positions(
    joint_gltf_nodes: &[usize],
    bind_local: &[Mat4],
    scene_parents: &HashMap<usize, usize>,
    bind_node_local: &HashMap<usize, Mat4>,
) -> Vec<Vec3> {
    compute_gltf_joint_worlds(
        joint_gltf_nodes,
        bind_local,
        scene_parents,
        bind_node_local,
    )
    .into_iter()
    .map(|m| m.transform_point3(Vec3::ZERO))
    .collect()
}

fn upright_quat_from_bind_joints(joint_names: &[String], positions: &[Vec3]) -> Option<Quat> {
    if positions.is_empty() {
        return None;
    }
    let mut up = if let (Some(hip), Some(head)) = (
        find_joint_index_by_name(joint_names, &["hips", "pelvis", "root"]),
        find_joint_index_by_name(joint_names, &["head"]),
    ) {
        positions[head] - positions[hip]
    } else {
        let (min_i, max_i) = positions.iter().enumerate().fold(
            (0usize, 0usize),
            |(min_i, max_i), (i, p)| {
                let min_i = if p.y < positions[min_i].y { i } else { min_i };
                let max_i = if p.y > positions[max_i].y { i } else { max_i };
                (min_i, max_i)
            },
        );
        positions[max_i] - positions[min_i]
    };
    if up.length_squared() < 1e-8 {
        return None;
    }
    up = up.normalize();
    if up.y > 0.999 {
        return Some(Quat::IDENTITY);
    }
    Some(Quat::from_rotation_arc(up, Vec3::Y))
}

fn upright_quat_for_gltf_play_character(
    parts: &[SkinnedMeshPart],
    joint_names: &[String],
    joint_gltf_nodes: &[usize],
    bind_local: &[Mat4],
    scene_parents: &HashMap<usize, usize>,
    bind_node_local: &HashMap<usize, Mat4>,
) -> Option<Quat> {
    let joint_positions = bind_joint_world_positions(
        joint_gltf_nodes,
        bind_local,
        scene_parents,
        bind_node_local,
    );
    if let Some(upright) = upright_quat_from_bind_joints(joint_names, &joint_positions) {
        log::info!(
            "[model_asset] glTF play upright: esqueleto (joints={}) quat=({:.3},{:.3},{:.3},{:.3})",
            joint_positions.len(),
            upright.x,
            upright.y,
            upright.z,
            upright.w
        );
        return Some(upright);
    }
    let (min, max) = gltf_skinned_parts_bounds(parts)?;
    let points = gltf_skinned_sample_points(parts);
    let upright =
        crate::config_3d::mesh_3d::upright_quat_from_vertices_bounds(min, max, &points);
    log::info!(
        "[model_asset] glTF play upright: AABB fallback quat=({:.3},{:.3},{:.3},{:.3})",
        upright.x,
        upright.y,
        upright.z,
        upright.w
    );
    Some(upright)
}

/// Altura y centro en espacio enderezado (malla + esqueleto) para escalar al jugador FP.
fn gltf_play_bind_height_and_center(
    parts: &[SkinnedMeshPart],
    joint_positions: &[Vec3],
    upright: Quat,
) -> Option<(f32, Vec3)> {
    let up_mat = Mat4::from_quat(upright);
    let mut min_u = Vec3::splat(f32::MAX);
    let mut max_u = Vec3::splat(f32::MIN);
    let mut count = 0usize;
    for p in joint_positions {
        let pu = up_mat.transform_point3(*p);
        min_u = min_u.min(pu);
        max_u = max_u.max(pu);
        count += 1;
    }
    for part in parts {
        for v in &part.mesh.vertices {
            let pu = up_mat.transform_point3(Vec3::from_array(v.position));
            min_u = min_u.min(pu);
            max_u = max_u.max(pu);
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    let height = (max_u.y - min_u.y).max(1e-5);
    let center = (min_u + max_u) * 0.5;
    Some((height, center))
}

fn skinned_vertex_position(palette: &[Mat4], vertex: &SkinnedVertex) -> Vec3 {
    let pos = Vec4::new(
        vertex.position[0],
        vertex.position[1],
        vertex.position[2],
        1.0,
    );
    let mut out = Vec4::ZERO;
    for slot in 0..4 {
        let w = vertex.weights[slot];
        if w <= 1e-8 {
            continue;
        }
        let ji = vertex.joints[slot] as usize;
        if ji >= palette.len() {
            continue;
        }
        out += palette[ji] * pos * w;
    }
    out.truncate()
}

/// AABB en bind pose deformado (glTF skinned), espacio local de entidad.
fn gltf_play_skinned_bind_pose_aabb(
    parts: &[SkinnedMeshPart],
    joint_gltf_nodes: &[usize],
    bind_local: &[Mat4],
    scene_parents: &HashMap<usize, usize>,
    bind_node_local: &HashMap<usize, Mat4>,
    mesh_normalize: Mat4,
) -> Option<([f32; 3], [f32; 3])> {
    let joint_count = bind_local.len().min(MAX_JOINTS).min(joint_gltf_nodes.len());
    if joint_count == 0 {
        return None;
    }
    let global = compute_gltf_joint_worlds(
        &joint_gltf_nodes[..joint_count],
        bind_local,
        scene_parents,
        bind_node_local,
    );
    let inv_norm = mesh_normalize.inverse();
    let mut min_p = Vec3::splat(f32::MAX);
    let mut max_p = Vec3::splat(f32::MIN);
    let mut any = false;
    for part in parts {
        let mut palette = vec![Mat4::IDENTITY; MAX_JOINTS];
        for ji in 0..joint_count {
            let g2b = Mat4::from_cols_array_2d(&part.inverse_bind[ji]);
            palette[ji] = mesh_normalize * global[ji] * g2b * inv_norm;
        }
        for vertex in &part.mesh.vertices {
            let p = skinned_vertex_position(&palette, vertex);
            min_p = min_p.min(p);
            max_p = max_p.max(p);
            any = true;
        }
    }
    any.then_some(([min_p.x, min_p.y, min_p.z], [max_p.x, max_p.y, max_p.z]))
}

/// Centro del AABB en bind pose (tras skinning), en espacio local de entidad.
fn gltf_play_bind_pose_aabb_center(
    parts: &[SkinnedMeshPart],
    joint_gltf_nodes: &[usize],
    bind_local: &[Mat4],
    scene_parents: &HashMap<usize, usize>,
    bind_node_local: &HashMap<usize, Mat4>,
    mesh_normalize: Mat4,
) -> Option<Vec3> {
    gltf_play_skinned_bind_pose_aabb(
        parts,
        joint_gltf_nodes,
        bind_local,
        scene_parents,
        bind_node_local,
        mesh_normalize,
    )
    .map(|(min, max)| {
        let min_v = Vec3::from_array(min);
        let max_v = Vec3::from_array(max);
        (min_v + max_v) * 0.5
    })
}

/// AABB del jugador: bind pose skinned o vértices en reposo.
pub(crate) fn model_asset_play_character_visual_bounds(asset: &ModelAsset) -> Option<([f32; 3], [f32; 3])> {
    if !asset.joint_gltf_nodes.is_empty() && !asset.bind_local.is_empty() {
        if let Some(bounds) = gltf_play_skinned_bind_pose_aabb(
            &asset.parts,
            &asset.joint_gltf_nodes,
            &asset.bind_local,
            &asset.gltf_scene_parents,
            &asset.gltf_bind_node_local,
            asset.mesh_normalize,
        ) {
            return Some(bounds);
        }
    }
    model_asset_bind_pose_bounds(asset)
}

fn skinned_mesh_bounds(vertices: &[SkinnedVertex]) -> Option<([f32; 3], [f32; 3])> {
    if vertices.is_empty() {
        return None;
    }
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in vertices.iter() {
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
        }
    }
    Some((min, max))
}

// ── Orientación jugador FP (glTF/GLB) ────────────────────────────────────────

fn torso_vertex_indices(vertices: &[crate::mesh::Vertex]) -> Vec<usize> {
    if vertices.is_empty() {
        return Vec::new();
    }
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in vertices {
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
        }
    }
    let h = (max[1] - min[1]).max(1e-5);
    let w = (max[0] - min[0]).max(1e-5);
    let cx = (min[0] + max[0]) * 0.5;
    let y_lo = min[1] + 0.18 * h;
    let y_hi = min[1] + 0.72 * h;
    let x_half = 0.42 * w;
    vertices
        .iter()
        .enumerate()
        .filter(|(_, v)| {
            let p = &v.position;
            p[1] >= y_lo && p[1] <= y_hi && (p[0] - cx).abs() <= x_half
        })
        .map(|(i, _)| i)
        .collect()
}

fn estimate_forward_from_normals(vertices: &[crate::mesh::Vertex]) -> glam::Vec2 {
    let mut sum = glam::Vec2::ZERO;
    for v in vertices {
        let n = glam::Vec2::new(v.normal[0], v.normal[2]);
        let len = n.length();
        if len > 1e-5 {
            sum += n / len;
        }
    }
    if sum.length_squared() < 1e-6 {
        glam::Vec2::new(0.0, 1.0)
    } else {
        sum.normalize()
    }
}

fn estimate_forward_torso(vertices: &[crate::mesh::Vertex]) -> glam::Vec2 {
    if vertices.is_empty() {
        return glam::Vec2::new(0.0, 1.0);
    }
    let torso_ix = torso_vertex_indices(vertices);
    let positions: Vec<[f32; 3]> = if torso_ix.len() >= 64 {
        torso_ix.iter().map(|&i| vertices[i].position).collect()
    } else {
        vertices.iter().map(|v| v.position).collect()
    };
    crate::config_3d::mesh_3d::estimate_forward_xz_from_positions(&positions)
}

fn find_hips_joint_index(asset: &ModelAsset) -> Option<usize> {
    asset
        .joint_names
        .iter()
        .position(|n| n.to_ascii_lowercase().contains("hips"))
}

fn skinned_forward_core_vertices(asset: &ModelAsset) -> Vec<crate::mesh::Vertex> {
    use crate::mesh::Vertex;
    let mut verts: Vec<Vertex> = asset
        .parts
        .iter()
        .filter(|p| part_counts_for_play_forward(&p.name))
        .flat_map(|p| &p.mesh.vertices)
        .map(|v| Vertex {
            position: v.position,
            normal: v.normal,
            uv: v.uv,
        })
        .collect();
    if verts.len() < 64 {
        verts = asset
            .parts
            .iter()
            .flat_map(|p| &p.mesh.vertices)
            .map(|v| Vertex {
                position: v.position,
                normal: v.normal,
                uv: v.uv,
            })
            .collect();
    }
    verts
}

fn estimate_skinned_forward_from_vertices(asset: &ModelAsset) -> glam::Vec2 {
    let verts = skinned_forward_core_vertices(asset);
    if verts.is_empty() {
        return asset.facing_forward_xz;
    }
    let est_pos = crate::config_3d::mesh_3d::estimate_mesh_forward_xz(&verts);
    let est_norm = estimate_forward_from_normals(&verts);
    if asset.facing_forward_xz.dot(est_norm).abs() >= asset.facing_forward_xz.dot(est_pos).abs() {
        est_norm
    } else {
        est_pos
    }
}

fn estimate_skinned_forward_torso(asset: &ModelAsset) -> glam::Vec2 {
    let verts = skinned_forward_core_vertices(asset);
    if verts.is_empty() {
        return asset.facing_forward_xz;
    }
    estimate_forward_torso(&verts)
}

fn needs_skinned_forward_correction(asset: &ModelAsset) -> bool {
    let Some(hi) = find_hips_joint_index(asset) else {
        return false;
    };
    let (_, rot, _) = asset.bind_local[hi]
        .to_scale_rotation_translation();
    let (bone_yaw, _, _) = rot.to_euler(glam::EulerRot::YXZ);
    bone_yaw.abs() >= 0.35
}

/// Compensa yaw del hueso raíz (Mixamo suele traer ~±90° en bind).
fn apply_hips_bind_yaw_to_forward(fwd: glam::Vec2, asset: &ModelAsset) -> glam::Vec2 {
    let Some(hi) = find_hips_joint_index(asset) else {
        return fwd;
    };
    let (_, rot, _) = asset.bind_local[hi]
        .to_scale_rotation_translation();
    let (bone_yaw, _, _) = rot.to_euler(glam::EulerRot::YXZ);
    if bone_yaw.abs() < 0.35 {
        return fwd;
    }
    let (s, c) = (-bone_yaw).sin_cos();
    let fx = fwd.x;
    let fz = fwd.y;
    glam::Vec2::new(fx * c - fz * s, fx * s + fz * c).normalize_or_zero()
}

fn part_counts_for_play_forward(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n.is_empty() {
        return true;
    }
    const SKIP: &[&str] = &[
        "hair", "gun", "weapon", "boost", "cape", "wing", "shield", "backpack", "acc",
        "accessory", "prop",
    ];
    !SKIP.iter().any(|s| n.contains(s))
}

fn display_forward_xz(asset: &ModelAsset) -> glam::Vec2 {
    let fwd = estimate_skinned_forward_torso(asset);
    apply_hips_bind_yaw_to_forward(fwd, asset)
}

/// Forward del jugador FP para asset skinned **glTF/GLB** (tras `try_bind`).
pub fn resolve_gltf_play_character_forward_xz(asset: &ModelAsset) -> glam::Vec2 {
    if needs_skinned_forward_correction(asset) {
        return display_forward_xz(asset);
    }
    let meta = asset.facing_forward_xz;
    let est_pos = {
        let verts = skinned_forward_core_vertices(asset);
        if verts.is_empty() {
            meta
        } else {
            crate::config_3d::mesh_3d::estimate_mesh_forward_xz(&verts)
        }
    };
    let est_torso = estimate_skinned_forward_torso(asset);
    if est_pos.dot(est_torso) < -0.5 {
        return crate::config_3d::mesh_3d::resolve_mesh_forward_xz(meta, est_torso);
    }
    let est = estimate_skinned_forward_from_vertices(asset);
    crate::config_3d::mesh_3d::resolve_mesh_forward_xz(meta, est)
}

/// Forward del jugador FP para asset skinned **FBX** (tras `try_bind`).
pub fn resolve_fbx_play_character_forward_xz(asset: &ModelAsset) -> glam::Vec2 {
    if needs_skinned_forward_correction(asset) {
        return display_forward_xz(asset);
    }
    let meta = asset.facing_forward_xz;
    let est_pos = {
        let verts = skinned_forward_core_vertices(asset);
        if verts.is_empty() {
            meta
        } else {
            crate::config_3d::mesh_3d::estimate_mesh_forward_xz(&verts)
        }
    };
    let est_torso = estimate_skinned_forward_torso(asset);
    if est_pos.dot(est_torso) < -0.5 {
        return crate::config_3d::fbx_facing::resolve_fbx_forward_xz(meta, est_torso);
    }
    let est = estimate_skinned_forward_from_vertices(asset);
    crate::config_3d::fbx_facing::resolve_fbx_forward_xz(meta, est)
}

