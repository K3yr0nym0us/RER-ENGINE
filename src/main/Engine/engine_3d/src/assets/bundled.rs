//! Modelos empaquetados con el binario del motor (jugador por defecto, etc.).

use std::path::{Path, PathBuf};

use rer_engine_shared::assets::{AssetState, RER_IMPORTER_VERSION};
use rer_engine_shared::bundled_models::{
    DEFAULT_PLAY_CHARACTER_FBX, DEFAULT_PLAY_CHARACTER_MODEL_ID, DEFAULT_PLAY_CHARACTER_NAME,
    DEFAULT_PLAY_CHARACTER_RERASSET,
};

use crate::engine::State;

use super::registry::{ImportedModelEntry, normalize_source_key, source_fingerprint};

/// Carpeta `Models` junto al binario del motor (dev: `Engine/Models`, empaquetado: `resources/engine/Models`).
pub fn resolve_engine_models_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let parent = exe.parent()?;
    let packaged = parent.join("Models");
    if packaged.is_dir() {
        return Some(packaged);
    }
    let dev = parent.parent()?.parent()?.join("Models");
    if dev.is_dir() {
        return Some(dev);
    }
    None
}

#[cfg_attr(not(test), allow(dead_code))]
fn engine_models_dir_for_bake() -> PathBuf {
    resolve_engine_models_dir().unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("engine_3d parent")
            .join("Models")
    })
}

fn bundled_fbx_path() -> Option<PathBuf> {
    resolve_engine_models_dir().map(|d| d.join(DEFAULT_PLAY_CHARACTER_FBX))
}

fn bundled_rerasset_path() -> Option<PathBuf> {
    resolve_engine_models_dir().map(|d| d.join(DEFAULT_PLAY_CHARACTER_RERASSET))
}

fn register_bundled_entry(state: &mut State, source_path: &str, rerasset_path: PathBuf) -> String {
    let model_id = DEFAULT_PLAY_CHARACTER_MODEL_ID.to_string();
    let (size, mtime) = source_fingerprint(Path::new(source_path));
    let category = Some("character".to_string());

    state.imported_model_registry.insert(ImportedModelEntry {
        model_id: model_id.clone(),
        name: DEFAULT_PLAY_CHARACTER_NAME.to_string(),
        category: category.clone(),
        state: AssetState::Ready,
        rerasset_path: rerasset_path.clone(),
        source_path: source_path.to_string(),
        source_size: size,
        source_mtime_secs: mtime,
        importer_version: RER_IMPORTER_VERSION,
    });

    state.model_store.insert(
        source_path.to_string(),
        crate::ipc::ModelStoreEntry {
            name: DEFAULT_PLAY_CHARACTER_NAME.to_string(),
            category,
            model_id: Some(model_id.clone()),
            rerasset_path: Some(rerasset_path.to_string_lossy().into_owned()),
        },
    );

    model_id
}

/// Registra y precarga el mesh base del jugador si aún no está en el registro.
/// Devuelve el `model_id` estable (`model_male_base_mesh`) cuando el asset existe en disco.
pub fn ensure_bundled_default_player_model(state: &mut State) -> Option<String> {
    let model_id = DEFAULT_PLAY_CHARACTER_MODEL_ID.to_string();
    if let Some(entry) = state.imported_model_registry.get(&model_id).cloned() {
        if entry.state == AssetState::Ready && entry.rerasset_path.is_file() {
            use crate::config_3d::static_model_cache::play_character_cache_key;
            let play_key = play_character_cache_key(&model_id);
            let needs_gpu = !state.static_model_cache.contains_key(&model_id)
                && !state.static_model_cache.contains_key(&play_key)
                && !state.model_preload_inflight.contains(&model_id);
            if needs_gpu {
                state.enqueue_gpu_from_rerasset(
                    &model_id,
                    &entry.rerasset_path,
                    DEFAULT_PLAY_CHARACTER_NAME,
                    true,
                );
            }
        }
        return Some(model_id);
    }

    let fbx_path = bundled_fbx_path()?;
    if !fbx_path.is_file() {
        log::warn!(
            "Modelo base del jugador no encontrado: {}",
            fbx_path.display()
        );
        return None;
    }
    let source_key = normalize_source_key(fbx_path.to_string_lossy().as_ref());

    if let Some(rerasset_path) = bundled_rerasset_path().filter(|p| p.is_file()) {
        log::info!(
            "Modelo base del jugador: .rerasset empaquetado {}",
            rerasset_path.display()
        );
        let id = register_bundled_entry(state, &source_key, rerasset_path.clone());
        state.enqueue_gpu_from_rerasset(&id, &rerasset_path, DEFAULT_PLAY_CHARACTER_NAME, true);
        return Some(id);
    }

    log::info!(
        "Modelo base del jugador: importando FBX (sin .rerasset precocido) {}",
        fbx_path.display()
    );
    state.start_imported_model_pipeline(
        &source_key,
        DEFAULT_PLAY_CHARACTER_NAME,
        Some("character"),
    );
    state
        .imported_model_registry
        .model_id_for_path(&source_key)
        .or(Some(model_id))
}

/// Precocina `male_base_mesh.rerasset` junto al FBX empaquetado (herramienta de mantenimiento).
#[cfg_attr(not(test), allow(dead_code))]
pub fn bake_default_player_rerasset_to_disk() -> Result<(), String> {
    use super::bake::{bake_to_rerasset, build_bake_input};
    use crate::config_3d::mesh_3d::{ModelPreloadOptions, preload_model_cpu_bundle};

    let models_dir = engine_models_dir_for_bake();
    let fbx = models_dir.join(DEFAULT_PLAY_CHARACTER_FBX);
    let out = models_dir.join(DEFAULT_PLAY_CHARACTER_RERASSET);
    if !fbx.is_file() {
        return Err(format!("FBX no encontrado: {}", fbx.display()));
    }

    let options = ModelPreloadOptions::library(Some("character"));
    let (parts, anim_asset, play_parts) = preload_model_cpu_bundle(&fbx, options)?;
    let input = build_bake_input(
        &fbx,
        Some("character"),
        &parts,
        play_parts.as_deref(),
        anim_asset.as_deref(),
    );
    bake_to_rerasset(&out, &input)?;
    log::info!(
        "Bake jugador base: {} ({} bytes)",
        out.display(),
        std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0)
    );
    Ok(())
}

#[cfg(test)]
mod bake_tests {
    use super::bake_default_player_rerasset_to_disk;

    /// Genera/actualiza `Engine/Models/male_base_mesh.rerasset`.
    /// `cargo test -p rer-engine-3d bake_default_player_rerasset -- --ignored --nocapture`
    #[test]
    #[ignore = "herramienta de mantenimiento: precocina .rerasset del jugador base"]
    fn bake_default_player_rerasset() {
        bake_default_player_rerasset_to_disk().expect("bake jugador base");
    }
}
