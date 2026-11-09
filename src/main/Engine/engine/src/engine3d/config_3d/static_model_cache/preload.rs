//! Precarga CPU/GPU por pasos y colas de modelos pendientes.

use super::*;

impl State {
    /// Variante `::play_character` en GPU para `replace_entity_model` instantáneo.
    pub(crate) fn ensure_play_character_model_cache_warmed(&mut self, canonical_path: &str) {
        let play_key = play_character_cache_key(canonical_path);
        if self.static_model_cache.contains_key(&play_key) {
            return;
        }
        self.try_warm_play_character_model_cache(canonical_path);
    }

    /// Tras precarga/import GPU, sube la variante jugador si falta en caché.
    pub(crate) fn try_warm_play_character_model_cache(&mut self, cache_key: &str) {
        let play_key = play_character_cache_key(cache_key);
        if self.static_model_cache.contains_key(&play_key) {
            return;
        }
        let Some(parts) = self.load_play_character_cpu_parts_from_rerasset(cache_key) else {
            return;
        };
        self.sync_upload_play_character_cache(cache_key, parts);
    }

    pub(crate) fn load_play_character_cpu_parts_from_rerasset(
        &self,
        cache_key: &str,
    ) -> Option<Vec<CpuModelMeshPart>> {
        let entry = self.imported_model_registry.get(cache_key)?;
        if entry.state != rer_engine_core::assets::AssetState::Ready {
            return None;
        }
        load_rerasset_cpu(&entry.rerasset_path)
            .ok()
            .and_then(|loaded| loaded.play_parts)
    }

    pub(crate) fn ensure_play_character_model_cached(&mut self, path: &str) -> Result<(), String> {
        let cache_key = self.model_cache_key(path);
        let play_key = play_character_cache_key(&cache_key);
        if self.static_model_cache.contains_key(&play_key) {
            return Ok(());
        }
        if self.model_preload_inflight.contains(&cache_key) {
            return Err(format!(
                "El modelo aún se está importando: {cache_key}. Espera a que termine en Recursos."
            ));
        }
        self.try_warm_play_character_model_cache(&cache_key);
        if self.static_model_cache.contains_key(&play_key) {
            Ok(())
        } else {
            Err(format!(
                "No se pudo preparar el modelo del jugador: {cache_key}"
            ))
        }
    }

    pub(crate) fn poll_model_preloads(&mut self) {
        let mut results = Vec::new();
        while let Ok(result) = self.model_preload_rx.try_recv() {
            results.push(result);
        }
        for result in results {
            match result {
                Ok(data) => {
                    if data.path.starts_with("model_") {
                        let import_meta =
                            self.imported_model_registry.get(&data.path).map(|entry| {
                                (
                                    entry.source_path.clone(),
                                    entry.name.clone(),
                                    entry.rerasset_path.clone(),
                                )
                            });
                        if let Some((source, name, rerasset_path)) = import_meta {
                            self.cache_rerasset_material_tex_map(&data.path, &rerasset_path);
                            self.on_import_bake_finished(&data.path, &source, &name);
                        }
                    }
                    self.enqueue_model_preload_for_gpu(data);
                }
                Err((path, message)) => {
                    if path.starts_with("model_") {
                        self.imported_model_registry
                            .set_state(&path, rer_engine_core::assets::AssetState::Failed);
                    }
                    self.emit_model_asset_load_failed(&path, message);
                }
            }
        }
    }

    pub(crate) fn upload_mesh_for_cpu_part(
        &mut self,
        part: &CpuModelMeshPart,
        part_index: usize,
        label: &str,
    ) -> usize {
        let mesh_idx = self.meshes.len();
        self.meshes.push(crate::engine3d::mesh::upload(
            &self.vulkan,
            &part.vertices,
            &part.indices,
            &format!("{label}-{part_index}"),
            None,
        ));
        mesh_idx
    }

    pub(crate) fn cache_rerasset_material_tex_map(
        &mut self,
        model_id: &str,
        rerasset_path: &std::path::Path,
    ) {
        if self.rerasset_material_tex.contains_key(model_id) {
            return;
        }
        let Ok(bytes) = std::fs::read(rerasset_path) else {
            return;
        };
        let Ok(file) = read_rerasset(&bytes) else {
            return;
        };
        self.rerasset_material_tex
            .insert(model_id.to_string(), material_texture_chunk_map(&file));
    }

