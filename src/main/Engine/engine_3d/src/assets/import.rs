//! Hilo de importación: GLB/FBX → bake `.rerasset` → precarga GPU.

use std::path::PathBuf;

use rer_engine_shared::assets::{AssetState, RER_IMPORTER_VERSION};

use crate::config_3d::mesh_3d::{preload_model_cpu_bundle, ModelPreloadOptions};
use crate::config_3d::static_model_cache::ModelPreloadCpuResult;
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};

use super::bake::{bake_to_rerasset, build_bake_input, current_importer_version};
use super::registry::{
    generate_model_id, rerasset_path_for_id, source_fingerprint, ImportedModelEntry,
};

impl State {
    pub(crate) fn start_imported_model_pipeline(
        &mut self,
        source_path: &str,
        name: &str,
        category: Option<&str>,
    ) {
        let key = self.model_path_key(source_path);
        let path_buf = PathBuf::from(&key);
        if !path_buf.is_file() {
            let message = format!("No se encontró el modelo: {source_path}");
            send_event(&EngineEvent::ModelAssetLoadFailed {
                path: key,
                message: message.clone(),
                model_id: None,
            });
            send_event(&EngineEvent::Error { message });
            return;
        }

        let category = crate::ipc::normalize_model_library_category(category)
            .or_else(|| self.model_store.get(&key).and_then(|e| e.category.clone()));
        let is_character = category.as_deref() == Some("character");

        let (size, mtime) = source_fingerprint(&path_buf);
        if let Some(existing) = self.imported_model_registry.get_by_source_path(&key).cloned() {
            if existing.state == AssetState::Ready
                && existing.source_size == size
                && existing.source_mtime_secs == mtime
                && existing.importer_version >= current_importer_version()
            {
                self.model_store.insert(
                    key.clone(),
                    crate::ipc::ModelStoreEntry {
                        name: name.to_string(),
                        category: category.clone(),
                        model_id: Some(existing.model_id.clone()),
                        rerasset_path: Some(
                            existing.rerasset_path.to_string_lossy().into_owned(),
                        ),
                    },
                );
                self.enqueue_gpu_from_rerasset(
                    &existing.model_id,
                    &existing.rerasset_path,
                    name,
                    is_character,
                );
                return;
            }
        }

        let model_id = generate_model_id(name, &key);
        let rerasset_path = rerasset_path_for_id(&model_id);

        self.imported_model_registry.insert(ImportedModelEntry {
            model_id: model_id.clone(),
            name: name.to_string(),
            category: category.clone(),
            state: AssetState::Importing,
            rerasset_path: rerasset_path.clone(),
            source_path: key.clone(),
            source_size: size,
            source_mtime_secs: mtime,
            importer_version: 0,
        });

        self.model_store.insert(
            key.clone(),
            crate::ipc::ModelStoreEntry {
                name: name.to_string(),
                category,
                model_id: Some(model_id.clone()),
                rerasset_path: None,
            },
        );

        send_event(&EngineEvent::ModelAssetImporting {
            model_id: model_id.clone(),
            path: key.clone(),
            name: name.to_string(),
        });

        if self.model_preload_inflight.contains(&model_id) {
            return;
        }
        self.model_preload_inflight.insert(model_id.clone());

        let tx = self.model_preload_tx.clone();
        let model_id_thread = model_id.clone();
        let rerasset_path_thread = rerasset_path.clone();
        let key_thread = key.clone();
        let name_thread = name.to_string();
        let category_label = self
            .model_store
            .get(&key)
            .and_then(|e| e.category.clone());

        if let Err(e) = std::thread::Builder::new()
            .name(format!("model-import-{model_id_thread}"))
            .spawn(move || {
                let options = ModelPreloadOptions::library(if category_label.as_deref()
                    == Some("character")
                {
                    Some("character")
                } else {
                    None
                });

                let result = (|| -> Result<ModelPreloadCpuResult, String> {
                    let (parts, anim_asset, play_parts) =
                        preload_model_cpu_bundle(&path_buf, options)?;
                    let input = build_bake_input(
                        &path_buf,
                        category_label.as_deref(),
                        &parts,
                        play_parts.as_deref(),
                        anim_asset.as_deref(),
                    );
                    bake_to_rerasset(&rerasset_path_thread, &input)?;
                    let _ = super::registry::mirror_source_file(&path_buf);
                    Ok(ModelPreloadCpuResult {
                        path: model_id_thread.clone(),
                        parts,
                        anim_asset,
                        warm_play_character: options.warm_play_character,
                        play_character_parts: play_parts,
                    })
                })();

                let _ = tx.send(result.map_err(|msg| (model_id_thread, msg)));
                let _ = (key_thread, name_thread);
            })
        {
            self.model_preload_inflight.remove(&model_id);
            self.imported_model_registry
                .set_state(&model_id, AssetState::Failed);
            let message = format!("No se pudo importar {name}: {e}");
            send_event(&EngineEvent::ModelAssetLoadFailed {
                path: key,
                message,
                model_id: Some(model_id),
            });
        }
    }

    pub(crate) fn enqueue_gpu_from_rerasset(
        &mut self,
        model_id: &str,
        rerasset_path: &std::path::Path,
        name: &str,
        is_character: bool,
    ) {
        if self.static_model_cache.contains_key(model_id) {
            send_event(&EngineEvent::ModelAssetLoaded {
                path: model_id.to_string(),
                name: name.to_string(),
                model_id: Some(model_id.to_string()),
            });
            return;
        }
        if self.model_preload_inflight.contains(model_id) {
            send_event(&EngineEvent::ModelAssetPreloadStarted {
                path: model_id.to_string(),
                name: name.to_string(),
                model_id: Some(model_id.to_string()),
            });
            return;
        }

        let loaded = match super::load::load_rerasset_cpu(rerasset_path) {
            Ok(cpu) => cpu,
            Err(e) => {
                send_event(&EngineEvent::ModelAssetLoadFailed {
                    path: model_id.to_string(),
                    message: e,
                    model_id: Some(model_id.to_string()),
                });
                return;
            }
        };

        self.model_preload_inflight.insert(model_id.to_string());
        send_event(&EngineEvent::ModelAssetPreloadStarted {
            path: model_id.to_string(),
            name: name.to_string(),
            model_id: Some(model_id.to_string()),
        });

        self.enqueue_model_preload_for_gpu(ModelPreloadCpuResult {
            path: model_id.to_string(),
            parts: loaded.editor_parts,
            anim_asset: loaded.anim_asset,
            warm_play_character: is_character,
            play_character_parts: loaded.play_parts,
        });
    }

    pub(crate) fn on_import_bake_finished(&mut self, model_id: &str, source_key: &str, name: &str) {
        let rerasset_path = rerasset_path_for_id(model_id);
        self.imported_model_registry.mark_ready(
            model_id,
            rerasset_path.clone(),
            RER_IMPORTER_VERSION,
        );
        if let Some(store) = self.model_store.get_mut(source_key) {
            store.rerasset_path = Some(rerasset_path.to_string_lossy().into_owned());
        }
        send_event(&EngineEvent::ModelAssetImported {
            model_id: model_id.to_string(),
            path: source_key.to_string(),
            name: name.to_string(),
            asset: rerasset_path.to_string_lossy().into_owned(),
        });
    }
}
