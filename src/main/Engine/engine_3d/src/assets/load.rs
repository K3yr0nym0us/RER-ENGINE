//! Carga CPU desde `.rerasset` → mallas listas para GPU + `ModelAsset` skinned.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use rer_engine_shared::assets::{ChunkType, RerassetFile, read_rerasset};

use crate::config_3d::mesh_3d::{CpuModelMeshPart, prepare_cpu_parts_textures_for_gpu};
use crate::config_3d::model_asset::{
    MaterialTextureCpu, ModelAsset, SkinnedMeshData, SkinnedMeshPart, empty_rgba_placeholder,
};
use crate::mesh::{SkinnedVertex, Vertex};
use crate::texture::layer_mip_chain_valid_for_array;

use super::model_asset_blob::{deserialize_animation_clip, deserialize_skeleton};

pub struct LoadedRerassetCpu {
    pub editor_parts: Vec<CpuModelMeshPart>,
    pub play_parts: Option<Vec<CpuModelMeshPart>>,
    pub anim_asset: Option<Arc<ModelAsset>>,
    /// `material_index` GLB → índice de chunk de textura en el `.rerasset`.
    pub material_tex_chunks: HashMap<u32, u32>,
}

/// Mapa material GLB → chunk de textura desde chunks `Material` del `.rerasset`.
pub fn material_texture_chunk_map(file: &RerassetFile) -> HashMap<u32, u32> {
    let mut map = HashMap::new();
    for entry in file
        .chunks
        .iter()
        .filter(|c| c.chunk_type == ChunkType::Material)
    {
        let Ok(data) = file.chunk_data(entry) else {
            continue;
        };
        if data.len() < 8 {
            continue;
        }
        let mat_idx = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let tex_idx = u32::from_le_bytes(data[4..8].try_into().unwrap());
        map.insert(mat_idx, tex_idx);
    }
    map
}

fn rtex_to_material_texture(
    file: &RerassetFile,
    texture_chunk_index: u32,
) -> Arc<MaterialTextureCpu> {
    let entry = file
        .chunks
        .iter()
        .find(|c| c.chunk_type == ChunkType::Texture && c.chunk_index == texture_chunk_index)
        .expect("texture chunk missing");
    let rtex = file.read_texture(entry).expect("rtex read");
    if layer_mip_chain_valid_for_array(&rtex.mips) {
        Arc::new(MaterialTextureCpu {
            rgba: empty_rgba_placeholder(),
            width: rtex.width,
            height: rtex.height,
            layer_mips: Some(Arc::new(rtex.mips)),
        })
    } else {
        let rgba = rtex
            .mips
            .first()
            .cloned()
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| vec![255, 255, 255, 255]);
        Arc::new(MaterialTextureCpu {
            rgba: Arc::from(rgba),
            width: rtex.width,
            height: rtex.height,
            layer_mips: None,
        })
    }
}

fn material_texture_for_index(file: &RerassetFile, material_index: u32) -> Arc<MaterialTextureCpu> {
    if let Some(entry) = file
        .chunks
        .iter()
        .find(|c| c.chunk_type == ChunkType::Material && c.chunk_index == material_index)
    {
        let data = file.chunk_data(entry).expect("material chunk");
        let tex_idx = u32::from_le_bytes(data[4..8].try_into().unwrap());
        return rtex_to_material_texture(file, tex_idx);
    }
    rtex_to_material_texture(file, 0)
}

fn load_static_parts(
    file: &RerassetFile,
    vert_type: ChunkType,
    idx_type: ChunkType,
) -> Result<Vec<CpuModelMeshPart>, String> {
    let vert_entries: Vec<_> = file.chunks_of_type(vert_type);
    let mut parts = Vec::with_capacity(vert_entries.len());

    for vert_entry in vert_entries {
        let part_index = vert_entry.chunk_index;
        let idx_entry = file
            .chunks
            .iter()
            .find(|c| c.chunk_type == idx_type && c.chunk_index == part_index)
            .ok_or_else(|| format!("índices faltantes para parte {part_index}"))?;

        let meta_entry = file
            .chunks
            .iter()
            .find(|c| c.chunk_type == ChunkType::MeshPartMeta && c.chunk_index == part_index);

        let (material_index, forward_xz, local_bounds) = if let Some(meta) = meta_entry {
            let data = file.chunk_data(meta).map_err(|e| e.to_string())?;
            let material_index = u32::from_le_bytes(data[4..8].try_into().unwrap());
            let forward_xz = [
                f32::from_le_bytes(data[8..12].try_into().unwrap()),
                f32::from_le_bytes(data[12..16].try_into().unwrap()),
            ];
            let mut min = [0f32; 3];
            let mut max = [0f32; 3];
            for i in 0..3 {
                min[i] = f32::from_le_bytes(data[16 + i * 4..20 + i * 4].try_into().unwrap());
                max[i] = f32::from_le_bytes(data[28 + i * 4..32 + i * 4].try_into().unwrap());
            }
            (material_index, forward_xz, (min, max))
        } else {
            (0, [0.0, 1.0], ([0.0; 3], [0.0; 3]))
        };

        let static_verts = file
            .read_editor_vertices(vert_entry)
            .map_err(|e| e.to_string())?;
        let vertices: Vec<Vertex> = static_verts
            .iter()
            .map(|v| Vertex {
                position: v.position,
                normal: v.normal,
                uv: v.uv,
            })
            .collect();
        let indices = file.read_indices(idx_entry).map_err(|e| e.to_string())?;
        let texture = material_texture_for_index(file, material_index);

        parts.push(CpuModelMeshPart {
            vertices,
            indices,
            material_index,
            texture,
            forward_xz: glam::Vec2::new(forward_xz[0], forward_xz[1]),
            local_bounds,
            roughness: -1.0,
            metallic: 0.0,
            ior: 0.0,
        });
    }

    Ok(parts)
}