    /// Capa GPU para un material bakeado en `.rerasset` (clave `{model_id}::mat{N}`).
    pub(crate) fn ensure_imported_material_texture_layer(
        &mut self,
        model_id: &str,
        material_index: u32,
    ) -> Option<crate::engine3d::texture::TextureLayer> {
        let tex_cache_key = format!("{model_id}::mat{material_index}");
        if let Some(&layer) = self.texture_path_layers.get(&tex_cache_key) {
            return Some(layer);
        }
        let entry = self.imported_model_registry.get(model_id)?;
        if entry.state != rer_engine_core::assets::AssetState::Ready {
            return None;
        }
        let loaded = load_rerasset_cpu(&entry.rerasset_path).ok()?;
        self.rerasset_material_tex
            .insert(model_id.to_string(), loaded.material_tex_chunks.clone());
        let tex_part = loaded
            .editor_parts
            .iter()
            .find(|p| p.material_index == material_index)?;
        let tex = std::sync::Arc::clone(&tex_part.texture);
        let mip0 = tex.effective_rgba();
        let layer = if let Some(mips) = &tex.layer_mips {
            let layer = self
                .texture_array
                .pack_prepared_mips(&self.vulkan, mips)
                .unwrap();
            if layer >= crate::engine3d::texture::TextureArray::MAX_LAYERS - 1 {
                send_event(&EngineEvent::TextureArrayExhausted {
                    max_layers: crate::engine3d::texture::TextureArray::MAX_LAYERS,
                });
            }
            self.texture_path_layers
                .insert(tex_cache_key.clone(), layer);
            layer
        } else {
            self.pack_texture_layer(Some(&tex_cache_key), mip0, tex.width, tex.height)
        };
        Some(layer)
    }

    pub(crate) fn upload_texture_for_cpu_part(
        &mut self,
        cache_key: &str,
        part: &CpuModelMeshPart,
    ) -> usize {
        let tex_idx = self.tex_layers.len();
        let tex_cache_key = format!("{cache_key}::mat{}", part.material_index);
        let mip0 = part.texture.effective_rgba();
        if let Some(mips) = &part.texture.layer_mips {
            let layer = if let Some(&cached) = self.texture_path_layers.get(&tex_cache_key) {
                cached
            } else {
                let layer = self
                    .texture_array
                    .pack_prepared_mips(&self.vulkan, mips)
                    .unwrap();
                if layer >= crate::engine3d::texture::TextureArray::MAX_LAYERS - 1 {
                    send_event(&EngineEvent::TextureArrayExhausted {
                        max_layers: crate::engine3d::texture::TextureArray::MAX_LAYERS,
                    });
                }
                self.texture_path_layers
                    .insert(tex_cache_key.clone(), layer);
                layer
            };
            self.tex_layers.push(layer);
            self.tex_layer_albedo
                .push(crate::engine3d::texture::rgba_mip0_average_linear(&mips[0]));
        } else {
            self.pack_texture_layer(
                Some(&tex_cache_key),
                mip0,
                part.texture.width,
                part.texture.height,
            );
        }
        tex_idx
    }

    /// Pivote editor: base en Y=0, centrado en X/Z (misma idea que el cubo placeholder).
    pub(crate) fn pivot_static_mesh_for_editor(part: &mut CpuModelMeshPart) {
        let (min, max) = part.local_bounds;
        let cx = (min[0] + max[0]) * 0.5;
        let cy = min[1];
        let cz = (min[2] + max[2]) * 0.5;
        for v in part.vertices.iter_mut() {
            v.position[0] -= cx;
            v.position[1] -= cy;
            v.position[2] -= cz;
        }
        part.local_bounds = vertex_local_bounds(&part.vertices);
    }

