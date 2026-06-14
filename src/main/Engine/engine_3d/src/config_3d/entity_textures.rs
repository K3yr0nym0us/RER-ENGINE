//! Bootstrap de texturas embebidas al enlazar modelo skinned (import / .rerasset).

use std::path::Path;

use crate::ecs::EntityId;
use crate::engine::State;

/// Tras bind de modelo: empaqueta la textura ya decodificada en import o la capa `.rerasset`.
pub(crate) fn bootstrap_entity_model_textures(
    state: &mut State,
    entity_id: EntityId,
    _model_path: &str,
) {
    let (asset_path, parts_len) = match state.model_animation_bindings.get(&entity_id) {
        Some(b) => (b.asset_path.clone(), b.part_gpu_indices.len()),
        None => return,
    };
    let cache_key_base = state.model_cache_key(&asset_path);
    let imported_ready = state
        .imported_model_registry
        .get(&cache_key_base)
        .is_some_and(|e| e.state == rer_engine_shared::assets::AssetState::Ready);
    let library_path = state.model_library_path_for(&asset_path);
    let catalog_path = state.model_path_key(&library_path);

    let asset = match state.get_model_asset(&asset_path) {
        Some(a) => a,
        None => return,
    };
    if asset.parts.len() != parts_len {
        return;
    }

    let mut layer_updates: Vec<(usize, crate::texture::TextureLayer)> =
        Vec::with_capacity(asset.parts.len());
    for (pi, part) in asset.parts.iter().enumerate() {
        let layer = if imported_ready {
            state.ensure_imported_material_texture_layer(&cache_key_base, part.material_index)
        } else if crate::config_3d::is_gltf_model_path(&catalog_path)
            && Path::new(&library_path).is_file()
        {
            pack_skinned_part_embedded_texture(state, &catalog_path, pi, part)
        } else {
            None
        };
        if let Some(layer) = layer {
            layer_updates.push((pi, layer));
        }
    }

    if let Some(binding) = state.model_animation_bindings.get_mut(&entity_id) {
        if binding.part_tex_layers.len() < asset.parts.len() {
            binding
                .part_tex_layers
                .resize(asset.parts.len(), state.fallback_layer);
        }
        for (pi, layer) in layer_updates {
            binding.part_tex_layers[pi] = layer;
        }
        binding.tex_layer = binding
            .part_tex_layers
            .first()
            .copied()
            .unwrap_or(state.fallback_layer);
    }
    if let Some(layer) = state
        .model_animation_bindings
        .get(&entity_id)
        .and_then(|b| b.part_tex_layers.first())
        .copied()
    {
        let tex_idx = ensure_tex_idx_for_layer(state, layer);
        if let Some(mc) = state.world.get_mut::<crate::ecs::MeshComponent>(entity_id) {
            mc.tex_idx = tex_idx;
        }
    }
}

fn ensure_tex_idx_for_layer(state: &mut State, layer: crate::texture::TextureLayer) -> usize {
    if let Some(idx) = state.tex_layers.iter().position(|&l| l == layer) {
        return idx;
    }
    state.tex_layers.push(layer);
    state.tex_layers.len() - 1
}

fn pack_skinned_part_embedded_texture(
    state: &mut State,
    catalog_path: &str,
    part_index: usize,
    part: &crate::config_3d::model_asset::SkinnedMeshPart,
) -> Option<crate::texture::TextureLayer> {
    if part.mesh.width <= 1 && part.mesh.height <= 1 {
        return None;
    }
    let cache_key = format!("{catalog_path}#part{part_index}");
    Some(state.pack_texture_layer(
        Some(&cache_key),
        &part.mesh.rgba,
        part.mesh.width,
        part.mesh.height,
    ))
}
