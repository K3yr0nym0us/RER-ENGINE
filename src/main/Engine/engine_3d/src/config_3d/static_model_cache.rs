// ── Precarga asíncrona y caché GPU de modelos 3D ─────────────────────────────

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc;
use std::sync::Arc;

use crate::config_3d::mesh_3d::{load_model_file_cpu, CpuModelMeshPart};
use crate::config_3d::model_asset;
use crate::ecs::{EntityId, MeshComponent, Transform};
use crate::engine::State;
use crate::entity_save_meta::EntitySaveMeta;
use crate::ipc::{send_event, EngineEvent};
use rer_engine_shared::editor_defaults::entity_label_for_category;

#[derive(Clone, Copy)]
pub(crate) struct CachedStaticModelPart {
    pub mesh_idx: usize,
    pub tex_idx: usize,
}

pub(crate) struct ModelPreloadCpuResult {
    pub path: String,
    pub parts: Vec<CpuModelMeshPart>,
    pub anim_asset: Option<Arc<model_asset::ModelAsset>>,
}

pub(crate) type ModelPreloadRx =
    mpsc::Receiver<Result<ModelPreloadCpuResult, (String, String)>>;
pub(crate) type ModelPreloadTx =
    mpsc::Sender<Result<ModelPreloadCpuResult, (String, String)>>;

pub(crate) fn create_model_preload_channel() -> (ModelPreloadTx, ModelPreloadRx) {
    mpsc::channel()
}

pub(crate) fn normalize_model_path(path: &str) -> String {
    Path::new(path)
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.replace('\\', "/"))
}

pub(crate) type StaticModelCache = HashMap<String, Vec<CachedStaticModelPart>>;

/// `load_model` recibido mientras la precarga GPU del path sigue en curso.
pub(crate) struct PendingLoadModel {
    pub path: String,
    pub entity_category: Option<String>,
    pub single_instance: bool,
}

/// `replace_entity_model` recibido mientras la precarga GPU del path sigue en curso.
pub(crate) struct PendingEntityModelReplace {
    pub id: EntityId,
    pub path: String,
}

impl State {
    pub(crate) fn model_path_key(&self, path: &str) -> String {
        normalize_model_path(path)
    }

    pub(crate) fn model_needs_skinned_bind(&self, path: &str) -> bool {
        let key = self.model_path_key(path);
        self.model_assets
            .get(&key)
            .is_some_and(|a| !a.parts.is_empty())
    }

    pub(crate) fn register_model_asset(&mut self, path: &str, name: &str) {
        let key = self.model_path_key(path);
        if !Path::new(&key).is_file() {
            send_event(&EngineEvent::Error {
                message: format!("No se encontró el modelo: {path}"),
            });
            return;
        }

        self.model_store.insert(key.clone(), name.to_string());

        if self.static_model_cache.contains_key(&key) {
            send_event(&EngineEvent::ModelAssetLoaded {
                path: key,
                name: name.to_string(),
            });
            return;
        }

        if self.model_preload_inflight.contains(&key) {
            send_event(&EngineEvent::ModelAssetPreloadStarted {
                path: key,
                name: name.to_string(),
            });
            return;
        }

        self.start_model_preload(key, name.to_string());
    }

    pub(crate) fn start_model_preload(&mut self, key: String, name: String) {
        self.model_preload_inflight.insert(key.clone());
        send_event(&EngineEvent::ModelAssetPreloadStarted {
            path: key.clone(),
            name: name.clone(),
        });

        let tx = self.model_preload_tx.clone();
        let key_for_thread = key.clone();
        if let Err(e) = std::thread::Builder::new()
            .name(format!(
                "model-preload-{}",
                key.split(['/', '\\']).last().unwrap_or("model")
            ))
            .spawn(move || {
                let path_buf = Path::new(&key_for_thread);
                let result = match load_model_file_cpu(path_buf, None) {
                    Ok(parts) => {
                        let anim_asset = model_asset::load_model_asset(path_buf, None);
                        Ok(ModelPreloadCpuResult {
                            path: key_for_thread,
                            parts,
                            anim_asset,
                        })
                    }
                    Err(err) => Err((key_for_thread, err)),
                };
                let _ = tx.send(result);
            })
        {
            log::error!("[model_preload] no se pudo lanzar hilo: {e}");
            self.model_preload_inflight.remove(&key);
            self.model_store.remove(&key);
            send_event(&EngineEvent::Error {
                message: format!("No se pudo precargar {name}: {e}"),
            });
        }
    }

    pub(crate) fn poll_model_preloads(&mut self) {
        let mut results = Vec::new();
        while let Ok(result) = self.model_preload_rx.try_recv() {
            results.push(result);
        }
        for result in results {
            match result {
                Ok(data) => self.commit_model_preload(data),
                Err((path, message)) => {
                    self.model_preload_inflight.remove(&path);
                    self.model_store.remove(&path);
                    self.drop_pending_load_models_for_path(&path);
                    log::error!("[model_preload] error precargando {path}: {message}");
                    send_event(&EngineEvent::Error { message });
                }
            }
        }
    }

