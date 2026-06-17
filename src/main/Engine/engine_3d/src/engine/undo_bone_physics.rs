//! Undo/redo de física por hueso.

use crate::config_3d::bone_physics::BonePhysicsMode;
use crate::ecs::EntityId;

use super::State;

impl State {
    pub(crate) fn apply_undo_bone_physics(
        &mut self,
        entity_id: EntityId,
        bone_name: &str,
        mode: Option<BonePhysicsMode>,
    ) {
        match mode {
            None => self.remove_bone_physics_entry(entity_id, bone_name),
            Some(m) => {
                let _ = self.set_bone_physics_no_undo(entity_id, bone_name, m);
            }
        }
        self.emit_entity_bone_physics_changed(entity_id);
    }

    pub(crate) fn set_bone_physics_no_undo(
        &mut self,
        entity_id: EntityId,
        bone_name: &str,
        mode: BonePhysicsMode,
    ) -> Result<(), String> {
        let bone_name = bone_name.trim();
        if bone_name.is_empty() {
            return Err("Nombre de hueso vacío.".to_string());
        }
        if mode == BonePhysicsMode::None {
            self.remove_bone_physics_entry(entity_id, bone_name);
        } else {
            let list = self.entity_bone_physics.entry(entity_id).or_default();
            if let Some(existing) = list
                .iter_mut()
                .find(|e| e.bone_name.eq_ignore_ascii_case(bone_name))
            {
                existing.mode = mode;
                existing.bone_name = bone_name.to_string();
            } else {
                list.push(crate::config_3d::bone_physics::BonePhysicsEntry {
                    bone_name: bone_name.to_string(),
                    mode,
                });
            }
            self.clear_bone_physics_sim_for_entity_bone(entity_id, bone_name);
        }
        Ok(())
    }

    pub(crate) fn current_bone_physics_mode(
        &self,
        entity_id: EntityId,
        bone_name: &str,
    ) -> Option<BonePhysicsMode> {
        self.entity_bone_physics.get(&entity_id).and_then(|list| {
            list.iter()
                .find(|e| e.bone_name.eq_ignore_ascii_case(bone_name))
                .map(|e| e.mode)
        })
    }
}
