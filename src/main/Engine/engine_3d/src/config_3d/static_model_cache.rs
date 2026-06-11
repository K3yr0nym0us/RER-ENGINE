// ── Precarga asíncrona y caché GPU de modelos 3D ─────────────────────────────
//
// Dos entradas por path canónico:
//   · clave normal — mesh tal cual (Recursos / props / `load_model`).
//   · `{path}::play_character` — mesh normalizado (~PLAY_CHARACTER_BODY_HEIGHT) para
//     `replace_entity_model` del jugador. No reutilizar la entrada normal en el player:
//     el pivote/escala difieren y forzar `sync_player_rotation_from_look` en editor
//     rompe la restauración del `.save`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config_3d::character_anchor::PLAY_CHARACTER_BODY_HEIGHT;
use crate::config_3d::mesh_3d::{
    load_gltf_cpu_from_file, load_model_file_cpu, preload_model_cpu_bundle,
    prepare_cpu_parts_textures_for_gpu, vertex_local_bounds, CpuModelMeshPart,
    ModelPreloadOptions,
};
use crate::config_3d::model_asset;
use crate::config_3d::{physics_body_world_center, physics_half_extents_for_model};
use crate::ecs::{EntityId, MeshComponent, Transform};
use crate::engine::State;
use crate::entity_save_meta::EntitySaveMeta;
use crate::ipc::{send_event, send_load_progress, EngineEvent};
use rer_engine_shared::editor_defaults::entity_label_for_spawn;

#[derive(Clone, Copy)]
pub(crate) struct CachedStaticModelPart {
    pub mesh_idx: usize,
    pub tex_idx: usize,
    pub local_bounds: ([f32; 3], [f32; 3]),
    pub forward_xz: glam::Vec2,
}

pub(crate) struct ModelPreloadCpuResult {
    pub path: String,
    pub parts: Vec<CpuModelMeshPart>,
    pub anim_asset: Option<Arc<model_asset::ModelAsset>>,
    /// Variante jugador (`::play_character`); no aplica a props/entorno.
    pub warm_play_character: bool,
    /// Mallas jugador parseadas en el hilo de precarga (sin re-leer disco al warm GPU).
    pub play_character_parts: Option<Vec<CpuModelMeshPart>>,
}

pub(crate) type ModelPreloadRx =
    mpsc::Receiver<Result<ModelPreloadCpuResult, (String, String)>>;
pub(crate) type ModelPreloadTx =
    mpsc::Sender<Result<ModelPreloadCpuResult, (String, String)>>;

pub(crate) fn create_model_preload_channel() -> (ModelPreloadTx, ModelPreloadRx) {
    mpsc::channel()
}

pub(crate) fn normalize_model_path(path: &str) -> String {
    // Normalizamos la "cache key" de forma determinista para evitar misses
    // cuando el mismo archivo llega con variantes de string (p.ej. `\\?\` vs
    // rutas normales o separadores `\` vs `/`).
    let canonical = Path::new(path)
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string());

    let mut s = canonical.replace('\\', "/");

    // Convertir rutas device-prefix para que todas compartan la misma key.
    // Ej: `\\?\C:\x\y.glb` -> `C:/x/y.glb`
    // Ej: `\\?\UNC\server\share\...` -> `//server/share/...`
    if let Some(rest) = s.strip_prefix("//?/UNC/") {
        s = format!("//{rest}");
    } else if let Some(rest) = s.strip_prefix("//?/") {
        s = rest.to_string();
    }

    s
}

pub(crate) type StaticModelCache = HashMap<String, Vec<CachedStaticModelPart>>;

pub(crate) fn play_character_cache_key(canonical_path: &str) -> String {
    format!("{canonical_path}::play_character")
}

/// `load_model` recibido mientras la precarga GPU del path sigue en curso.
pub(crate) struct PendingLoadModel {
    pub path: String,
    pub entity_category: Option<String>,
    pub single_instance: bool,
    pub kind: String,
}

