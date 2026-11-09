use super::*;

impl State {
    pub(super) fn cmd_scripts(&mut self, cmd: EngineCommand) -> Option<EngineCommand> {
        match cmd {
            EngineCommand::Common(EngineCommandCommon::LoadSceneVisualScript {
                scene_id,
                source,
            }) => {
                log::info!("[IPC] LoadSceneVisualScript: scene_id={}", scene_id);
                if let Err(e) = self.handle_load_scene_visual_script(scene_id, &source) {
                    log::error!("[scene_script] Error: {e}");
                    send_event(&EngineEvent::Error {
                        message: format!("Error en script de escena: {e}"),
                    });
                }
            }
            EngineCommand::Common(EngineCommandCommon::LoadScript { id, path, source }) => {
                log::info!("[IPC] LoadScript: entity_id={} path={}", id, path);
                if let Err(e) = self.script_engine.attach_script(id, &path, &source) {
                    log::error!("[scripting] Error cargando script '{}': {}", path, e);
                    send_event(&EngineEvent::Error {
                        message: format!("Error en script '{path}': {e}"),
                    });
                } else {
                    use crate::engine3d::entity_save_meta::ScriptSourceRecord;
                    let list = self.save_registry.script_sources.entry(id).or_default();
                    if let Some(existing) = list.iter_mut().find(|s| s.name == path) {
                        existing.source = source;
                    } else {
                        list.push(ScriptSourceRecord { name: path, source });
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::SetControlBindings { id, bindings }) => {
                if bindings.keyboard_mouse.is_empty() && bindings.gamepad.is_empty() {
                    self.control_bindings_by_entity.remove(&id);
                } else {
                    self.control_bindings_by_entity.insert(id, bindings);
                }
                self.script_engine.clear_control_script_cache();
            }
            EngineCommand::Common(EngineCommandCommon::RunControlScript {
                id,
                control_key,
                path,
                source,
            }) => {
                self.execute_control_script(id, &control_key, &path, &source);
            }
            EngineCommand::Common(EngineCommandCommon::UnloadScript { id }) => {
                log::info!("[IPC] UnloadScript: entity_id={}", id);
                self.script_engine.detach_entity(id);
                self.save_registry.script_sources.remove(&id);
            }
            other => return Some(other),
        }
        None
    }
}
