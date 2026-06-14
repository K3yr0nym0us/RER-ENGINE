//! Texturas embebidas GLB: tier global, selección por tope de resolución y LOD por distancia.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::config_3d::texture_graphics::{
    self, TextureGraphicsTier, DEFAULT_TEXTURE_DETAIL_NEAR_M,
};
use crate::ecs::{EntityId, Transform};
use crate::engine::State;
use crate::entity_save_meta::is_model_3d_asset_path;
use crate::ipc::{send_event, EngineEvent};

#[derive(Debug, Clone)]
pub struct MaterialTexturesCatalog {
    pub material_index: u32,
    #[allow(dead_code)]
    pub material_name: String,
    pub default_image_index: Option<u32>,
    pub variants: Vec<(u32, u32, u32)>,
}

fn build_material_variants(indices: &HashSet<u32>, all_variants: &[(u32, u32, u32)]) -> Vec<(u32, u32, u32)> {
    let mut by_size: HashMap<(u32, u32), (u32, u32, u32)> = HashMap::new();
    for &(idx, w, h) in all_variants {
        if indices.contains(&idx) {
            by_size.entry((w, h)).or_insert((idx, w, h));
        }
    }
    let mut out: Vec<_> = by_size.into_values().collect();
    out.sort_by_key(|(_, w, h)| w * h);
    out
}

pub fn catalog_gltf_embedded_textures(path: &Path) -> Result<Vec<MaterialTexturesCatalog>, String> {
    if !crate::config_3d::is_gltf_model_path(path.to_string_lossy().as_ref()) {
        return Err("Solo modelos GLB/GLTF".to_string());
    }
    let gltf = gltf::Gltf::open(path).map_err(|e| format!("gltf error: {e}"))?;
    let doc = gltf.document;
    let buffers = gltf::import_buffers(&doc, path.parent(), gltf.blob)
        .map_err(|e| format!("buffers: {e}"))?;

    let mut all_variants: Vec<(u32, u32, u32)> = Vec::new();
    for image in doc.images() {
        let idx = image.index() as u32;
        if let Some((w, h)) = crate::config_3d::gltf_texture_load::peek_gltf_image_dimensions(
            image,
            &buffers,
            path.parent(),
        ) {
            all_variants.push((idx, w, h));
        }
    }
    all_variants.sort_by_key(|(_, w, h)| w * h);
    let mut all_by_image_index = all_variants.clone();
    all_by_image_index.sort_by_key(|(idx, _, _)| *idx);

    let mut image_labels: HashMap<u32, String> = HashMap::new();
    for image in doc.images() {
        image_labels.insert(
            image.index() as u32,
            crate::config_3d::gltf_texture_load::gltf_image_search_label(image),
        );
    }

    let mut materials = Vec::new();
    for (mat_idx, mat) in doc.materials().enumerate() {
        let default_image_index = mat
            .pbr_metallic_roughness()
            .base_color_texture()
            .map(|info| info.texture().source().index() as u32);
        let name = mat
            .name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Material {}", mat_idx));
        let indices = crate::config_3d::gltf_texture_load::discover_material_image_indices(
            &doc,
            mat_idx,
            &name,
            &image_labels,
            &all_variants,
            &all_by_image_index,
        );
        let variants = build_material_variants(&indices, &all_variants);
        materials.push(MaterialTexturesCatalog {
            material_index: mat_idx as u32,
            material_name: name,
            default_image_index,
            variants,
        });
    }

    if materials.is_empty() && !all_variants.is_empty() {
        materials.push(MaterialTexturesCatalog {
            material_index: 0,
            material_name: "Default".to_string(),
            default_image_index: all_variants.first().map(|(i, _, _)| *i),
            variants: all_variants,
        });
    }

    Ok(materials)
}

pub(crate) fn bootstrap_entity_model_textures(
    state: &mut State,
    entity_id: EntityId,
    _model_path: &str,
) {
    state.refresh_entity_textures_with_distance(entity_id);
}

impl State {
    pub(crate) fn entity_model_path_for_textures(&self, id: EntityId) -> Option<String> {
        let visual = self.entity_asset_path_for_bounds(id)?;
        if is_model_3d_asset_path(&visual) {
            Some(visual)
        } else {
            None
        }
    }

