use super::*;

impl State {
    pub(super) fn cmd_scenes(&mut self, cmd: EngineCommand) -> Option<EngineCommand> {
        match cmd {
            EngineCommand::Common(EngineCommandCommon::SetScene { scene, save_path }) => {
                match scene.as_str() {
                    "3D" | "first-person" | "second-person" | "third-person" => {
                        if let Some(path) = save_path.filter(|p| !p.trim().is_empty()) {
                            if std::path::Path::new(&path).is_dir() {
                                self.load_proyect_from_save_path(&path);
                            } else {
                                log::warn!(
                                    "[SetScene] se esperaba directorio extraído, no archivo: {path}"
                                );
                                self.setup_default_3d_scene();
                            }
                        } else {
                            self.setup_default_3d_scene();
                        }
                    }
                    "2D" => {
                        let _ = save_path;
                        log::warn!("SetScene '2D' ignorado en rer_engine_3d");
                    }
                    _ => log::info!("SetScene: escena '{}' no reconocida", scene),
                }
            }
            EngineCommand::Common(EngineCommandCommon::GetDefaultSceneName { id }) => {
                let name = rer_engine_core::editor_defaults::default_scene_name(id);
                send_event(&EngineEvent::DefaultSceneNameReady { id, name });
            }
            EngineCommand::Only3d(EngineCommand3dOnly::CreateEditorScene { name }) => {
                self.handle_create_editor_scene(&name);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SwitchEditorScene { scene_id }) => {
                self.handle_switch_editor_scene(scene_id);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::DeleteEditorScene { scene_id }) => {
                self.handle_delete_editor_scene(scene_id);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::NotifyProjectSaved { extract_dir }) => {
                self.handle_notify_project_saved(&extract_dir);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::ClearEditorUndoRedo) => {
                self.clear_editor_undo_redo();
            }
            other => return Some(other),
        }
        None
    }
}
