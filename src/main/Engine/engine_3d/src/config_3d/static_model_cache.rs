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

use crate::config_3d::mesh_3d::{
    prepare_cpu_parts_textures_for_gpu, vertex_local_bounds, CpuModelMeshPart,
};
use crate::assets::load::{load_rerasset_cpu, material_texture_chunk_map};
use rer_engine_shared::assets::read_rerasset;
use crate::config_3d::model_asset;
use crate::config_3d::{
    physics_body_world_center, physics_half_extents_for_model, transform_position_for_visual_center,
};
use crate::ecs::{EntityId, MeshComponent, SurfacePbr, Transform};
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
    pub roughness: f32,
    pub metallic: f32,
    pub ior: f32,
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

/// Clave de `model_assets` (skinning/clips); distinta de la malla estática del editor.
pub(crate) fn model_asset_cache_key(canonical_path: &str, normalize: Option<f32>) -> String {
    use crate::config_3d::character_anchor::PLAY_CHARACTER_BODY_HEIGHT;
    match normalize {
        None => canonical_path.to_string(),
        Some(h) if (h - PLAY_CHARACTER_BODY_HEIGHT).abs() < 0.05 => {
            play_character_cache_key(canonical_path)
        }
        Some(h) => format!("{canonical_path}::norm_{h:.3}"),
    }
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
        if path.starts_with("model_") {
            if let Some(entry) = self.imported_model_registry.get(path) {
                let source_key = self.model_path_key(&entry.source_path);
                self.model_store.remove(&source_key);
            }
        }
        self.model_store.remove(&key);
        self.model_preload_gpu_queue.retain(|p| p.path != key);
        self.drop_pending_load_models_for_path(&key);
        log::error!("error cargando modelo {key}: {message}");
        let model_id = self.imported_model_registry.model_id_for_path(&key);
        if let Some(ref id) = model_id {
            self.imported_model_registry.set_state(id, rer_engine_shared::assets::AssetState::Failed);
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
        if let Some(id) = self.imported_model_registry.model_id_for_path(&source_key) {
            if self
                .imported_model_registry
                .get(&id)
                .is_some_and(|e| e.state == rer_engine_shared::assets::AssetState::Ready)
            {
                return id;
            }
        }
        source_key
    }

    /// `model_assets` se indexa por [`Self::model_asset_cache_key`], no por la ruta visual del proyecto.
    pub(crate) fn get_model_asset(
        &self,
        path: &str,
    ) -> Option<std::sync::Arc<crate::config_3d::model_asset::ModelAsset>> {
        self.model_assets
            .get(&self.model_cache_key(path))
            .cloned()
    }

    pub(crate) fn get_model_asset_for_entity(
        &self,
        path: &str,
        entity_id: crate::ecs::EntityId,
    ) -> Option<std::sync::Arc<crate::config_3d::model_asset::ModelAsset>> {
        let base = self.model_cache_key(path);
        let normalize = if self.play_character_entity == Some(entity_id) {
            Some(crate::config_3d::character_anchor::PLAY_CHARACTER_BODY_HEIGHT)
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
        if path.starts_with("model_") {
            if let Some(entry) = self.imported_model_registry.get(path) {
                return entry.name.clone();
            }
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

    pub(crate) fn register_model_asset(
        &mut self,
        path: &str,
        name: &str,
        category: Option<&str>,
    ) {
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
    fn register_model_anim_assets(
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
        if entry.state != rer_engine_shared::assets::AssetState::Ready {
            return false;
        }
        let Ok(loaded) = crate::assets::load::load_rerasset_cpu(&entry.rerasset_path) else {
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

    /// Variante `::play_character` en GPU para `replace_entity_model` instantáneo.
    fn ensure_play_character_model_cache_warmed(&mut self, canonical_path: &str) {
        let play_key = play_character_cache_key(canonical_path);
        if self.static_model_cache.contains_key(&play_key) {
            return;
        }
        self.try_warm_play_character_model_cache(canonical_path);
    }

    /// Tras precarga/import GPU, sube la variante jugador si falta en caché.
    fn try_warm_play_character_model_cache(&mut self, cache_key: &str) {
        let play_key = play_character_cache_key(cache_key);
        if self.static_model_cache.contains_key(&play_key) {
            return;
        }
        let Some(parts) = self.load_play_character_cpu_parts_from_rerasset(cache_key) else {
            return;
        };
        self.sync_upload_play_character_cache(cache_key, parts);
    }

    fn load_play_character_cpu_parts_from_rerasset(
        &self,
        cache_key: &str,
    ) -> Option<Vec<CpuModelMeshPart>> {
        let entry = self.imported_model_registry.get(cache_key)?;
        if entry.state != rer_engine_shared::assets::AssetState::Ready {
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
                        let import_meta = self
                            .imported_model_registry
                            .get(&data.path)
                            .map(|entry| {
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
                            .set_state(&path, rer_engine_shared::assets::AssetState::Failed);
                    }
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
    ) -> Option<crate::texture::TextureLayer> {
        let tex_cache_key = format!("{model_id}::mat{material_index}");
        if let Some(&layer) = self.texture_path_layers.get(&tex_cache_key) {
            return Some(layer);
        }
        let entry = self.imported_model_registry.get(model_id)?;
        if entry.state != rer_engine_shared::assets::AssetState::Ready {
            
            return None;
        }
        let loaded = load_rerasset_cpu(&entry.rerasset_path).ok()?;
        self.rerasset_material_tex.insert(
            model_id.to_string(),
            loaded.material_tex_chunks.clone(),
        );
        let Some(tex_part) = loaded
            .editor_parts
            .iter()
            .find(|p| p.material_index == material_index)
        else {
            
            return None;
        };
        let tex = std::sync::Arc::clone(&tex_part.texture);
        let mip0 = tex.effective_rgba();
        let layer = if let Some(mips) = &tex.layer_mips {
            let layer = self.texture_array.pack_prepared_mips(&self.queue, mips);
            if layer >= crate::texture::TextureArray::MAX_LAYERS - 1 {
                send_event(&EngineEvent::TextureArrayExhausted {
                    max_layers: crate::texture::TextureArray::MAX_LAYERS,
                });
            }
            self.texture_path_layers
                .insert(tex_cache_key.clone(), layer);
            layer
        } else {
            self.pack_texture_layer(
                Some(&tex_cache_key),
                mip0,
                tex.width,
                tex.height,
            )
        };
        Some(layer)
    }

    fn upload_texture_for_cpu_part(
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
                let layer = self.texture_array.pack_prepared_mips(&self.queue, mips);
                if layer >= crate::texture::TextureArray::MAX_LAYERS - 1 {
                    send_event(&EngineEvent::TextureArrayExhausted {
                        max_layers: crate::texture::TextureArray::MAX_LAYERS,
                    });
                }
                self.texture_path_layers
                    .insert(tex_cache_key.clone(), layer);
                layer
            };
            self.tex_layers.push(layer);
            self.tex_layer_albedo
                .push(crate::texture::rgba_mip0_average_linear(&mips[0]));
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
                    roughness: part.roughness,
                    metallic: part.metallic,
                    ior: part.ior,
                }
            })
            .collect();
        self.static_model_cache.insert(play_key, cached);
        
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
        if !pending.uploaded.is_empty() {
            self.static_model_cache
                .insert(play_key.clone(), pending.uploaded);
        }
        if let Some(flush_path) = pending.defer_flush_path {
            self.flush_pending_load_models_for_path(&flush_path);
            self.flush_pending_entity_model_replaces_for_path(&flush_path);
        }
    }

    fn finalize_model_preload_on_gpu(&mut self, pending: PendingGpuModelPreload) {
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

    fn wait_for_static_model_cache(&mut self, key: &str) -> Result<(), String> {
        let started = Instant::now();
        let timeout = Duration::from_secs(120);
        while !self.static_model_cache.contains_key(key) {
            if started.elapsed() > timeout {
                return Err(format!("Tiempo agotado esperando modelo en GPU: {key}"));
            }
            if self
                .imported_model_registry
                .get(key)
                .is_some_and(|e| e.state == rer_engine_shared::assets::AssetState::Failed)
            {
                return Err(format!("Carga del modelo falló: {key}"));
            }
            if !self.model_preload_inflight.contains(key)
                && self
                    .model_preload_gpu_queue
                    .iter()
                    .all(|p| p.path != key)
                && !self.static_model_cache.contains_key(key)
            {
                return Err(format!(
                    "Precarga GPU detenida sin completar caché: {key}"
                ));
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

        if let Some(entry) = ready_entry.filter(|e| {
            e.state == rer_engine_shared::assets::AssetState::Ready
        }) {
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
            self.enqueue_gpu_from_rerasset(
                &model_id,
                &rerasset_path,
                &entry_name,
                is_character,
            );
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

    pub(crate) fn cached_static_model_parts(&self, path: &str) -> Option<&[CachedStaticModelPart]> {
        let key = self.model_cache_key(path);
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
        desired_id: Option<EntityId>,
        roughness: f32,
        metallic: f32,
        ior: f32,
    ) -> EntityId {
        let key = self.model_path_key(path);
        let resolved_category = entity_category.clone().or_else(|| {
            if kind == "character" {
                Some("character".to_string())
            } else {
                None
            }
        });
        let id = if let Some(desired) = desired_id.filter(|&d| d != 0) {
            if self.world.spawn_with_id(desired, Some(entity_name)) {
                desired
            } else {
                log::warn!("[restore] id guardado {desired} en uso; generando id nuevo");
                self.world.spawn(Some(entity_name))
            }
        } else {
            self.world.spawn(Some(entity_name))
        };
        self.world.insert(id, MeshComponent { mesh_idx, tex_idx });
        if roughness >= 0.0 {
            self.world.insert(
                id,
                SurfacePbr {
                    roughness,
                    metallic,
                    ior,
                },
            );
        }
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
            self.try_bind_model_animations_with_gltf(id, &key, None);
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
        desired_id: Option<EntityId>,
    ) -> Result<EntityId, String> {
        if let Err(e) = self.ensure_static_model_cached(path) {
            return Err(e);
        }
        let Some(part) = self
            .cached_static_model_parts(path)
            .and_then(|parts| parts.first())
            .copied()
        else {
            let key = self.model_cache_key(path);
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
            path,
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
            desired_id,
            part.roughness,
            part.metallic,
            part.ior,
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
        let desired = glam::Vec3::from_array(self.default_model_spawn_position());
        let rotation = glam::Quat::IDENTITY;
        let scale = glam::Vec3::ONE;
        let key = self.model_path_key(path);
        let position = transform_position_for_visual_center(
            desired,
            rotation,
            scale,
            &key,
            Some(part.local_bounds),
        )
        .to_array();
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
            None,
            part.roughness,
            part.metallic,
            part.ior,
        )
    }

    pub(crate) fn invalidate_static_model_cache(&mut self, path: &str) {
        let key = self.model_cache_key(path);
        self.static_model_cache.remove(&key);
        self.static_model_cache
            .remove(&play_character_cache_key(&key));
        self.model_assets.remove(&key);
        self.model_preload_inflight.remove(&key);
        if let Some(source) = self.imported_model_registry.get(&key) {
            let source_key = self.model_path_key(&source.source_path);
            self.model_preload_inflight.remove(&source_key);
        }
    }

    /// Elimina un modelo de la biblioteca Resources (`model_store` + registro importado + caché GPU).
    /// Devuelve la clave canónica (`model_id`) si se eliminó algo.
    pub(crate) fn remove_model_from_library(&mut self, path: &str) -> Option<String> {
        let key = self.model_path_key(path);
        let model_id = if key.starts_with("model_") {
            Some(key.clone())
        } else {
            self.imported_model_registry.model_id_for_path(&key)
        };

        let store_keys: Vec<String> = self
            .model_store
            .iter()
            .filter(|(store_key, entry)| {
                **store_key == key
                    || entry
                        .model_id
                        .as_deref()
                        .is_some_and(|id| model_id.as_deref() == Some(id))
            })
            .map(|(store_key, _)| store_key.clone())
            .collect();

        if store_keys.is_empty() && model_id.is_none() {
            return None;
        }

        if let Some(ref id) = model_id {
            self.imported_model_registry.remove(id);
            self.invalidate_static_model_cache(id);
        }

        let canonical = model_id
            .or_else(|| {
                store_keys
                    .iter()
                    .find(|k| k.starts_with("model_"))
                    .cloned()
            })
            .unwrap_or_else(|| key.clone());

        for store_key in store_keys {
            self.invalidate_static_model_cache(&store_key);
            self.model_store.remove(&store_key);
            self.model_preload_gpu_queue.retain(|p| p.path != store_key);
            self.drop_pending_load_models_for_path(&store_key);
        }

        Some(canonical)
    }
}