    fn catalog_for_path(&mut self, path: &str) -> Result<Vec<MaterialTexturesCatalog>, String> {
        if let Some(cached) = self.glb_texture_catalog_cache.get(path) {
            return Ok(cached.clone());
        }
        let catalog = catalog_gltf_embedded_textures(Path::new(path))?;
        self.glb_texture_catalog_cache
            .insert(path.to_string(), catalog.clone());
        Ok(catalog)
    }

    fn effective_texture_cap_for_entity(&self, entity_id: EntityId) -> u32 {
        let camera = self.camera_world_position();
        let distance = self
            .world
            .get::<Transform>(entity_id)
            .map(|t| (t.position - camera).length())
            .unwrap_or(0.0);
        let base = self.graphics_texture_tier.max_dimension();
        texture_graphics::distance_adjusted_cap(base, distance, self.texture_detail_near_m)
    }

    pub(crate) fn refresh_entity_textures_with_distance(&mut self, entity_id: EntityId) {
        let cap = self.effective_texture_cap_for_entity(entity_id);
        self.entity_texture_effective_cap.insert(entity_id, cap);
        self.apply_entity_textures_at_cap(entity_id, cap);
    }

    pub(crate) fn refresh_all_entity_textures(&mut self) {
        let ids: Vec<EntityId> = self
            .model_animation_bindings
            .keys()
            .copied()
            .collect();
        self.entity_texture_effective_cap.clear();
        for id in ids {
            self.refresh_entity_textures_with_distance(id);
        }
    }

    pub(crate) fn set_graphics_texture_tier(&mut self, tier: TextureGraphicsTier) {
        if self.graphics_texture_tier == tier {
            return;
        }
        self.graphics_texture_tier = tier;
        self.script_engine
            .sync_graphics_texture_tier_readback(tier.wire());
        send_event(&EngineEvent::GraphicsTextureTierChanged {
            tier: tier.wire().to_string(),
        });
        self.refresh_all_entity_textures();
    }

    pub(crate) fn set_texture_detail_near_m(&mut self, distance_m: f32) {
        let clamped = distance_m.clamp(1.0, 500.0);
        if (self.texture_detail_near_m - clamped).abs() < f32::EPSILON {
            return;
        }
        self.texture_detail_near_m = clamped;
        send_event(&EngineEvent::TextureDetailDistanceChanged {
            distance_m: clamped,
        });
        self.refresh_all_entity_textures();
    }

    pub(crate) fn update_texture_distance_lod(&mut self) {
        const MIN_INTERVAL: Duration = Duration::from_millis(250);
        if self.texture_lod_last_update.elapsed() < MIN_INTERVAL {
            return;
        }
        self.texture_lod_last_update = Instant::now();

        let ids: Vec<EntityId> = self
            .model_animation_bindings
            .keys()
            .copied()
            .collect();
        for id in ids {
            if self.entity_model_path_for_textures(id).is_none() {
                continue;
            }
            let cap = self.effective_texture_cap_for_entity(id);
            if self.entity_texture_effective_cap.get(&id) == Some(&cap) {
                continue;
            }
            self.entity_texture_effective_cap.insert(id, cap);
            self.apply_entity_textures_at_cap(id, cap);
        }
    }

