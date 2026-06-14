//! Registro de escenas del editor 3D, baselines y cambio de escena activa (bloqueo si hay undo pendiente).

use std::collections::HashMap;

use super::load_proyect::{
    self, ActiveSaveView, ProjectSaveData, SavedScene,
};
use super::State;
use crate::ipc::{send_event, EditorSceneListItem, EngineEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineKind {
    FpBootTemplate,
    FpPlaceholder,
    SavedManifest,
}

#[derive(Debug, Clone)]
pub struct EditorSceneRecord {
    #[allow(dead_code)]
    pub id: u32,
    pub name: String,
    #[allow(dead_code)]
    pub baseline_kind: BaselineKind,
    pub baseline: SavedScene,
    pub committed: Option<SavedScene>,
}

#[derive(Debug, Default)]
pub struct EditorSceneStore {
    pub records: HashMap<u32, EditorSceneRecord>,
    pub active_scene_id: u32,
    pub next_scene_id: u32,
    pub extract_dir: Option<String>,
    pub project_saved_once: bool,
    pub game_style: String,
    pub(crate) cached_project: Option<ProjectSaveData>,
}

impl EditorSceneStore {
    pub fn new() -> Self {
        Self {
            active_scene_id: 1,
            next_scene_id: 2,
            game_style: "first-person".to_string(),
            ..Default::default()
        }
    }

    pub fn list_items(&self, active_scene_has_undo: bool) -> Vec<EditorSceneListItem> {
        let mut ids: Vec<u32> = self.records.keys().copied().collect();
        ids.sort_unstable();
        ids.into_iter()
            .map(|id| {
                let rec = &self.records[&id];
                EditorSceneListItem {
                    id,
                    name: rec.name.clone(),
                    dirty: id == self.active_scene_id && active_scene_has_undo,
                }
            })
            .collect()
    }
}

impl State {
    pub(crate) fn editor_scene_list_items(&self) -> Vec<EditorSceneListItem> {
        self.editor_scenes
            .list_items(!self.undo_stack.is_empty())
    }

    pub(crate) fn emit_editor_scenes_updated(&self, update_reason: &str) {
        send_event(&EngineEvent::EditorScenesUpdated {
            active_scene_id: self.editor_scenes.active_scene_id,
            scenes: self.editor_scene_list_items(),
            update_reason: update_reason.to_string(),
        });
    }

    /// Refresca `dirty` de la escena activa en el renderer (pila undo del mundo).
    pub(crate) fn sync_editor_scenes_undo_dirty_to_renderer(&self) {
        if self.editor_scenes.records.is_empty() {
            return;
        }
        self.emit_editor_scenes_updated("undo_state");
    }

    pub(crate) fn editor_scenes_init_from_boot(&mut self, scene_name: &str) {
        let snapshot = self.build_save_scene_snapshot();
        let scene = load_proyect::saved_scene_from_snapshot_payload(&snapshot, 1, scene_name);
        let mut store = EditorSceneStore::new();
        store.records.insert(
            1,
            EditorSceneRecord {
                id: 1,
                name: scene_name.to_string(),
                baseline_kind: BaselineKind::FpBootTemplate,
                baseline: scene.clone(),
                committed: Some(scene),
            },
        );
        store.active_scene_id = 1;
        store.next_scene_id = 2;
        self.editor_scenes = store;
        self.clear_editor_undo_redo();
        self.emit_editor_scenes_updated("boot");
    }

    pub(crate) fn editor_scenes_init_from_project(
        &mut self,
        project: &ProjectSaveData,
        extract_dir: &str,
        active_view: &ActiveSaveView,
    ) {
        let mut store = EditorSceneStore::new();
        store.extract_dir = Some(extract_dir.to_string());
        store.project_saved_once = true;
        store.game_style = project.gameStyle.clone();
        store.cached_project = Some(project.clone());
        store.active_scene_id = active_view.sceneId;

        if project.scenes.is_empty() {
            let scene = load_proyect::saved_scene_from_active_view(active_view);
            let scene_id = scene.id;
            store.records.insert(
                scene_id,
                EditorSceneRecord {
                    id: scene_id,
                    name: scene.name.clone(),
                    baseline_kind: BaselineKind::SavedManifest,
                    baseline: scene.clone(),
                    committed: Some(scene),
                },
            );
            store.next_scene_id = scene_id.saturating_add(1);
        } else {
            let mut max_id = 0u32;
            for s in &project.scenes {
                max_id = max_id.max(s.id);
                store.records.insert(
                    s.id,
                    EditorSceneRecord {
                        id: s.id,
                        name: s.name.clone(),
                        baseline_kind: BaselineKind::SavedManifest,
                        baseline: s.clone(),
                        committed: None,
                    },
                );
            }
            store.next_scene_id = max_id.saturating_add(1);
        }
        self.editor_scenes = store;
        self.sync_active_editor_scene_committed();
    }

    /// Alinea `committed` de la escena activa con el mundo actual del motor (post-carga).
    pub(crate) fn sync_active_editor_scene_committed(&mut self) {
        let active_id = self.editor_scenes.active_scene_id;
        let name = self
            .editor_scenes
            .records
            .get(&active_id)
            .map(|r| r.name.clone())
            .unwrap_or_default();
        let snapshot = self.build_save_scene_snapshot();
        let scene =
            load_proyect::saved_scene_from_snapshot_payload(&snapshot, active_id, &name);
        if let Some(rec) = self.editor_scenes.records.get_mut(&active_id) {
            rec.committed = Some(scene);
        }
        
    }

    pub(crate) fn handle_create_editor_scene(&mut self, name: &str) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return;
        }
        let id = self.editor_scenes.next_scene_id;
        self.editor_scenes.next_scene_id = id.saturating_add(1);
        let scene_name = trimmed.to_string();
        let baseline = load_proyect::build_fp_placeholder_saved_scene(id, &scene_name);
        self.editor_scenes.records.insert(
            id,
            EditorSceneRecord {
                id,
                name: scene_name.clone(),
                baseline_kind: BaselineKind::FpPlaceholder,
                baseline: baseline.clone(),
                committed: None,
            },
        );
        send_event(&EngineEvent::EditorSceneCreated {
            id,
            name: scene_name,
            scenes: self.editor_scene_list_items(),
        });
    }

    pub(crate) fn handle_delete_editor_scene(&mut self, scene_id: u32) {
        if self.editor_scenes.records.len() <= 1 {
            return;
        }
        if !self.editor_scenes.records.contains_key(&scene_id) {
            return;
        }
        let was_active = self.editor_scenes.active_scene_id == scene_id;
        self.editor_scenes.records.remove(&scene_id);
        if was_active {
            let next_id = *self
                .editor_scenes
                .records
                .keys()
                .min()
                .expect("delete: queda al menos una escena");
            self.switch_editor_scene_to(next_id, true);
        } else {
            self.emit_editor_scenes_updated("scene_deleted");
        }
    }

    pub(crate) fn handle_switch_editor_scene(&mut self, target_id: u32) {
        self.switch_editor_scene_to(target_id, false);
    }

    fn switch_editor_scene_to(&mut self, target_id: u32, skip_dirty_check: bool) {
        if target_id == self.editor_scenes.active_scene_id {
            return;
        }
        if !self.editor_scenes.records.contains_key(&target_id) {
            log::warn!("[editor_scenes] switch: escena {target_id} no existe");
            return;
        }

        let current_id = self.editor_scenes.active_scene_id;

        if !skip_dirty_check && !self.undo_stack.is_empty() {
            
            send_event(&EngineEvent::EditorSceneSwitchBlocked {
                reason: "unsaved_changes".to_string(),
                active_scene_id: current_id,
                target_scene_id: target_id,
            });
            return;
        }

        let snapshot = self.build_save_scene_snapshot();
        let current_name = self.editor_scenes.records[&current_id].name.clone();
        let current_scene =
            load_proyect::saved_scene_from_snapshot_payload(&snapshot, current_id, &current_name);
        if let Some(r) = self.editor_scenes.records.get_mut(&current_id) {
            r.committed = Some(current_scene);
        }

        let target_scene = {
            let rec = &self.editor_scenes.records[&target_id];
            rec.committed
                .clone()
                .unwrap_or_else(|| rec.baseline.clone())
        };

        let project = if let Some(p) = self.editor_scenes.cached_project.clone() {
            p
        } else if let Some(ref dir) = self.editor_scenes.extract_dir.clone() {
            match load_proyect::load_project_from_extract_dir(dir) {
                Ok(p) => {
                    self.editor_scenes.cached_project = Some(p.clone());
                    p
                }
                Err(e) => {
                    log::error!("[editor_scenes] switch: {e}");
                    return;
                }
            }
        } else {
            let scenes: Vec<SavedScene> = self
                .editor_scenes
                .records
                .values()
                .map(|r| r.committed.clone().unwrap_or_else(|| r.baseline.clone()))
                .collect();
            load_proyect::build_minimal_project_from_store(&self.editor_scenes.game_style, &scenes)
        };

        match load_proyect::apply_editor_scene_switch(self, &project, &target_scene) {
            Ok(view) => {
                self.editor_scenes.active_scene_id = target_id;
                if let Some(r) = self.editor_scenes.records.get_mut(&target_id) {
                    if r.committed.is_none() {
                        r.committed = Some(load_proyect::saved_scene_from_active_view(&view));
                    }
                }
                let editor_tabs = self.editor_scene_list_items();
                load_proyect::send_project_loaded_3d_with_editor_scenes(
                    &project,
                    &view,
                    &editor_tabs,
                );
                send_project_load_3d_complete_event();
                self.clear_editor_undo_redo();
                send_event(&EngineEvent::EditorSceneSwitched {
                    active_scene_id: target_id,
                    scenes: self.editor_scene_list_items(),
                });
            }
            Err(err) => log::error!("[editor_scenes] switch apply: {err}"),
        }
    }

    pub(crate) fn handle_notify_project_saved(&mut self, extract_dir: &str) {
        match load_proyect::load_project_from_extract_dir(extract_dir) {
            Ok(project) => {
                self.editor_scenes.extract_dir = Some(extract_dir.to_string());
                self.editor_scenes.project_saved_once = true;
                self.editor_scenes.cached_project = Some(project.clone());
                self.editor_scenes.game_style = project.gameStyle.clone();
                for scene in &project.scenes {
                    let committed = self
                        .editor_scenes
                        .records
                        .get(&scene.id)
                        .and_then(|r| r.committed.clone());
                    self.editor_scenes.records.insert(
                        scene.id,
                        EditorSceneRecord {
                            id: scene.id,
                            name: scene.name.clone(),
                            baseline_kind: BaselineKind::SavedManifest,
                            baseline: scene.clone(),
                            committed,
                        },
                    );
                }
                self.clear_editor_undo_redo();
                self.emit_editor_scenes_updated("project_saved");
            }
            Err(e) => log::error!("[editor_scenes] notify_project_saved: {e}"),
        }
    }
}

use crate::ipc::send_project_load_3d_complete_event;
