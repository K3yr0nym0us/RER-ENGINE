use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::engine3d::ipc::{
    AnimScriptData, AnimationFrameData, ControlBindingsData, ControlScriptData, EngineCommand,
    EngineCommand3dOnly, EngineCommandCommon, EngineEvent, EntityRestorePhysics,
    EntityRestoreTransform, ImportSceneSprite, ProjectLoaded3dEvent, ProjectLoaded3dSceneTab,
    ProjectLoaded3dWorld, send_event, send_load_progress, send_project_load_3d_complete_event,
    send_project_loaded_3d_event,
};

use super::State;
use crate::engine3d::assets::registry::{
    relative_rerasset_manifest_path, resolve_manifest_asset_path,
};
use crate::engine3d::config_3d::static_model_cache::MODEL_GPU_PARTS_DURING_SAVE_LOAD;
use rer_engine_core::assets::AssetState;

const SCRIPT_FILE_PREFIX: &str = "@file:";
const DEFAULT_LIGHT_AMBIENT: f32 = 0.06;
const DEFAULT_LIGHT_INTENSITY: f32 = 1.0;
const DEFAULT_SHADOW_DARKNESS: f32 =
    crate::engine3d::config_3d::directional_light::DEFAULT_SHADOW_DARKNESS;
const ENTITY_MARKERS: &[&str] = &[
    "[EditorBox]",
    "[Ground]",
    "[Player]",
    "[EditorCamera]",
    "[Sun]",
    "[Ball]",
    "[Colisionador]",
    "[ExecutionArea]",
];

// ── Manifest: nombres de campo = claves JSON en `src/shared-types/types.ts`. ─

mod events;
mod manifest;
mod paths;
mod restore;
mod scene;
mod schema;
mod snapshot;

pub(crate) use self::events::send_project_loaded_3d_with_editor_scenes;
pub(crate) use self::manifest::load_project_from_extract_dir;
pub(crate) use self::schema::*;
pub(crate) use self::snapshot::{
    build_fp_placeholder_saved_scene, build_minimal_project_from_store,
    saved_scene_from_active_view, saved_scene_from_snapshot_payload,
};
impl State {
    /// Carga proyecto 3D desde la carpeta ya extraída por Electron (manifest + assets).
    pub(crate) fn load_proyect_from_save_path(&mut self, extract_path: &str) {
        let path = if extract_path.trim().is_empty() {
            std::env::var("RER_PROJECT_EXTRACT_DIR").unwrap_or_default()
        } else {
            extract_path.to_string()
        };
        match manifest::load_project_from_extract_dir(&path) {
            Ok(mut project) => {
                if project.r#type != "3D" {
                    log::warn!("tipo '{}' ignorado en binario 3D", project.r#type);
                    return;
                }
                let extract_dir = PathBuf::from(&path);
                manifest::log_load_manifest_summary(&project, &extract_dir);
                paths::resolve_loaded_paths(&mut project, &extract_dir);
                let extract_path = path.clone();
                match apply_loaded_proyect_3d(self, &project) {
                    Ok(view) => {
                        events::send_project_loaded_3d(&project, &view, None);
                        self.editor_scenes_init_from_project(&project, &extract_path, &view);
                        self.sync_active_editor_scene_committed();
                        self.clear_editor_undo_redo();
                        self.emit_editor_scenes_updated("project_loaded");
                        send_project_load_3d_complete_event();
                        log::info!("[load-pack] carga .save completada");
                    }
                    Err(err) => {
                        log::error!("[load-pack] error al aplicar proyecto: {err}");
                    }
                }
            }
            Err(err) => log::error!("[load-pack] error al abrir '{path}': {err}"),
        }
    }
}

pub(crate) fn apply_loaded_proyect_3d(
    state: &mut State,
    project: &ProjectSaveData,
) -> Result<ActiveSaveView, String> {
    let result = scene::apply_loaded_proyect_3d_with_scene(state, project, None);
    if result.is_ok()
        && let Some(renderer) = state.frame_renderer.as_mut()
    {
        renderer.reset_taa_history();
    }
    result
}

pub(crate) fn apply_editor_scene_switch(
    state: &mut State,
    project: &ProjectSaveData,
    scene: &SavedScene,
) -> Result<ActiveSaveView, String> {
    let result = scene::apply_loaded_proyect_3d_with_scene(state, project, Some(scene));
    if result.is_ok()
        && let Some(renderer) = state.frame_renderer.as_mut()
    {
        renderer.reset_taa_history();
    }
    result
}
