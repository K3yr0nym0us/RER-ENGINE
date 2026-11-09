//! Registro de modelos, etiquetas y assets de animación.

use super::*;

impl State {
    pub(crate) fn emit_model_asset_load_failed(&mut self, path: &str, message: String) {
        let key = self.model_path_key(path);
        self.model_preload_inflight.remove(&key);
        if path.starts_with("model_")
            && let Some(entry) = self.imported_model_registry.get(path)
        {
            let source_key = self.model_path_key(&entry.source_path);
            self.model_store.remove(&source_key);
        }
        self.model_store.remove(&key);
        self.model_preload_gpu_queue.retain(|p| p.path != key);
        self.drop_pending_load_models_for_path(&key);
        log::error!("error cargando modelo {key}: {message}");
        let model_id = self.imported_model_registry.model_id_for_path(&key);
        if let Some(ref id) = model_id {
            self.imported_model_registry
                .set_state(id, rer_engine_core::assets::AssetState::Failed);
        }
        send_event(&EngineEvent::ModelAssetLoadFailed {
            path: key,
            message: message.clone(),
            model_id,
        });
        send_event(&EngineEvent::Error { message });
    }

    pub(crate) fn model_path_key(&self, path: &str) -> String {
        normalize_model_path(path)
    }

    /// Clave de caché GPU: `model_id` si el asset importado está listo; si no, ruta fuente.
    pub(crate) fn model_cache_key(&self, path: &str) -> String {
        if path.starts_with("model_") {
            return path.to_string();
        }
        let source_key = self.model_path_key(path);
        if let Some(id) = self.imported_model_registry.model_id_for_path(&source_key)
            && self
                .imported_model_registry
                .get(&id)
                .is_some_and(|e| e.state == rer_engine_core::assets::AssetState::Ready)
        {
            return id;
        }
        source_key
    }

    /// `model_assets` se indexa por [`Self::model_asset_cache_key`], no por la ruta visual del proyecto.
    pub(crate) fn get_model_asset(
        &self,
        path: &str,
    ) -> Option<std::sync::Arc<crate::engine3d::config_3d::model_asset::ModelAsset>> {
        self.model_assets.get(&self.model_cache_key(path)).cloned()
    }

    pub(crate) fn get_model_asset_for_entity(
        &self,
        path: &str,
        entity_id: crate::engine3d::ecs::EntityId,
    ) -> Option<std::sync::Arc<crate::engine3d::config_3d::model_asset::ModelAsset>> {
        let base = self.model_cache_key(path);
        let normalize = if self.play_character_entity == Some(entity_id) {
            Some(crate::engine3d::config_3d::character_anchor::PLAY_CHARACTER_BODY_HEIGHT)
        } else {
            None
        };
        let key = model_asset_cache_key(&base, normalize);
        self.model_assets.get(&key).cloned()
    }

    /// Nombre corto del recurso (alias del proyecto) o nombre de archivo.
    pub(crate) fn model_display_label(&self, path: &str) -> String {
        self.model_store_display_name(path)
    }

    pub(crate) fn model_store_display_name(&self, path: &str) -> String {
        if path.starts_with("model_")
            && let Some(entry) = self.imported_model_registry.get(path)
        {
            return entry.name.clone();
        }
        let key = self.model_path_key(path);
        self.model_store
            .get(&key)
            .map(|e| e.name.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path)
                    .to_string()
            })
    }

    pub(crate) fn model_needs_skinned_bind(&self, path: &str) -> bool {
        let key = self.model_cache_key(path);
        self.model_assets
            .get(&key)
            .is_some_and(|a| !a.parts.is_empty())
    }

    pub(crate) fn register_model_asset(&mut self, path: &str, name: &str, category: Option<&str>) {
        self.start_imported_model_pipeline(path, name, category);
    }

    pub(crate) fn model_library_path_for(&self, path: &str) -> String {
        if path.starts_with("model_") {
            self.imported_model_registry
                .get(path)
                .map(|e| e.source_path.clone())
                .unwrap_or_else(|| path.to_string())
        } else {
            self.model_path_key(path)
        }
    }

    /// Registra `ModelAsset` skinned bajo la clave canónica y, si aplica, `::play_character`.
    pub(crate) fn register_model_anim_assets(
        &mut self,
        canonical_path: &str,
        anim_asset: Arc<model_asset::ModelAsset>,
        warm_play_character: bool,
    ) {
        self.model_assets
            .insert(canonical_path.to_string(), Arc::clone(&anim_asset));
        if warm_play_character {
            let play_key = play_character_cache_key(canonical_path);
            self.model_assets.insert(play_key, anim_asset);
        }
    }

    /// Tras cargar `.rerasset`, asegura `ModelAsset` skinned en `model_assets`.
    pub(crate) fn ensure_model_anim_assets_from_rerasset(
        &mut self,
        model_id: &str,
        warm_play_character: bool,
    ) -> bool {
        let play_key = warm_play_character.then(|| play_character_cache_key(model_id));
        if let Some(ref pk) = play_key {
            if self.model_assets.contains_key(pk) {
                return true;
            }
        } else if self.model_assets.contains_key(model_id) {
            return true;
        }
        if let Some(base) = self.model_assets.get(model_id).cloned() {
            if let Some(pk) = play_key {
                self.model_assets.insert(pk, base);
            }
            return true;
        }
        let Some(entry) = self.imported_model_registry.get(model_id) else {
            return false;
        };
        if entry.state != rer_engine_core::assets::AssetState::Ready {
            return false;
        }
        let Ok(loaded) = crate::engine3d::assets::load::load_rerasset_cpu(&entry.rerasset_path)
        else {
            return false;
        };
        let Some(asset) = loaded.anim_asset else {
            return false;
        };
        self.register_model_anim_assets(model_id, asset, warm_play_character);
        true
    }

    /// Tras cargar `.rerasset`, el jugador enlaza skinning con clave `model_id::play_character`.
    pub(crate) fn ensure_play_character_model_assets_cached(&mut self, model_id: &str) -> bool {
        self.ensure_model_anim_assets_from_rerasset(model_id, true)
    }
}
