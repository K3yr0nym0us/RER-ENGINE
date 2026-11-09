//! Modelo de datos del asset: clips, mallas skinned, materiales y `ModelAsset`.

use super::*;

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
    /// glTF: padre en la escena y local de bind (cadena hasta raíz).
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