    /// Sube la variante `::play_character` a GPU de forma síncrona (replace jugador inmediato).
    /// Reutiliza capas de textura ya empaquetadas bajo `canonical_path` (precarga Recursos).
    pub(crate) fn sync_upload_play_character_cache(
        &mut self,
        canonical_path: &str,
        mut parts: Vec<CpuModelMeshPart>,
    ) {
        let play_key = play_character_cache_key(canonical_path);
        if self.static_model_cache.contains_key(&play_key) {
            return;
        }
        if parts.is_empty() {
            return;
        }
        for part in parts.iter_mut() {
            Self::pivot_static_mesh_for_editor(part);
        }
        prepare_cpu_parts_textures_for_gpu(&mut parts);
        let cached: Vec<CachedStaticModelPart> = parts
            .iter()
            .enumerate()
            .map(|(i, part)| {
                let mesh_idx = self.upload_mesh_for_cpu_part(part, i, "play-preload");
                let tex_idx = self.upload_texture_for_cpu_part(canonical_path, part);
                CachedStaticModelPart {
                    mesh_idx,
                    tex_idx,
                    local_bounds: part.local_bounds,
                    forward_xz: part.forward_xz,
                    roughness: part.roughness,
                    metallic: part.metallic,
                    ior: part.ior,
                }
            })
            .collect();
        self.static_model_cache.insert(play_key, cached);
    }

    pub(crate) fn finalize_gpu_model_preload(&mut self, pending: PendingGpuModelPreload) {
        if pending.play_character_warm_only {
            self.finalize_play_character_warm_on_gpu(pending);
            return;
        }
        self.finalize_model_preload_on_gpu(pending);
    }

    pub(crate) fn finalize_play_character_warm_on_gpu(&mut self, pending: PendingGpuModelPreload) {
        let play_key = pending.path;
        if !pending.uploaded.is_empty() {
            self.static_model_cache
                .insert(play_key.clone(), pending.uploaded);
        }
        if let Some(flush_path) = pending.defer_flush_path {
            self.flush_pending_load_models_for_path(&flush_path);
            self.flush_pending_entity_model_replaces_for_path(&flush_path);
        }
    }

    pub(crate) fn finalize_model_preload_on_gpu(&mut self, pending: PendingGpuModelPreload) {
        let path = pending.path;
        let _display_name = self.model_display_label(&path);
        self.model_preload_inflight.remove(&path);

        if pending.uploaded.is_empty() {
            self.emit_model_asset_load_failed(&path, format!("Modelo vacío: {path}"));
            return;
        }

        self.static_model_cache
            .insert(path.clone(), pending.uploaded);
        if let Some(asset) = pending.anim_asset {
            self.register_model_anim_assets(&path, asset, pending.warm_play_character);
        }

        let name = self.model_store_display_name(&path);
        let model_id = if path.starts_with("model_") {
            Some(path.clone())
        } else {
            self.imported_model_registry.model_id_for_path(&path)
        };
        send_event(&EngineEvent::ModelAssetLoaded {
            path: path.clone(),
            name,
            model_id,
        });
        // GPU upload progress log suppressed
        if pending.warm_play_character {
            if let Some(play_parts) = pending.play_character_parts {
                self.sync_upload_play_character_cache(&path, play_parts);
            } else {
                self.try_warm_play_character_model_cache(&path);
            }
        }
        self.flush_pending_load_models_for_path(&path);
        self.flush_pending_entity_model_replaces_for_path(&path);
    }

    pub(crate) fn wait_for_static_model_cache(&mut self, key: &str) -> Result<(), String> {
        let started = Instant::now();
        let timeout = Duration::from_secs(120);
        while !self.static_model_cache.contains_key(key) {
            if started.elapsed() > timeout {
                return Err(format!("Tiempo agotado esperando modelo en GPU: {key}"));
            }
            if self
                .imported_model_registry
                .get(key)
                .is_some_and(|e| e.state == rer_engine_core::assets::AssetState::Failed)
            {
                return Err(format!("Carga del modelo falló: {key}"));
            }
            if !self.model_preload_inflight.contains(key)
                && self.model_preload_gpu_queue.iter().all(|p| p.path != key)
                && !self.static_model_cache.contains_key(key)
            {
                return Err(format!("Precarga GPU detenida sin completar caché: {key}"));
            }
            self.poll_and_advance_model_preloads(MODEL_GPU_PARTS_DURING_SAVE_LOAD);
            std::thread::yield_now();
        }
        Ok(())
    }

