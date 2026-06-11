// Catálogo de texturas embebidas GLB/GLTF por material y asignación por nivel gráfico.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Serialize;

use crate::ecs::EntityId;
use crate::engine::State;
use crate::entity_save_meta::is_model_3d_asset_path;
use crate::ipc::{
    send_event, EngineEvent, SaveEntityTextureLodSnapshot, SaveMaterialTextureLod,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureGraphicsTier {
    Low,
    Medium,
    High,
    Ultra,
}

impl TextureGraphicsTier {
    pub fn from_wire(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "low" | "bajo" => Some(Self::Low),
            "medium" | "medio" => Some(Self::Medium),
            "high" | "alto" => Some(Self::High),
            "ultra" => Some(Self::Ultra),
            _ => None,
        }
    }

    pub fn wire(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }
}

#[derive(Clone, Debug)]
pub struct EntityTextureLodState {
    pub preview_tier: TextureGraphicsTier,
    pub active_material_index: u32,
    /// material_index → tier → image_index embebido en el GLB
    pub assignments: HashMap<u32, HashMap<TextureGraphicsTier, u32>>,
}

impl Default for EntityTextureLodState {
    fn default() -> Self {
        Self {
            preview_tier: TextureGraphicsTier::Low,
            active_material_index: 0,
            assignments: HashMap::new(),
        }
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedTextureVariantWire {
    pub image_index: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EntityMaterialTexturesWire {
    pub material_index: u32,
    pub material_name: String,
    pub default_image_index: Option<u32>,
    pub variants: Vec<EmbeddedTextureVariantWire>,
    pub tier_image_index: HashMap<String, u32>,
    pub preview_tier: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityTexturesReadyWire {
    pub event: &'static str,
    pub entity_id: u32,
    pub model_path: String,
    pub materials: Vec<EntityMaterialTexturesWire>,
    pub active_tier: String,
}

#[derive(Debug, Clone)]
pub struct MaterialTexturesCatalog {
    pub material_index: u32,
    pub material_name: String,
    pub default_image_index: Option<u32>,
    pub variants: Vec<(u32, u32, u32)>,
}

/// Variantes del material (una entrada por resolución); misma fuente que apply GPU.
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

fn image_index_belongs_to_material(
    image_index: u32,
    mat_catalog: &MaterialTexturesCatalog,
) -> bool {
    mat_catalog
        .variants
        .iter()
        .any(|(idx, _, _)| *idx == image_index)
}

/// Materiales del GLB + imágenes embebidas (solo metadata; no depende de GPU).
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
        let Ok(bytes) = crate::config_3d::gltf_texture_load::gltf_image_encoded_bytes(
            image,
            &buffers,
            path.parent(),
        ) else {
            continue;
        };
        if let Some((w, h)) = crate::config_3d::gltf_texture_load::peek_encoded_dimensions(&bytes) {
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

pub(crate) fn entity_texture_lod_to_snapshot(
    lod: &EntityTextureLodState,
) -> Option<SaveEntityTextureLodSnapshot> {
    if lod.assignments.is_empty() {
        return None;
    }
    let materials = lod
        .assignments
        .iter()
        .map(|(mat_idx, tiers)| SaveMaterialTextureLod {
            material_index: *mat_idx,
            tier_image_index: tiers
                .iter()
                .map(|(t, idx)| (t.wire().to_string(), *idx))
                .collect(),
        })
        .collect();
    Some(SaveEntityTextureLodSnapshot {
        preview_tier: lod.preview_tier.wire().to_string(),
        active_material_index: lod.active_material_index,
        materials,
    })
}

/// Tras bind de modelo: cada pieza skinned usa la variante menor de su material.
pub(crate) fn bootstrap_entity_model_textures(
    state: &mut State,
    entity_id: EntityId,
    model_path: &str,
) {
    let key = state.model_path_key(model_path);
    if crate::config_3d::is_gltf_model_path(&key) {
        if let Ok(catalog) = state.catalog_for_path(&key) {
            state.ensure_entity_texture_lod_defaults(entity_id, &catalog);
        }
    }
    state.apply_skinned_part_texture_layers(entity_id, state.graphics_texture_tier, None);
}

pub(crate) fn apply_entity_texture_lod_snapshot(
    state: &mut State,
    entity_id: EntityId,
    snap: &SaveEntityTextureLodSnapshot,
) {
    let preview_tier = TextureGraphicsTier::from_wire(&snap.preview_tier)
        .unwrap_or(TextureGraphicsTier::Low);
    let catalog = state
        .entity_model_path_for_textures(entity_id)
        .and_then(|path| state.catalog_for_path(&path).ok());
    let mut assignments = HashMap::new();
    for mat in &snap.materials {
        let mut tier_map = HashMap::new();
        let mat_catalog = catalog.as_ref().and_then(|c| {
            c.iter().find(|m| m.material_index == mat.material_index)
        });
        for (tier_str, img_idx) in &mat.tier_image_index {
            let Some(t) = TextureGraphicsTier::from_wire(tier_str) else {
                continue;
            };
            if let Some(mc) = mat_catalog {
                if !image_index_belongs_to_material(*img_idx, mc) {
                    continue;
                }
            }
            tier_map.insert(t, *img_idx);
        }
        if !tier_map.is_empty() {
            assignments.insert(mat.material_index, tier_map);
        }
    }
    if assignments.is_empty() {
        return;
    }
    state.entity_texture_lod.insert(
        entity_id,
        EntityTextureLodState {
            preview_tier,
            active_material_index: snap.active_material_index,
            assignments,
        },
    );
    state.apply_entity_texture_at_active_tier(entity_id);
}

fn smallest_image_index(variants: &[(u32, u32, u32)]) -> Option<u32> {
    variants
        .iter()
        .min_by_key(|(_, w, h)| w * h)
        .map(|(i, _, _)| *i)
}

/// Reparte variantes ordenadas por tamaño en low → ultra (solo tiers vacíos).
fn default_tier_indices(variants: &[(u32, u32, u32)]) -> HashMap<TextureGraphicsTier, u32> {
    let mut out = HashMap::new();
    if variants.is_empty() {
        return out;
    }
    let tiers = [
        TextureGraphicsTier::Low,
        TextureGraphicsTier::Medium,
        TextureGraphicsTier::High,
        TextureGraphicsTier::Ultra,
    ];
    let n = variants.len();
    for (ti, tier) in tiers.iter().enumerate() {
        let vi = if n == 1 {
            0
        } else {
            (ti * (n - 1)) / (tiers.len() - 1)
        };
        out.insert(*tier, variants[vi].0);
    }
    out
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

    pub(crate) fn ensure_entity_texture_lod_defaults(
        &mut self,
        entity_id: EntityId,
        catalog: &[MaterialTexturesCatalog],
    ) {
        let entry = self
            .entity_texture_lod
            .entry(entity_id)
            .or_insert_with(EntityTextureLodState::default);
        for mat in catalog {
            let slot = entry.assignments.entry(mat.material_index).or_default();
            if mat.variants.len() >= 2 {
                for (tier, idx) in default_tier_indices(&mat.variants) {
                    slot.entry(tier).or_insert(idx);
                }
            } else if !slot.contains_key(&TextureGraphicsTier::Low) {
                if let Some(idx) = smallest_image_index(&mat.variants) {
                    slot.insert(TextureGraphicsTier::Low, idx);
                }
            }
        }
    }

    pub(crate) fn build_entity_textures_wire(
        &self,
        entity_id: EntityId,
        model_path: &str,
        catalog: &[MaterialTexturesCatalog],
    ) -> EntityTexturesReadyWire {
        let lod = self.entity_texture_lod.get(&entity_id);
        let active_tier = self.graphics_texture_tier.wire().to_string();

        let materials = catalog
            .iter()
            .map(|mat| {
                let tier_map = lod
                    .and_then(|l| l.assignments.get(&mat.material_index))
                    .map(|m| {
                        m.iter()
                            .map(|(t, idx)| (t.wire().to_string(), *idx))
                            .collect()
                    })
                    .unwrap_or_default();
                EntityMaterialTexturesWire {
                    material_index: mat.material_index,
                    material_name: mat.material_name.clone(),
                    default_image_index: mat.default_image_index,
                    variants: mat
                        .variants
                        .iter()
                        .map(|(image_index, width, height)| EmbeddedTextureVariantWire {
                            image_index: *image_index,
                            width: *width,
                            height: *height,
                        })
                        .collect(),
                    tier_image_index: tier_map,
                    preview_tier: active_tier.clone(),
                }
            })
            .collect();

        EntityTexturesReadyWire {
            event: "entity_textures_ready",
            entity_id,
            model_path: model_path.to_string(),
            materials,
            active_tier,
        }
    }

    fn catalog_for_path(&mut self, path: &str) -> Result<Vec<MaterialTexturesCatalog>, String> {
        let catalog = catalog_gltf_embedded_textures(Path::new(path))?;
        self.glb_texture_catalog_cache
            .insert(path.to_string(), catalog.clone());
        Ok(catalog)
    }

    /// Precarga catálogo + capas GPU de imágenes embebidas (acordeón Recursos).
    pub(crate) fn prewarm_glb_texture_catalog(&mut self, path: &str) {
        let Ok(catalog) = self.catalog_for_path(path) else {
            return;
        };
        let path_buf = Path::new(path);
        let Ok(gltf) = gltf::Gltf::open(path_buf) else {
            return;
        };
        let doc = gltf.document;
        let Ok(buffers) = gltf::import_buffers(&doc, path_buf.parent(), gltf.blob) else {
            return;
        };
        for mat in &catalog {
            let Some(image_index) = smallest_image_index(&mat.variants) else {
                continue;
            };
            let cache_key = format!("{path}#m{}#img{image_index}", mat.material_index);
            if self.texture_path_layers.contains_key(&cache_key) {
                continue;
            }
            let Ok(img) = crate::config_3d::gltf_texture_load::decode_gltf_image_at_index(
                &doc,
                &buffers,
                path_buf.parent(),
                image_index as usize,
            ) else {
                continue;
            };
            let (rgba, w, h) = crate::config_3d::model_asset::gltf_image_data_to_rgba(&img);
            let _ = self.pack_texture_layer(Some(&cache_key), &rgba, w, h);
        }
        log::debug!(
            "[entity_textures] catálogo precalentado: {path} ({} material/es)",
            catalog.len()
        );
        self.patch_static_model_cache_textures(path);
    }

    /// Alinea cada parte en caché con la capa precalentada de su material.
    pub(crate) fn patch_static_model_cache_textures(&mut self, path: &str) {
        let catalog = match self.glb_texture_catalog_cache.get(path) {
            Some(c) => c.clone(),
            None => return,
        };
        for list_key in [
            path.to_string(),
            crate::config_3d::static_model_cache::play_character_cache_key(path),
        ] {
            let snapshot: Vec<(usize, u32)> = match self.static_model_cache.get(&list_key) {
                Some(parts) => parts
                    .iter()
                    .enumerate()
                    .map(|(i, part)| (i, part.material_index))
                    .collect(),
                None => continue,
            };
            for (part_i, material_index) in snapshot {
                let Some(mat) = catalog
                    .iter()
                    .find(|m| m.material_index == material_index)
                else {
                    continue;
                };
                let Some(image_index) = smallest_image_index(&mat.variants) else {
                    continue;
                };
                let cache_key = format!("{path}#m{material_index}#img{image_index}");
                let Some(layer) = self.texture_path_layers.get(&cache_key).copied() else {
                    continue;
                };
                let tex_idx = self.ensure_tex_idx_for_layer(layer);
                if let Some(parts_mut) = self.static_model_cache.get_mut(&list_key) {
                    if let Some(part) = parts_mut.get_mut(part_i) {
                        part.tex_idx = tex_idx;
                    }
                }
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

    pub(crate) fn send_entity_textures_ready(&mut self, entity_id: EntityId) {
        let Some(path) = self.entity_model_path_for_textures(entity_id) else {
            send_event(&EngineEvent::Error {
                message: "La entidad no tiene un modelo GLB/GLTF".to_string(),
            });
            return;
        };
        let catalog = match self.catalog_for_path(&path) {
            Ok(c) => c,
            Err(e) => {
                send_event(&EngineEvent::Error { message: e });
                return;
            }
        };
        self.ensure_entity_texture_lod_defaults(entity_id, &catalog);
        let wire = self.build_entity_textures_wire(entity_id, &path, &catalog);
        send_event(&EngineEvent::EntityTexturesReady {
            entity_id: wire.entity_id,
            model_path: wire.model_path,
            materials: wire.materials,
            active_tier: wire.active_tier,
        });
    }

    pub(crate) fn set_graphics_texture_tier(
        &mut self,
        tier: TextureGraphicsTier,
    ) {
        if self.graphics_texture_tier == tier {
            return;
        }
        self.graphics_texture_tier = tier;
        self.script_engine
            .sync_graphics_texture_tier_readback(tier.wire());
        send_event(&EngineEvent::GraphicsTextureTierChanged {
            tier: tier.wire().to_string(),
        });
        self.refresh_all_entity_textures_for_active_tier();
    }

    pub(crate) fn refresh_all_entity_textures_for_active_tier(&mut self) {
        let ids: Vec<EntityId> = self.save_registry.meta.keys().copied().collect();
        for id in ids {
            let Some(path) = self.entity_model_path_for_textures(id) else {
                continue;
            };
            if let Ok(catalog) = self.catalog_for_path(&path) {
                self.ensure_entity_texture_lod_defaults(id, &catalog);
            }
            self.apply_entity_texture_at_active_tier(id);
        }
    }

    pub(crate) fn set_entity_texture_lod(
        &mut self,
        entity_id: EntityId,
        material_index: u32,
        tier: TextureGraphicsTier,
        image_index: u32,
    ) {
        let Some(path) = self.entity_model_path_for_textures(entity_id) else {
            return;
        };
        let Ok(catalog) = self.catalog_for_path(&path) else {
            return;
        };
        let Some(mat_catalog) = catalog.iter().find(|m| m.material_index == material_index) else {
            return;
        };
        if !image_index_belongs_to_material(image_index, mat_catalog) {
            log::warn!(
                "[entity_textures] image_index {image_index} no pertenece al material {material_index} ({})",
                mat_catalog.material_name
            );
            return;
        }
        {
            let entry = self
                .entity_texture_lod
                .entry(entity_id)
                .or_insert_with(EntityTextureLodState::default);
            entry.active_material_index = material_index;
            entry
                .assignments
                .entry(material_index)
                .or_default()
                .insert(tier, image_index);
        }
        if tier == self.graphics_texture_tier {
            self.apply_skinned_part_texture_layers(entity_id, tier, Some(material_index));
        }
        self.send_entity_textures_ready(entity_id);
    }

    /// Compat: redirige al nivel gráfico global (el `id` se ignora).
    pub(crate) fn set_entity_texture_preview_tier(
        &mut self,
        _entity_id: EntityId,
        tier: TextureGraphicsTier,
    ) {
        self.set_graphics_texture_tier(tier);
    }

    pub(crate) fn apply_entity_texture_at_active_tier(&mut self, entity_id: EntityId) {
        self.apply_skinned_part_texture_layers(entity_id, self.graphics_texture_tier, None);
    }

    pub(crate) fn apply_skinned_part_texture_layers(
        &mut self,
        entity_id: EntityId,
        tier: TextureGraphicsTier,
        only_material: Option<u32>,
    ) {
        let (asset_path, parts_len) = match self.model_animation_bindings.get(&entity_id) {
            Some(b) => (b.asset_path.clone(), b.part_gpu_indices.len()),
            None => return,
        };
        let key = self.model_path_key(&asset_path);
        let is_gltf = crate::config_3d::is_gltf_model_path(&key);
        let asset = match self.model_assets.get(&asset_path) {
            Some(a) => std::sync::Arc::clone(a),
            None => return,
        };
        if asset.parts.len() != parts_len {
            return;
        }

        let catalog = if is_gltf {
            self.catalog_for_path(&key).ok()
        } else {
            None
        };
        let lod = self.entity_texture_lod.get(&entity_id).cloned();

        let mut layer_updates: Vec<(usize, crate::texture::TextureLayer)> =
            Vec::with_capacity(asset.parts.len());
        for (pi, part) in asset.parts.iter().enumerate() {
            if only_material.is_some_and(|m| part.material_index != m) {
                continue;
            }
            let layer = if is_gltf {
                catalog.as_ref().and_then(|catalog| {
                    self.resolve_gltf_material_texture_layer(
                        &key,
                        part.material_index,
                        tier,
                        lod.as_ref(),
                        catalog,
                    )
                })
            } else {
                let cache_key = format!("{key}#fbx-part{pi}");
                Some(self.pack_texture_layer(
                    Some(&cache_key),
                    &part.mesh.rgba,
                    part.mesh.width,
                    part.mesh.height,
                ))
            };
            let Some(layer) = layer else {
                continue;
            };
            layer_updates.push((pi, layer));
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

    fn resolve_gltf_material_texture_layer(
        &mut self,
        path: &str,
        material_index: u32,
        tier: TextureGraphicsTier,
        lod: Option<&EntityTextureLodState>,
        catalog: &[MaterialTexturesCatalog],
    ) -> Option<crate::texture::TextureLayer> {
        let mat_catalog = catalog
            .iter()
            .find(|m| m.material_index == material_index)?;
        let mut image_index = lod
            .and_then(|l| l.assignments.get(&material_index))
            .and_then(|m| m.get(&tier))
            .copied()
            .or_else(|| {
                lod.and_then(|l| l.assignments.get(&material_index))
                    .and_then(|m| m.get(&TextureGraphicsTier::Low))
                    .copied()
            })
            .or_else(|| mat_catalog.default_image_index)
            .or_else(|| smallest_image_index(&mat_catalog.variants))?;
        if !image_index_belongs_to_material(image_index, mat_catalog) {
            image_index = mat_catalog
                .default_image_index
                .or_else(|| smallest_image_index(&mat_catalog.variants))?;
        }

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