/// `replace_entity_model` recibido mientras la precarga GPU del path sigue en curso.
pub(crate) struct PendingEntityModelReplace {
    pub id: EntityId,
    pub path: String,
}

/// CPU listo; subida a GPU pendiente (no bloquear el event loop de winit).
pub(crate) struct PendingGpuModelPreload {
    path: String,
    parts: Vec<CpuModelMeshPart>,
    uploaded: Vec<CachedStaticModelPart>,
    anim_asset: Option<Arc<model_asset::ModelAsset>>,
    /// Variante jugador (`::play_character`); no aplica a props/entorno.
    warm_play_character: bool,
    /// Malla en GPU; textura de la parte actual pendiente (presupuesto en pasos).
    pending_part_mesh_idx: Option<usize>,
    /// Tras este job, ejecutar flush de `load_model` / replace en cola.
    defer_flush_path: Option<String>,
    /// Solo calienta caché `::play_character` (sin repetir `ModelAssetLoaded`).
    play_character_warm_only: bool,
    /// Variante jugador ya parseada en CPU (precarga Recursos con warm).
    play_character_parts: Option<Vec<CpuModelMeshPart>>,
}

/// Pasos GPU (malla o textura) por frame durante precarga en editor (event loop / IPC).
pub(crate) const MODEL_GPU_PARTS_PER_FRAME: usize = 12;
/// Presupuesto mayor al cargar `.save` o esperar precarga (sin redibujar ventana; ver `c51c943`).
pub(crate) const MODEL_GPU_PARTS_DURING_SAVE_LOAD: usize = 64;

impl State {
    fn emit_model_asset_load_failed(&mut self, path: &str, message: String) {
        let key = self.model_path_key(path);
        self.model_preload_inflight.remove(&key);
        self.model_store.remove(&key);
        self.model_preload_gpu_queue.retain(|p| p.path != key);
        self.drop_pending_load_models_for_path(&key);
        log::error!("error cargando modelo {key}: {message}");
        send_event(&EngineEvent::ModelAssetLoadFailed {
            path: key,
            message: message.clone(),
        });
        send_event(&EngineEvent::Error { message });
    }

    pub(crate) fn model_path_key(&self, path: &str) -> String {
        normalize_model_path(path)
    }

    /// Nombre corto del recurso (alias del proyecto) o nombre de archivo.
    pub(crate) fn model_display_label(&self, path: &str) -> String {
        self.model_store_display_name(path)
    }

    pub(crate) fn model_store_display_name(&self, path: &str) -> String {
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
        let key = self.model_path_key(path);
        self.model_assets
            .get(&key)
            .is_some_and(|a| !a.parts.is_empty())
    }