    pub(crate) fn enqueue_model_preload_for_gpu(&mut self, data: ModelPreloadCpuResult) {
        let path = data.path.clone();

        if self.static_model_cache.contains_key(&path) {
            self.model_preload_inflight.remove(&path);
            self.ensure_play_character_model_cache_warmed(&path);
            if path.starts_with("model_") {
                self.ensure_play_character_model_assets_cached(&path);
            }
            let model_id = if path.starts_with("model_") {
                Some(path.clone())
            } else {
                self.imported_model_registry.model_id_for_path(&path)
            };
            send_event(&EngineEvent::ModelAssetLoaded {
                path: path.clone(),
                name: self.model_store_display_name(&path),
                model_id,
            });
            self.flush_pending_load_models_for_path(&path);
            self.flush_pending_entity_model_replaces_for_path(&path);
            return;
        }

        if data.parts.is_empty() {
            self.emit_model_asset_load_failed(&path, format!("Modelo vacío: {path}"));
            return;
        }

        self.model_preload_gpu_queue.push(PendingGpuModelPreload {
            path: data.path,
            parts: data.parts,
            uploaded: Vec::new(),
            anim_asset: data.anim_asset,
            warm_play_character: data.warm_play_character,
            pending_part_mesh_idx: None,
            defer_flush_path: None,
            play_character_warm_only: false,
            play_character_parts: data.play_character_parts,
        });
    }

    /// Sube hasta `max_steps` pasos GPU (malla o textura por paso; cola FIFO).
    pub(crate) fn advance_gpu_model_preloads(&mut self, max_steps: usize) {
        let mut budget = max_steps;
        while budget > 0 {
            if self.model_preload_gpu_queue.is_empty() {
                break;
            }
            let mut pending = self.model_preload_gpu_queue.remove(0);

            if let Some(mesh_idx) = pending.pending_part_mesh_idx.take() {
                let part_index = pending.uploaded.len();
                let part = &pending.parts[part_index];
                let tex_idx = self.upload_texture_for_cpu_part(&pending.path, part);
                pending.uploaded.push(CachedStaticModelPart {
                    mesh_idx,
                    tex_idx,
                    local_bounds: part.local_bounds,
                    forward_xz: part.forward_xz,
                    roughness: part.roughness,
                    metallic: part.metallic,
                    ior: part.ior,
                });
                budget -= 1;

                if pending.uploaded.len() >= pending.parts.len() {
                    self.finalize_gpu_model_preload(pending);
                } else {
                    self.model_preload_gpu_queue.insert(0, pending);
                }
                continue;
            }

            let part_index = pending.uploaded.len();
            if part_index >= pending.parts.len() {
                self.finalize_gpu_model_preload(pending);
                continue;
            }

            let part = &pending.parts[part_index];
            let mesh_idx = self.upload_mesh_for_cpu_part(part, part_index, "preload");
            pending.pending_part_mesh_idx = Some(mesh_idx);
            budget -= 1;
            self.model_preload_gpu_queue.insert(0, pending);
        }
    }

    pub(crate) fn poll_and_advance_model_preloads(&mut self, gpu_parts_budget: usize) {
        self.poll_model_preloads();
        self.advance_gpu_model_preloads(gpu_parts_budget);
    }

    pub(crate) fn queue_entity_model_replace_if_preloading(
        &mut self,
        id: EntityId,
        path: &str,
        _is_play_character: bool,
    ) -> bool {
        let key = self.model_path_key(path);
        if !self.model_preload_inflight.contains(&key) {
            return false;
        }
        self.pending_entity_model_replaces
            .push(PendingEntityModelReplace {
                id,
                path: path.to_string(),
            });
        log::info!("replace_entity_model en cola (precarga en curso): {key}");
        true
    }