    pub(crate) fn apply_entity_textures_at_cap(&mut self, entity_id: EntityId, cap_px: u32) {
        let (asset_path, parts_len) = match self.model_animation_bindings.get(&entity_id) {
            Some(b) => (b.asset_path.clone(), b.part_gpu_indices.len()),
            None => return,
        };
        let cache_key_base = self.model_cache_key(&asset_path);
        let imported_ready = self
            .imported_model_registry
            .get(&cache_key_base)
            .is_some_and(|e| e.state == rer_engine_shared::assets::AssetState::Ready);
        let library_path = self.model_library_path_for(&asset_path);
        let catalog_path = self.model_path_key(&library_path);
        let is_gltf_source = crate::config_3d::is_gltf_model_path(&catalog_path)
            && Path::new(&library_path).is_file();

        let asset = match self.get_model_asset(&asset_path) {
            Some(a) => a,
            None => return,
        };
        if asset.parts.len() != parts_len {
            return;
        }

        let catalog = if is_gltf_source && !imported_ready {
            self.catalog_for_path(&catalog_path).ok()
        } else {
            None
        };

        let mut layer_updates: Vec<(usize, crate::texture::TextureLayer)> =
            Vec::with_capacity(asset.parts.len());
        for (pi, part) in asset.parts.iter().enumerate() {
            let layer = if imported_ready {
                self.ensure_imported_material_texture_layer(&cache_key_base, part.material_index)
            } else if is_gltf_source {
                catalog
                    .as_ref()
                    .and_then(|catalog| {
                        self.resolve_gltf_material_texture_layer_for_cap(
                            &catalog_path,
                            part.material_index,
                            catalog,
                            cap_px,
                        )
                    })
                    .or_else(|| {
                        pack_skinned_part_embedded_texture(self, &catalog_path, pi, part)
                    })
            } else {
                pack_skinned_part_embedded_texture(self, &catalog_path, pi, part)
            };
            if let Some(layer) = layer {
                layer_updates.push((pi, layer));
            }
        }

        if let Some(binding) = self.model_animation_bindings.get_mut(&entity_id) {
            if binding.part_tex_layers.len() < asset.parts.len() {
                binding
                    .part_tex_layers
                    .resize(asset.parts.len(), self.fallback_layer);
            }
            for (pi, layer) in layer_updates {
                binding.part_tex_layers[pi] = layer;
            }
            binding.tex_layer = binding
                .part_tex_layers
                .first()
                .copied()
                .unwrap_or(self.fallback_layer);
        }
        if let Some(layer) = self
            .model_animation_bindings
            .get(&entity_id)
            .and_then(|b| b.part_tex_layers.first())
            .copied()
        {
            let tex_idx = self.ensure_tex_idx_for_layer(layer);
            if let Some(mc) = self.world.get_mut::<crate::ecs::MeshComponent>(entity_id) {
                mc.tex_idx = tex_idx;
            }
        }
    }

    fn ensure_tex_idx_for_layer(&mut self, layer: crate::texture::TextureLayer) -> usize {
        if let Some(idx) = self.tex_layers.iter().position(|&l| l == layer) {
            return idx;
        }
        self.tex_layers.push(layer);
        self.tex_layers.len() - 1
    }

    fn resolve_gltf_material_texture_layer_for_cap(
        &mut self,
        path: &str,
        material_index: u32,
        catalog: &[MaterialTexturesCatalog],
        cap_px: u32,
    ) -> Option<crate::texture::TextureLayer> {
        let mat_catalog = catalog
            .iter()
            .find(|m| m.material_index == material_index)?;
        let image_index = texture_graphics::pick_image_index_for_cap(&mat_catalog.variants, cap_px)
            .or(mat_catalog.default_image_index)?;

        let cache_key = format!("{path}#m{material_index}#img{image_index}");
        if !self.texture_path_layers.contains_key(&cache_key) {
            let path_buf = Path::new(path);
            let gltf = gltf::Gltf::open(path_buf).ok()?;
            let doc = gltf.document;
            let buffers = gltf::import_buffers(&doc, path_buf.parent(), gltf.blob).ok()?;
            let img = crate::config_3d::gltf_texture_load::decode_gltf_image_at_index(
                &doc,
                &buffers,
                path_buf.parent(),
                image_index as usize,
            )
            .map_err(|e| {
                send_event(&EngineEvent::Error { message: e });
            })
            .ok()?;
            let (rgba, w, h) = crate::config_3d::model_asset::gltf_image_data_to_rgba(&img);
            let _ = self.pack_texture_layer(Some(&cache_key), &rgba, w, h);
        }
        self.texture_path_layers.get(&cache_key).copied()
    }
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

pub(crate) fn apply_graphics_settings_from_world_wire(
    state: &mut State,
    tier: Option<&str>,
    detail_distance_m: Option<f32>,
) {
    if let Some(t) = tier.and_then(TextureGraphicsTier::from_wire) {
        state.graphics_texture_tier = t;
        state
            .script_engine
            .sync_graphics_texture_tier_readback(t.wire());
    }
    if let Some(d) = detail_distance_m {
        state.texture_detail_near_m = d.clamp(1.0, 500.0);
    }
}

pub(crate) fn default_texture_detail_near_m() -> f32 {
    DEFAULT_TEXTURE_DETAIL_NEAR_M
}