    pub(crate) fn register_model_asset(
        &mut self,
        path: &str,
        name: &str,
        category: Option<&str>,
    ) {
        let key = self.model_path_key(path);
        if !Path::new(&key).is_file() {
            let message = format!("No se encontró el modelo: {path}");
            send_event(&EngineEvent::ModelAssetLoadFailed {
                path: key.clone(),
                message: message.clone(),
            });
            send_event(&EngineEvent::Error { message });
            return;
        }

        let category = crate::ipc::normalize_model_library_category(category)
            .or_else(|| self.model_store.get(&key).and_then(|e| e.category.clone()));
        let is_character = category.as_deref() == Some("character");
        let entry = crate::ipc::ModelStoreEntry {
            name: name.to_string(),
            category,
        };
        self.model_store.insert(key.clone(), entry);

        if self.static_model_cache.contains_key(&key) {
            if is_character {
                self.ensure_play_character_model_cache_warmed(&key);
            }
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

        self.start_model_preload(
            key,
            name.to_string(),
            ModelPreloadOptions::library(if is_character {
                Some("character")
            } else {
                None
            }),
        );
    }

    /// Variante `::play_character` en GPU para `replace_entity_model` instantáneo.
    fn ensure_play_character_model_cache_warmed(&mut self, canonical_path: &str) {
        let play_key = play_character_cache_key(canonical_path);
        if self.static_model_cache.contains_key(&play_key) {
            return;
        }
        self.try_warm_play_character_model_cache(canonical_path);
    }

    pub(crate) fn start_model_preload(
        &mut self,
        key: String,
        name: String,
        options: ModelPreloadOptions,
    ) {
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
                let result = match preload_model_cpu_bundle(path_buf, options) {
                    Ok((parts, anim_asset, play_character_parts)) => {
                        Ok(ModelPreloadCpuResult {
                            path: key_for_thread,
                            parts,
                            anim_asset,
                            warm_play_character: options.warm_play_character,
                            play_character_parts,
                        })
                    }
                    Err(err) => Err((key_for_thread, err)),
                };
                let _ = tx.send(result);
            })
        {
            log::error!("no se pudo lanzar hilo: {e}");
            self.emit_model_asset_load_failed(&key, format!("No se pudo precargar {name}: {e}"));
        }
    }

    pub(crate) fn poll_model_preloads(&mut self) {
        let mut results = Vec::new();
        while let Ok(result) = self.model_preload_rx.try_recv() {
            results.push(result);
        }
        for result in results {
            match result {
                Ok(data) => self.enqueue_model_preload_for_gpu(data),
                Err((path, message)) => {
                    self.emit_model_asset_load_failed(&path, message);
                }
            }
        }
    }

    fn upload_mesh_for_cpu_part(
        &mut self,
        part: &CpuModelMeshPart,
        part_index: usize,
        label: &str,
    ) -> usize {
        let mesh_idx = self.meshes.len();
        self.meshes.push(crate::mesh::upload(
            &self.device,
            &part.vertices,
            &part.indices,
            &format!("{label}-{part_index}"),
        ));
        mesh_idx
    }

    fn upload_texture_for_cpu_part(
        &mut self,
        cache_key: &str,
        part: &CpuModelMeshPart,
    ) -> usize {
        let tex_idx = self.tex_layers.len();
        let tex_cache_key = format!("{cache_key}::mat{}", part.material_index);
        if let Some(mips) = &part.texture.layer_mips {
            let layer = if let Some(&cached) = self.texture_path_layers.get(&tex_cache_key) {
                cached
            } else {
                let layer = self.texture_array.pack_prepared_mips(&self.queue, mips);
                if layer >= crate::texture::TextureArray::MAX_LAYERS - 1 {
                    send_event(&EngineEvent::TextureArrayExhausted {
                        max_layers: crate::texture::TextureArray::MAX_LAYERS,
                    });
                }
                self.texture_path_layers
                    .insert(tex_cache_key, layer);
                layer
            };
            self.tex_layers.push(layer);
        } else {
            self.pack_texture_layer(
                Some(&tex_cache_key),
                &part.texture.rgba,
                part.texture.width,
                part.texture.height,
            );
        }
        tex_idx
    }

    /// Pivote editor: base en Y=0, centrado en X/Z (misma idea que el cubo placeholder).
    fn pivot_static_mesh_for_editor(part: &mut CpuModelMeshPart) {
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
    fn sync_upload_play_character_cache(
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
                }
            })
            .collect();
        self.static_model_cache.insert(play_key, cached);
        log::debug!("variante jugador en GPU: {canonical_path}");
    }

    fn finalize_gpu_model_preload(&mut self, pending: PendingGpuModelPreload) {
        if pending.play_character_warm_only {
            self.finalize_play_character_warm_on_gpu(pending);
            return;
        }
        self.finalize_model_preload_on_gpu(pending);
    }

    fn finalize_play_character_warm_on_gpu(&mut self, pending: PendingGpuModelPreload) {
        let play_key = pending.path;
        if pending.uploaded.is_empty() {
            log::debug!("warm jugador vacío: {play_key}");
        } else {
            self.static_model_cache
                .insert(play_key.clone(), pending.uploaded);
            log::debug!(
                "variante jugador en GPU: {}",
                pending.defer_flush_path.as_deref().unwrap_or(&play_key)
            );
        }
        if let Some(flush_path) = pending.defer_flush_path {
            self.flush_pending_load_models_for_path(&flush_path);
            self.flush_pending_entity_model_replaces_for_path(&flush_path);
        }
    }

    fn finalize_model_preload_on_gpu(&mut self, pending: PendingGpuModelPreload) {
        let path = pending.path;
        let display_name = self.model_display_label(&path);
        self.model_preload_inflight.remove(&path);

        if pending.uploaded.is_empty() {
            self.emit_model_asset_load_failed(&path, format!("Modelo vacío: {path}"));
            return;
        }

        self.static_model_cache
            .insert(path.clone(), pending.uploaded);
        if let Some(asset) = pending.anim_asset {
            self.model_assets.insert(path.clone(), asset);
        }

        let name = self.model_store_display_name(&path);
        send_event(&EngineEvent::ModelAssetLoaded {
            path: path.clone(),
            name,
        });
        let gpu_msg = format!(
            "Modelo «{display_name}» subido a GPU ({} parte/s)",
            self.static_model_cache
                .get(&path)
                .map(|p| p.len())
                .unwrap_or(0)
        );
        log::info!("{gpu_msg}");
        send_load_progress(&gpu_msg, None, None);
        if pending.warm_play_character {
            if let Some(play_parts) = pending.play_character_parts {
                self.sync_upload_play_character_cache(&path, play_parts);
            } else if let Some(play_parts) = self.load_play_character_cpu_parts(&path) {
                self.sync_upload_play_character_cache(&path, play_parts);
            }
        }
        self.flush_pending_load_models_for_path(&path);
        self.flush_pending_entity_model_replaces_for_path(&path);
    }

    fn load_play_character_cpu_parts(
        &mut self,
        canonical_path: &str,
    ) -> Option<Vec<CpuModelMeshPart>> {
        let path_buf = Path::new(canonical_path);
        if !path_buf.is_file() {
            return None;
        }
        let is_gltf = path_buf
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("glb") || e.eq_ignore_ascii_case("gltf"));

        let parts = if is_gltf {
            match model_asset::import_gltf(path_buf).and_then(|file| {
                if !self.model_assets.contains_key(canonical_path)
                    && model_asset::gltf_needs_model_asset(file.as_ref())
                {
                    if let Some(asset) =
                        model_asset::load_model_asset_from_gltf(file.as_ref(), None)
                    {
                        self.model_assets
                            .insert(canonical_path.to_string(), Arc::clone(&asset));
                    }
                }
                load_gltf_cpu_from_file(file.as_ref(), Some(PLAY_CHARACTER_BODY_HEIGHT))
            }) {
                Ok(parts) if !parts.is_empty() => parts,
                Ok(_) => return None,
                Err(e) => {
                    log::debug!(
                        "sin variante jugador para {canonical_path}: {e}"
                    );
                    return None;
                }
            }
        } else {
            match load_model_file_cpu(path_buf, Some(PLAY_CHARACTER_BODY_HEIGHT)) {
                Ok(parts) if !parts.is_empty() => parts,
                Ok(_) => return None,
                Err(e) => {
                    log::debug!(
                        "sin variante jugador para {canonical_path}: {e}"
                    );
                    return None;
                }
            }
        };

        if !is_gltf && !self.model_assets.contains_key(canonical_path) {
            if let Some(asset) = model_asset::load_model_asset(path_buf, None) {
                self.model_assets.insert(canonical_path.to_string(), asset);
            }
        }
        Some(parts)
    }

    fn wait_for_static_model_cache(&mut self, key: &str) {
        while !self.static_model_cache.contains_key(key) {
            self.poll_and_advance_model_preloads(MODEL_GPU_PARTS_DURING_SAVE_LOAD);
            std::thread::yield_now();
        }
    }

    fn enqueue_model_preload_for_gpu(&mut self, data: ModelPreloadCpuResult) {
        let path = data.path.clone();

        if self.static_model_cache.contains_key(&path) {
            self.model_preload_inflight.remove(&path);
            self.ensure_play_character_model_cache_warmed(&path);
            send_event(&EngineEvent::ModelAssetLoaded {
                path: path.clone(),
                name: self.model_store_display_name(&path),
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

    /// Tras `load_model_asset`, sube la variante jugador para que el replace sea inmediato.
    pub(crate) fn try_warm_play_character_model_cache(&mut self, canonical_path: &str) {
        let play_key = play_character_cache_key(canonical_path);
        if self.static_model_cache.contains_key(&play_key) {
            return;
        }
        let Some(parts) = self.load_play_character_cpu_parts(canonical_path) else {
            return;
        };
        self.sync_upload_play_character_cache(canonical_path, parts);
    }

    pub(crate) fn ensure_play_character_model_cached(&mut self, path: &str) -> Result<(), String> {
        let key = self.model_path_key(path);
        let play_key = play_character_cache_key(&key);
        if self.static_model_cache.contains_key(&play_key) {
            return Ok(());
        }
        if self.model_preload_inflight.contains(&key) {
            return Err(format!(
                "El modelo aún se está precargando: {key}. Espera a que termine en Recursos."
            ));
        }
        self.try_warm_play_character_model_cache(&key);
        if self.static_model_cache.contains_key(&play_key) {
            Ok(())
        } else {
            Err(format!(
                "No se pudo preparar el modelo del jugador: {key}"
            ))
        }
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
                self.load_model_single(&req.path, category, &req.kind);
            } else {
                self.load_model(&req.path, category, &req.kind);
            }
        }
    }

    pub(crate) fn ensure_static_model_cached(&mut self, path: &str) -> Result<(), String> {
        let key = self.model_path_key(path);
        if self.static_model_cache.contains_key(&key) {
            return Ok(());
        }
        let display_name = self.model_display_label(path);
        if self.model_preload_inflight.contains(&key) {
            let wait_msg = format!("Esperando precarga de «{display_name}»…");
            log::info!("{wait_msg}");
            send_load_progress(&wait_msg, None, None);
            let wait_started = Instant::now();
            let file_bytes = std::fs::metadata(&key).map(|m| m.len()).unwrap_or(0);
            let extra_secs = (file_bytes / (5 * 1024 * 1024)).min(480);
            let preload_wait =
                Duration::from_secs(120).saturating_add(Duration::from_secs(extra_secs));
            while self.model_preload_inflight.contains(&key) && wait_started.elapsed() < preload_wait
            {
                self.poll_and_advance_model_preloads(MODEL_GPU_PARTS_DURING_SAVE_LOAD);
                if self.static_model_cache.contains_key(&key) {
                    let ready_msg = format!(
                        "Modelo «{display_name}» listo (espera {} ms)",
                        wait_started.elapsed().as_millis()
                    );
                    log::info!("{ready_msg}");
                    send_load_progress(&ready_msg, None, None);
                    return Ok(());
                }
                std::thread::yield_now();
            }
            if self.static_model_cache.contains_key(&key) {
                return Ok(());
            }
            if self.model_preload_inflight.contains(&key) {
                return Err(format!(
                    "Tiempo agotado esperando el modelo «{display_name}»",
                ));
            }
        }
        if !Path::new(&key).is_file() {
            return Err(format!("No se encontró el modelo: {key}"));
        }

        let sync_started = Instant::now();
        let sync_msg = format!("Cargando modelo «{display_name}» (hilo principal)…");
        log::info!("{sync_msg}");
        send_load_progress(&sync_msg, None, None);
        let path_buf = Path::new(&key);
        let (mut parts, anim_asset, _) = preload_model_cpu_bundle(
            path_buf,
            ModelPreloadOptions {
                warm_play_character: false,
                load_skinned_asset: false,
            },
        )?;
        if parts.is_empty() {
            return Err(format!("Modelo vacío: {key}"));
        }
        for part in &mut parts {
            Self::pivot_static_mesh_for_editor(part);
        }
        self.model_preload_gpu_queue.push(PendingGpuModelPreload {
            path: key.clone(),
            parts,
            uploaded: Vec::new(),
            anim_asset,
            warm_play_character: false,
            pending_part_mesh_idx: None,
            defer_flush_path: None,
            play_character_warm_only: false,
            play_character_parts: None,
        });
        self.wait_for_static_model_cache(&key);
        let done_msg = format!(
            "Modelo «{display_name}» listo en GPU (+{} ms)",
            sync_started.elapsed().as_millis()
        );
        log::info!("{done_msg}");
        send_load_progress(&done_msg, None, None);
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
        kind: &str,
        blueprint_id: Option<String>,
        entity_category: Option<String>,
        physics_enabled: bool,
        physics_type: &str,
        local_bounds: ([f32; 3], [f32; 3]),
    ) -> EntityId {
        let key = self.model_path_key(path);
        let resolved_category = entity_category.clone().or_else(|| {
            if kind == "character" {
                Some("character".to_string())
            } else {
                None
            }
        });
        let id = self.world.spawn(Some(entity_name));
        self.world.insert(id, MeshComponent { mesh_idx, tex_idx });
        if kind == "character" && !self.character_entities.contains(&id) {
            self.character_entities.push(id);
        }
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = glam::Vec3::from_array(position);
            t.rotation =
                glam::Quat::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
            t.scale = glam::Vec3::from_array(scale);
        }
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: kind.to_string(),
                path: key.clone(),
                visual_model_path: None,
                entity_category: resolved_category.clone(),
            },
        );
        if physics_enabled && self.play_character_entity != Some(id) {
            if let Some(t) = self.world.get::<Transform>(id).cloned() {
                let half = physics_half_extents_for_model(
                    t.scale.abs().to_array(),
                    Some(local_bounds),
                );
                let body_pos =
                    physics_body_world_center(&t, Some(local_bounds), path, half);
                self.physics
                    .set_entity_physics(id, true, physics_type, body_pos, half);
            }
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
            let gltf = model_asset::import_gltf(Path::new(&key)).ok();
            self.try_bind_model_animations_with_gltf(id, &key, gltf.as_deref());
        }
        self.push_remove_entity_undo(id);
        send_event(&EngineEvent::ModelLoaded {
            id,
            name: Some(entity_name.to_string()),
            position: Some(position),
            scale: Some(scale),
            rotation: Some(rotation),
            path: Some(key),
            kind: Some(kind.to_string()),
            blueprint_id,
            physics_enabled: Some(physics_enabled),
            physics_type: Some(physics_type.to_string()),
            entity_category: resolved_category,
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
        let kind = if entity_category.as_deref() == Some("character") {
            "character"
        } else {
            "model"
        };
        let name = entity_name
            .filter(|n| !n.is_empty())
            .map(|n| n.to_string())
            .unwrap_or_else(|| {
                self.next_numbered_entity_name(entity_label_for_spawn(
                    kind,
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
            kind,
            blueprint_id,
            entity_category,
            physics_enabled,
            physics_type,
            part.local_bounds,
        ))
    }

    pub(crate) fn spawn_model_from_cached_part(
        &mut self,
        part: CachedStaticModelPart,
        path: &str,
        entity_category: Option<&str>,
        kind: &str,
    ) -> EntityId {
        let label = self
            .next_numbered_entity_name(entity_label_for_spawn(kind, entity_category));
        let position = self.default_model_spawn_position();
        self.spawn_cached_model_part_at(
            part.mesh_idx,
            part.tex_idx,
            path,
            position,
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            &label,
            kind,
            None,
            entity_category.map(str::to_string),
            false,
            "static",
            part.local_bounds,
        )
    }

    pub(crate) fn invalidate_static_model_cache(&mut self, path: &str) {
        let key = self.model_path_key(path);
        self.static_model_cache.remove(&key);
        self.static_model_cache
            .remove(&play_character_cache_key(&key));
        self.model_assets.remove(&key);
        self.model_preload_inflight.remove(&key);
        model_asset::invalidate_gltf_import_cache(Path::new(&key));
    }
}