    fn commit_model_preload(&mut self, data: ModelPreloadCpuResult) {
        let path = data.path.clone();
        self.model_preload_inflight.remove(&path);

        if self.static_model_cache.contains_key(&path) {
            send_event(&EngineEvent::ModelAssetLoaded {
                path: path.clone(),
                name: self
                    .model_store
                    .get(&path)
                    .cloned()
                    .unwrap_or_else(|| path.clone()),
            });
            self.flush_pending_load_models_for_path(&path);
            self.flush_pending_entity_model_replaces_for_path(&path);
            return;
        }

        let mut cached_parts = Vec::with_capacity(data.parts.len());
        for (i, part) in data.parts.iter().enumerate() {
            let mesh_idx = self.meshes.len();
            let tex_idx = self.tex_layers.len();
            self.meshes.push(crate::mesh::upload(
                &self.device,
                &part.vertices,
                &part.indices,
                &format!("preload-{i}"),
            ));
            let cache_key = format!("{path}::part{i}");
            self.pack_texture_layer(
                Some(&cache_key),
                &part.rgba,
                part.width,
                part.height,
            );
            cached_parts.push(CachedStaticModelPart {
                mesh_idx,
                tex_idx,
            });
        }

        self.static_model_cache.insert(path.clone(), cached_parts);
        if let Some(asset) = data.anim_asset {
            self.model_assets.insert(path.clone(), asset);
        }

        let name = self
            .model_store
            .get(&path)
            .cloned()
            .unwrap_or_else(|| path.clone());
        send_event(&EngineEvent::ModelAssetLoaded {
            path: path.clone(),
            name,
        });
        log::info!(
            "[model_preload] precarga GPU lista: {path} ({} parte/s)",
            self.static_model_cache
                .get(&path)
                .map(|p| p.len())
                .unwrap_or(0)
        );
        self.flush_pending_load_models_for_path(&path);
        self.flush_pending_entity_model_replaces_for_path(&path);
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
        log::info!("[model_cache] replace_entity_model en cola (precarga en curso): {key}");
        true
    }

    fn flush_pending_entity_model_replaces_for_path(&mut self, path: &str) {
        let key = self.model_path_key(path);
        let pending: Vec<_> = self
            .pending_entity_model_replaces
            .drain(..)
            .collect();
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
    ) -> bool {
        let key = self.model_path_key(path);
        if !self.model_preload_inflight.contains(&key) {
            return false;
        }
        self.pending_load_models.push(PendingLoadModel {
            path: path.to_string(),
            entity_category: entity_category.map(str::to_owned),
            single_instance,
        });
        log::info!("[model_cache] load_model en cola (precarga en curso): {key}");
        true
    }

    fn drop_pending_load_models_for_path(&mut self, path: &str) {
        let key = self.model_path_key(path);
        self.pending_load_models
            .retain(|req| normalize_model_path(&req.path) != key);
    }

    fn flush_pending_load_models_for_path(&mut self, path: &str) {
        let key = self.model_path_key(path);
        let pending: Vec<_> = self.pending_load_models.drain(..).collect();
        let (ready, still_pending): (Vec<_>, Vec<_>) = pending
            .into_iter()
            .partition(|req| normalize_model_path(&req.path) == key);
        self.pending_load_models = still_pending;
        for req in ready {
            let category = req.entity_category.as_deref();
            if req.single_instance {
                self.load_model_single(&req.path, category);
            } else {
                self.load_model(&req.path, category);
            }
        }
    }

    pub(crate) fn ensure_static_model_cached(&mut self, path: &str) -> Result<(), String> {
        let key = self.model_path_key(path);
        if self.static_model_cache.contains_key(&key) {
            return Ok(());
        }
        if self.model_preload_inflight.contains(&key) {
            return Err(format!(
                "El modelo aún se está precargando: {key}. Espera a que termine en Recursos."
            ));
        }
        if !Path::new(&key).is_file() {
            return Err(format!("No se encontró el modelo: {key}"));
        }

        log::info!("[model_cache] precarga síncrona (no estaba en caché): {key}");
        let parts = load_model_file_cpu(Path::new(&key), None)?;
        if parts.is_empty() {
            return Err(format!("Modelo vacío: {key}"));
        }

        let mut cached_parts = Vec::with_capacity(parts.len());
        for (i, part) in parts.iter().enumerate() {
            let mesh_idx = self.meshes.len();
            let tex_idx = self.tex_layers.len();
            self.meshes.push(crate::mesh::upload(
                &self.device,
                &part.vertices,
                &part.indices,
                &format!("sync-{i}"),
            ));
            let cache_key = format!("{key}::part{i}");
            self.pack_texture_layer(
                Some(&cache_key),
                &part.rgba,
                part.width,
                part.height,
            );
            cached_parts.push(CachedStaticModelPart {
                mesh_idx,
                tex_idx,
            });
        }
        self.static_model_cache.insert(key.clone(), cached_parts);
        if let Some(asset) = model_asset::load_model_asset(Path::new(&key), None) {
            self.model_assets.insert(key, asset);
        }
        Ok(())
    }