    pub(crate) fn flush_pending_entity_model_replaces_for_path(&mut self, path: &str) {
        let key = self.model_path_key(path);
        let pending: Vec<_> = self.pending_entity_model_replaces.drain(..).collect();
        let (for_path, rest): (Vec<_>, Vec<_>) = pending
            .into_iter()
            .partition(|req| self.model_path_key(&req.path) == key);
        self.pending_entity_model_replaces = rest;
        for req in for_path {
            self.replace_entity_model(req.id, &req.path);
        }
    }

    pub(crate) fn queue_load_model_if_preloading(
        &mut self,
        path: &str,
        entity_category: Option<&str>,
        single_instance: bool,
        kind: &str,
    ) -> bool {
        let key = self.model_path_key(path);
        if !self.model_preload_inflight.contains(&key) {
            return false;
        }
        self.pending_load_models.push(PendingLoadModel {
            path: path.to_string(),
            entity_category: entity_category.map(str::to_owned),
            single_instance,
            kind: kind.to_string(),
        });
        log::info!("load_model en cola (precarga en curso): {key}");
        true
    }

    pub(crate) fn drop_pending_load_models_for_path(&mut self, path: &str) {
        let key = self.model_path_key(path);
        self.pending_load_models
            .retain(|req| normalize_model_path(&req.path) != key);
    }

    pub(crate) fn flush_pending_load_models_for_path(&mut self, path: &str) {
        let key = self.model_path_key(path);
        let pending: Vec<_> = self.pending_load_models.drain(..).collect();
        let (ready, still_pending): (Vec<_>, Vec<_>) = pending
            .into_iter()
            .partition(|req| normalize_model_path(&req.path) == key);
        self.pending_load_models = still_pending;
        for req in ready {
            let category = req.entity_category.as_deref();
            if req.single_instance {
                self.load_model_single(&req.path, category, &req.kind);
            } else {
                self.load_model(&req.path, category, &req.kind);
            }
        }
    }

    pub(crate) fn ensure_static_model_cached(&mut self, path: &str) -> Result<(), String> {
        let cache_key = self.model_cache_key(path);
        if self.static_model_cache.contains_key(&cache_key) {
            return Ok(());
        }

        let ready_entry = if path.starts_with("model_") {
            self.imported_model_registry.get(path).cloned()
        } else {
            let source_key = self.model_path_key(path);
            self.imported_model_registry
                .get_by_source_path(&source_key)
                .cloned()
                .or_else(|| {
                    if cache_key.starts_with("model_") {
                        self.imported_model_registry.get(&cache_key).cloned()
                    } else {
                        None
                    }
                })
        };

        if let Some(entry) =
            ready_entry.filter(|e| e.state == rer_engine_core::assets::AssetState::Ready)
        {
            let is_character = entry.category.as_deref() == Some("character");
            let model_id = entry.model_id.clone();
            let rerasset_path = entry.rerasset_path.clone();
            let entry_name = entry.name.clone();
            if !rerasset_path.is_file() {
                return Err(format!(
                    "Archivo .rerasset no encontrado: {}",
                    rerasset_path.display()
                ));
            }
            self.enqueue_gpu_from_rerasset(&model_id, &rerasset_path, &entry_name, is_character);
            if !self.static_model_cache.contains_key(&model_id)
                && !self.model_preload_inflight.contains(&model_id)
            {
                return Err(format!(
                    "No se pudo iniciar carga GPU desde .rerasset: {model_id}"
                ));
            }
            self.wait_for_static_model_cache(&model_id)?;
            if is_character {
                self.ensure_play_character_model_assets_cached(&model_id);
            } else if self.model_needs_skinned_bind(path) {
                self.ensure_model_anim_assets_from_rerasset(&model_id, false);
            }
            return Ok(());
        }

        let display_name = self.model_display_label(path);
        if self.model_preload_inflight.contains(&cache_key) {
            let wait_msg = format!("Esperando importación de «{display_name}»…");
            log::info!("{wait_msg}");
            send_load_progress(&wait_msg, None, None);
            self.wait_for_static_model_cache(&cache_key)?;
            return Ok(());
        }

        Err(format!(
            "Modelo «{display_name}» no importado (requiere .rerasset). Importa el GLB/GLTF/FBX en Recursos."
        ))
    }
}