fn load_skinned_mesh_stubs(file: &RerassetFile) -> Result<Vec<SkinnedMeshPart>, String> {
    let mut vert_entries: Vec<_> = file.chunks_of_type(ChunkType::MeshSkinnedVert);
    vert_entries.sort_by_key(|e| e.chunk_index);
    let mut out = Vec::with_capacity(vert_entries.len());

    for vert_entry in vert_entries {
        let part_index = vert_entry.chunk_index;
        let idx_entry = file
            .chunks
            .iter()
            .find(|c| c.chunk_type == ChunkType::MeshSkinnedIdx && c.chunk_index == part_index)
            .ok_or_else(|| format!("skinned idx faltante parte {part_index}"))?;

        let skinned_verts = file
            .read_skinned_vertices(vert_entry)
            .map_err(|e| e.to_string())?;
        let vertices: Vec<SkinnedVertex> = skinned_verts
            .iter()
            .map(|v| SkinnedVertex {
                position: v.position,
                normal: v.normal,
                uv: v.uv,
                joints: v.joints,
                weights: v.weights,
            })
            .collect();
        let indices = file.read_indices(idx_entry).map_err(|e| e.to_string())?;

        let meta = file
            .chunks
            .iter()
            .find(|c| c.chunk_type == ChunkType::MeshPartMeta && c.chunk_index == part_index);
        let material_index = meta
            .and_then(|m| {
                file.chunk_data(m)
                    .ok()
                    .map(|d| u32::from_le_bytes(d[4..8].try_into().unwrap()))
            })
            .unwrap_or(0);

        let tex = material_texture_for_index(file, material_index);
        let rgba = tex.effective_rgba().to_vec();
        let width = tex.width;
        let height = tex.height;
        out.push(SkinnedMeshPart {
            name: String::new(),
            material_index,
            mesh: SkinnedMeshData {
                vertices,
                indices,
                rgba,
                width,
                height,
            },
            mesh_bind_world: glam::Mat4::IDENTITY,
            inverse_bind: vec![],
        });
    }
    Ok(out)
}

fn load_anim_asset(file: &RerassetFile) -> Result<Option<Arc<ModelAsset>>, String> {
    let skel_entry = file
        .chunks
        .iter()
        .find(|c| c.chunk_type == ChunkType::Skeleton)
        .cloned();
    let Some(skel_entry) = skel_entry else {
        return Ok(None);
    };
    let skel_bytes = file.chunk_data(&skel_entry).map_err(|e| e.to_string())?;
    let skinned_stubs = load_skinned_mesh_stubs(file)?;
    let mut asset = deserialize_skeleton(skel_bytes, skinned_stubs)?;

    let mut clip_entries: Vec<_> = file.chunks_of_type(ChunkType::AnimClip);
    clip_entries.sort_by_key(|e| e.chunk_index);
    for entry in clip_entries {
        let data = file.chunk_data(entry).map_err(|e| e.to_string())?;
        let clip = deserialize_animation_clip(data)?;
        asset.clips.push(clip);
    }
    Ok(Some(Arc::new(asset)))
}

pub fn load_rerasset_cpu(path: &Path) -> Result<LoadedRerassetCpu, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let file = read_rerasset(&bytes).map_err(|e| e.to_string())?;
    let material_tex_chunks = material_texture_chunk_map(&file);

    let mut editor_parts =
        load_static_parts(&file, ChunkType::MeshEditorVert, ChunkType::MeshEditorIdx)?;
    let mut play_parts = if file
        .header
        .flags
        .contains(rer_engine_shared::assets::AssetFlags::HAS_PLAY_CHARACTER)
    {
        Some(load_static_parts(
            &file,
            ChunkType::MeshPlayVert,
            ChunkType::MeshPlayIdx,
        )?)
    } else {
        None
    };
    prepare_cpu_parts_textures_for_gpu(&mut editor_parts);
    if let Some(play) = play_parts.as_mut() {
        prepare_cpu_parts_textures_for_gpu(play);
    }
    let anim_asset = load_anim_asset(&file)?;

    let loaded = LoadedRerassetCpu {
        editor_parts,
        play_parts,
        anim_asset,
        material_tex_chunks,
    };
    Ok(loaded)
}
