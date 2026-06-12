//! Bake de modelos CPU → `.rerasset` binario.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use rer_engine_shared::assets::{
    BakeInput, BakeMeshPart, BakeSkinnedPart, MaterialDesc, RER_IMPORTER_VERSION, RtexData,
    SourceExt, TextureFormat, CompressionType, write_rerasset_atomic,
};

use crate::config_3d::mesh_3d::CpuModelMeshPart;
use crate::config_3d::model_asset::{MaterialTextureCpu, ModelAsset};
use crate::mesh::{SkinnedVertex, Vertex};

fn source_ext_from_path(path: &Path) -> SourceExt {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "gltf" => SourceExt::Gltf,
        "fbx" => SourceExt::Fbx,
        _ => SourceExt::Glb,
    }
}

fn category_to_u8(category: Option<&str>) -> u8 {
    match category {
        Some("character") => 0,
        Some("environment") => 1,
        _ => 2,
    }
}

fn texture_to_rtex(tex: &MaterialTextureCpu) -> RtexData {
    let mips = if let Some(chain) = &tex.layer_mips {
        chain.iter().cloned().collect()
    } else {
        vec![tex.rgba.to_vec()]
    };
    RtexData {
        width: tex.width,
        height: tex.height,
        texture_format: TextureFormat::Rgba8UnormSrgb,
        compression_type: CompressionType::None,
        mips,
    }
}

fn collect_textures_from_editor_and_play(
    editor_parts: &[CpuModelMeshPart],
    play_parts: Option<&[CpuModelMeshPart]>,
) -> (Vec<RtexData>, Vec<MaterialDesc>) {
    let mut tex_by_material: HashMap<u32, Arc<MaterialTextureCpu>> = HashMap::new();
    for part in editor_parts.iter().chain(play_parts.into_iter().flatten()) {
        tex_by_material
            .entry(part.material_index)
            .or_insert_with(|| Arc::clone(&part.texture));
    }
    let mut indices: Vec<u32> = tex_by_material.keys().copied().collect();
    indices.sort_unstable();

    let textures: Vec<RtexData> = indices
        .iter()
        .map(|mi| texture_to_rtex(tex_by_material[mi].as_ref()))
        .collect();

    let materials: Vec<MaterialDesc> = indices
        .iter()
        .enumerate()
        .map(|(ti, mi)| MaterialDesc {
            material_index: *mi,
            texture_chunk_index: ti as u32,
            name: format!("material_{mi}"),
        })
        .collect();

    (textures, materials)
}

fn part_to_bake(part: &CpuModelMeshPart, part_index: u16) -> BakeMeshPart {
    let vertices: Vec<rer_engine_shared::assets::StaticMeshVertex> = part
        .vertices
        .iter()
        .map(|v| rer_engine_shared::assets::StaticMeshVertex {
            position: v.position,
            normal: v.normal,
            uv: v.uv,
        })
        .collect();
    BakeMeshPart {
        part_index,
        material_index: part.material_index,
        forward_xz: [part.forward_xz.x, part.forward_xz.y],
        local_bounds: part.local_bounds,
        vertices,
        indices: part.indices.clone(),
    }
}

fn skinned_part_to_bake(
    name: &str,
    material_index: u32,
    part_index: u16,
    vertices: &[SkinnedVertex],
    indices: &[u32],
) -> BakeSkinnedPart {
    let verts: Vec<rer_engine_shared::assets::SkinnedMeshVertex> = vertices
        .iter()
        .map(|v| rer_engine_shared::assets::SkinnedMeshVertex {
            position: v.position,
            normal: v.normal,
            uv: v.uv,
            joints: v.joints,
            weights: v.weights,
        })
        .collect();
    BakeSkinnedPart {
        part_index,
        name: name.to_string(),
        material_index,
        vertices: verts,
        indices: indices.to_vec(),
    }
}

/// Construye `BakeInput` desde el bundle de precarga CPU.
pub fn build_bake_input(
    source_path: &Path,
    category: Option<&str>,
    editor_parts: &[CpuModelMeshPart],
    play_parts: Option<&[CpuModelMeshPart]>,
    anim_asset: Option<&ModelAsset>,
) -> BakeInput {
    let (source_size, source_mtime_secs) = super::registry::source_fingerprint(source_path);

    let (textures, materials) =
        collect_textures_from_editor_and_play(editor_parts, play_parts);

    let editor_bake: Vec<BakeMeshPart> = editor_parts
        .iter()
        .enumerate()
        .map(|(i, p)| part_to_bake(p, i as u16))
        .collect();

    let play_bake = play_parts.map(|parts| {
        parts
            .iter()
            .enumerate()
            .map(|(i, p)| part_to_bake(p, i as u16))
            .collect::<Vec<_>>()
    });

    let skinned_parts = anim_asset.map(|asset| {
        asset
            .parts
            .iter()
            .enumerate()
            .map(|(i, part)| {
                skinned_part_to_bake(
                    &part.name,
                    part.material_index,
                    i as u16,
                    &part.mesh.vertices,
                    &part.mesh.indices,
                )
            })
            .collect::<Vec<_>>()
    });

    let skeleton = anim_asset.map(|asset| rer_engine_shared::assets::ImportedSkeleton {
        blob: super::model_asset_blob::serialize_skeleton(asset),
    });

    let clips = anim_asset
        .map(|asset| {
            asset
                .clips
                .iter()
                .map(|clip| rer_engine_shared::assets::ImportedAnimationClip {
                    name: clip.name.clone(),
                    duration_s: clip.duration_s,
                    fps: clip.fps,
                    blob: super::model_asset_blob::serialize_animation_clip(clip),
                })
                .collect()
        })
        .unwrap_or_default();

    BakeInput {
        category: category_to_u8(category),
        source_ext: source_ext_from_path(source_path),
        source_size,
        source_mtime_secs,
        source_sha256: None,
        textures,
        materials,
        editor_parts: editor_bake,
        play_parts: play_bake,
        skinned_parts,
        skeleton,
        clips,
    }
}

pub fn bake_to_rerasset(
    rerasset_path: &Path,
    input: &BakeInput,
) -> Result<(), String> {
    write_rerasset_atomic(rerasset_path, input).map_err(|e| e.to_string())?;
    let size = std::fs::metadata(rerasset_path).map(|m| m.len()).unwrap_or(0);
    let skinned = input.skinned_parts.as_ref().map(|p| p.len()).unwrap_or(0);
    let play = input.play_parts.as_ref().map(|p| p.len()).unwrap_or(0);
    log::info!(
        "[RERASSET_BAKE] {} | {} bytes tex={} mat={} parts=e{}/p{}/s{} clips={}",
        rerasset_path.display(),
        size,
        input.textures.len(),
        input.materials.len(),
        input.editor_parts.len(),
        play,
        skinned,
        input.clips.len(),
    );
    Ok(())
}

/// Log PBR + texturas tras bake (requiere ruta GLB para comparar factores).
pub fn log_bake_material_summary(gltf_source: &Path, input: &BakeInput) {
    let skinned = super::log_tex::skinned_material_set_from_parts(input.skinned_parts.as_deref());
    super::log_tex::log_bake_materials_with_gltf(
        gltf_source,
        &input.materials,
        &input.textures,
        &skinned,
    );
}

pub fn current_importer_version() -> u16 {
    RER_IMPORTER_VERSION
}

// Silence unused import warning for Vertex used in type inference paths
#[allow(dead_code)]
fn _vertex_layout_check(_: &Vertex) {}