    pub(crate) fn cached_static_model_parts(&self, path: &str) -> Option<&[CachedStaticModelPart]> {
        let key = self.model_path_key(path);
        self.static_model_cache.get(&key).map(|v| v.as_slice())
    }

    pub(crate) fn default_model_spawn_position(&self) -> [f32; 3] {
        let forward = self.camera.view_forward();
        let spawn = self.camera.target + forward * 2.5;
        [spawn.x, spawn.y.max(0.0), spawn.z]
    }

    /// Misma lógica que el acordeón Entidades (`load_model` → primera malla en caché),
    /// con transform y metadatos explícitos.
    pub(crate) fn spawn_cached_model_part_at(
        &mut self,
        mesh_idx: usize,
        tex_idx: usize,
        path: &str,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
        entity_name: &str,
        _kind: &str,
        blueprint_id: Option<String>,
        entity_category: Option<String>,
        physics_enabled: bool,
        physics_type: &str,
    ) -> EntityId {
        let key = self.model_path_key(path);
        let id = self.world.spawn(Some(entity_name));
        self.world.insert(id, MeshComponent { mesh_idx, tex_idx });
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = glam::Vec3::from_array(position);
            t.rotation =
                glam::Quat::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
            t.scale = glam::Vec3::from_array(scale);
        }
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "model".to_string(),
                path: key.clone(),
                visual_model_path: None,
                points: None,
                entity_category: entity_category.clone(),
            },
        );
        if physics_enabled && self.play_character_entity != Some(id) {
            let (pos, half) = if let Some(t) = self.world.get::<Transform>(id) {
                (
                    t.position.to_array(),
                    (t.scale.abs() * 0.5).to_array(),
                )
            } else {
                ([0.0_f32; 3], [0.5_f32; 3])
            };
            self.physics
                .set_entity_physics(id, true, physics_type, pos, half);
            send_event(&EngineEvent::PhysicsChanged {
                entity_id: id,
                enabled: true,
                body_type: physics_type.to_string(),
            });
        }
        if let Some(bp) = blueprint_id.clone() {
            self.register_entity_blueprint_id(id, bp);
        }
        if self.model_needs_skinned_bind(&key) {
            self.try_bind_model_animations(id, &key);
        }
        self.push_remove_entity_undo(id);
        send_event(&EngineEvent::ModelLoaded {
            id,
            name: Some(entity_name.to_string()),
            position: Some(position),
            scale: Some(scale),
            rotation: Some(rotation),
            path: Some(key),
            kind: Some("model".to_string()),
            blueprint_id,
            physics_enabled: Some(physics_enabled),
            physics_type: Some(physics_type.to_string()),
            entity_category,
        });
        id
    }

    pub(crate) fn spawn_cached_model_from_save(
        &mut self,
        path: &str,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
        entity_name: Option<&str>,
        entity_category: Option<String>,
        blueprint_id: Option<String>,
        physics_enabled: bool,
        physics_type: &str,
    ) -> Result<EntityId, String> {
        if let Err(e) = self.ensure_static_model_cached(path) {
            return Err(e);
        }
        let key = self.model_path_key(path);
        let Some(part) = self
            .cached_static_model_parts(&key)
            .and_then(|parts| parts.first())
            .copied()
        else {
            return Err(format!("Modelo vacío: {key}"));
        };
        let name = entity_name
            .filter(|n| !n.is_empty())
            .map(|n| n.to_string())
            .unwrap_or_else(|| {
                self.next_numbered_entity_name(entity_label_for_category(
                    entity_category.as_deref(),
                ))
            });
        Ok(self.spawn_cached_model_part_at(
            part.mesh_idx,
            part.tex_idx,
            &key,
            position,
            rotation,
            scale,
            &name,
            "model",
            blueprint_id,
            entity_category,
            physics_enabled,
            physics_type,
        ))
    }

    pub(crate) fn spawn_model_from_cached_part(
        &mut self,
        mesh_idx: usize,
        tex_idx: usize,
        path: &str,
        entity_category: Option<&str>,
    ) -> EntityId {
        let label = self
            .next_numbered_entity_name(entity_label_for_category(entity_category));
        let position = self.default_model_spawn_position();
        self.spawn_cached_model_part_at(
            mesh_idx,
            tex_idx,
            path,
            position,
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            &label,
            "model",
            None,
            None,
            false,
            "static",
        )
    }

    pub(crate) fn invalidate_static_model_cache(&mut self, path: &str) {
        let key = self.model_path_key(path);
        self.static_model_cache.remove(&key);
        self.model_assets.remove(&key);
        self.model_preload_inflight.remove(&key);
    }
}
