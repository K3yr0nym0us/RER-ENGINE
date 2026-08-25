use glam::{Quat, Vec3 as GlamVec3};

use crate::ecs::{MeshComponent, NonSelectable, Transform};
use crate::ipc::{EngineEvent, send_event};

use super::State;
use super::types::EntityUndoSnapshot;

impl State {
    pub(crate) fn capture_entity_undo_snapshot(&self, id: u32) -> Option<EntityUndoSnapshot> {
        if self.background_entity == Some(id)
            || self.quick_build_ghost_id == Some(id)
            || Some(id) == self.sun_entity
            || Some(id) == self.play_character_entity
            || self.world.get::<NonSelectable>(id).is_some()
        {
            return None;
        }

        let t = self.world.get::<Transform>(id)?;
        let mesh = self.world.get::<MeshComponent>(id)?.clone();
        let save_meta = self.resolve_entity_save_meta(id)?;
        let name = self
            .world
            .get::<crate::ecs::NameComponent>(id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| format!("Entity {id}"));

        let scale = t.scale.to_array();
        let physics_enabled = self.physics.has_physics(id);
        let physics_type = self.physics.get_body_type(id).to_string();
        let physics_half = [scale[0] * 0.5, scale[1] * 0.5, scale[2] * 0.5];

        Some(EntityUndoSnapshot {
            id,
            name,
            transform_position: t.position.to_array(),
            transform_rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
            transform_scale: scale,
            mesh,
            save_meta,
            physics_enabled,
            physics_type,
            physics_half,
            in_character_list: self.character_entities.contains(&id),
            in_scenario_list: self.scenario_entities.contains(&id),
        })
    }

    pub(crate) fn push_remove_entity_undo(&mut self, id: u32) {
        let Some(snapshot) = self.capture_entity_undo_snapshot(id) else {
            log::warn!("[undo] entidad {id} sin snapshot — redo no disponible");
            return;
        };
        if !self.is_applying_undo {
            self.redo_stack.clear();
        }
        self.undo_stack
            .push(super::types::UndoAction::RemoveEntity { snapshot });
        self.sync_editor_scenes_undo_dirty_to_renderer();
    }

    pub(crate) fn restore_entity_from_undo_snapshot(
        &mut self,
        snapshot: &EntityUndoSnapshot,
    ) -> bool {
        if !self.world.spawn_with_id(snapshot.id, Some(&snapshot.name)) {
            log::warn!("[redo] no se pudo reinsertar entidad {}", snapshot.id);
            return false;
        }

        self.world.insert(
            snapshot.id,
            Transform {
                position: GlamVec3::from_array(snapshot.transform_position),
                rotation: Quat::from_xyzw(
                    snapshot.transform_rotation[0],
                    snapshot.transform_rotation[1],
                    snapshot.transform_rotation[2],
                    snapshot.transform_rotation[3],
                ),
                scale: GlamVec3::from_array(snapshot.transform_scale),
            },
        );
        self.world.insert(snapshot.id, snapshot.mesh.clone());

        if snapshot.in_character_list && !self.character_entities.contains(&snapshot.id) {
            self.character_entities.push(snapshot.id);
        }
        if snapshot.in_scenario_list && !self.scenario_entities.contains(&snapshot.id) {
            self.scenario_entities.push(snapshot.id);
        }

        self.save_registry
            .register_meta(snapshot.id, snapshot.save_meta.clone());

        if snapshot.physics_enabled {
            self.physics.set_entity_physics(
                snapshot.id,
                true,
                &snapshot.physics_type,
                snapshot.transform_position,
                snapshot.physics_half,
            );
        }

        self.emit_entity_restored_event(snapshot);
        log::info!("[redo] entidad {} restaurada", snapshot.id);
        true
    }

    fn emit_entity_restored_event(&self, snapshot: &EntityUndoSnapshot) {
        let id = snapshot.id;
        match snapshot.save_meta.kind.as_str() {
            "character" => {
                send_event(&EngineEvent::CharacterLoaded {
                    id,
                    path: snapshot.save_meta.path.clone(),
                });
            }
            _ => {
                self.send_model_loaded_event(id, &snapshot.name);
            }
        }
    }
}
