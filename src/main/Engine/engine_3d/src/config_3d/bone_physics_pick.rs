//! Selección interactiva de huesos para física por hueso.

use crate::ecs::EntityId;
use crate::engine::State;
use crate::ipc::{EngineEvent, send_event};

impl State {
    pub(crate) fn set_bone_physics_editor_entity(&mut self, entity_id: EntityId, active: bool) {
        if active {
            self.bone_physics_editor_entity = Some(entity_id);
        } else if self.bone_physics_editor_entity == Some(entity_id) {
            self.bone_physics_editor_entity = None;
        }
    }

    pub(crate) fn set_bone_physics_pick_mode(&mut self, entity_id: EntityId, active: bool) {
        if active {
            self.socket_bone_pick_entity = None;
            self.socket_bone_pick_hovered_joint = None;
            self.bone_physics_pick_entity = Some(entity_id);
            self.bone_physics_pick_hovered_joint = None;
            self.bone_physics_editor_entity = Some(entity_id);
            self.hovered_gizmo_axis = None;
            self.active_gizmo_axis = None;
        } else if self.bone_physics_pick_entity == Some(entity_id) {
            self.bone_physics_pick_entity = None;
            self.bone_physics_pick_hovered_joint = None;
        }
    }

    pub(crate) fn update_bone_physics_pick_hover(&mut self, pixel_x: f32, pixel_y: f32) {
        let Some(entity_id) = self.bone_physics_pick_entity else {
            return;
        };
        self.bone_physics_pick_hovered_joint = self
            .pick_bone_at_pixel(entity_id, pixel_x, pixel_y)
            .map(|(ji, _)| ji);
    }

    pub(crate) fn try_pick_bone_physics_click(&mut self, pixel_x: f32, pixel_y: f32) -> bool {
        let Some(entity_id) = self.bone_physics_pick_entity else {
            return false;
        };
        let Some((_, bone_name)) = self.pick_bone_at_pixel(entity_id, pixel_x, pixel_y) else {
            return true;
        };
        self.bone_physics_pick_entity = None;
        self.bone_physics_pick_hovered_joint = None;
        send_event(&EngineEvent::BonePhysicsPicked {
            entity_id,
            bone_name,
        });
        true
    }

    pub(crate) fn pick_bone_at_pixel(
        &self,
        entity_id: EntityId,
        pixel_x: f32,
        pixel_y: f32,
    ) -> Option<(usize, String)> {
        const PICK_THRESHOLD_PX: f32 = 22.0;
        let hits = self.collect_joint_screen_hits(entity_id)?;
        let mut best: Option<(f32, usize, String)> = None;
        for hit in hits {
            let dx = hit.screen.0 - pixel_x;
            let dy = hit.screen.1 - pixel_y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= PICK_THRESHOLD_PX && best.as_ref().is_none_or(|(bd, _, _)| dist < *bd) {
                best = Some((dist, hit.joint_index, hit.bone_name));
            }
        }
        best.map(|(_, ji, name)| (ji, name))
    }
}
