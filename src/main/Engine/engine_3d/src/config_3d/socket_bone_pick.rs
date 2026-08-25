//! Selección interactiva de huesos para sockets (click + hover en viewport).

use glam::{Mat4, Vec3};

use crate::config_3d::model_animation::asset_joint_globals_with_clip;
use crate::config_3d::model_asset::ModelAsset;
use crate::ecs::{EntityId, Transform};
use crate::engine::State;
use crate::ipc::{EngineEvent, send_event};

pub(crate) struct JointScreenHit {
    pub joint_index: usize,
    pub bone_name: String,
    pub screen: (f32, f32),
}

impl State {
    pub(crate) fn set_socket_bone_pick_mode(&mut self, entity_id: EntityId, active: bool) {
        if active {
            self.bone_physics_pick_entity = None;
            self.bone_physics_pick_hovered_joint = None;
            self.socket_bone_pick_entity = Some(entity_id);
            self.socket_bone_pick_hovered_joint = None;
            self.hovered_gizmo_axis = None;
            self.active_gizmo_axis = None;
        } else if self.socket_bone_pick_entity == Some(entity_id) {
            self.socket_bone_pick_entity = None;
            self.socket_bone_pick_hovered_joint = None;
        }
    }

    pub(crate) fn update_socket_bone_pick_hover(&mut self, pixel_x: f32, pixel_y: f32) {
        let Some(entity_id) = self.socket_bone_pick_entity else {
            return;
        };
        self.socket_bone_pick_hovered_joint = self
            .pick_bone_at_pixel(entity_id, pixel_x, pixel_y)
            .map(|(ji, _)| ji);
    }

    /// Click en viewport durante modo selección de hueso. Devuelve true si consumió el evento.
    pub(crate) fn try_pick_socket_bone_click(&mut self, pixel_x: f32, pixel_y: f32) -> bool {
        let Some(entity_id) = self.socket_bone_pick_entity else {
            return false;
        };
        let Some((_, bone_name)) = self.pick_bone_at_pixel(entity_id, pixel_x, pixel_y) else {
            return true;
        };
        self.socket_bone_pick_entity = None;
        self.socket_bone_pick_hovered_joint = None;
        send_event(&EngineEvent::SocketBonePicked {
            entity_id,
            bone_name,
        });
        true
    }

    pub(crate) fn collect_joint_screen_hits(
        &self,
        entity_id: EntityId,
    ) -> Option<Vec<JointScreenHit>> {
        let (asset, globals, entity_model) = self.entity_skeleton_globals(entity_id)?;
        let joint_count = asset.joint_names.len().min(globals.len());
        let mut out = Vec::with_capacity(joint_count);
        for (ji, global) in globals.iter().enumerate().take(joint_count) {
            let Some(name) = asset.joint_names.get(ji) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let norm = asset.mesh_normalize;
            let local = norm.transform_point3(global.transform_point3(Vec3::ZERO));
            let world = entity_model.transform_point3(local);
            let Some(screen) = self.project_to_screen(world) else {
                continue;
            };
            out.push(JointScreenHit {
                joint_index: ji,
                bone_name: name.clone(),
                screen,
            });
        }
        Some(out)
    }

    pub(crate) fn entity_skeleton_globals(
        &self,
        entity_id: EntityId,
    ) -> Option<(std::sync::Arc<ModelAsset>, Vec<Mat4>, Mat4)> {
        let binding = self.model_animation_bindings.get(&entity_id)?;
        let asset = self.get_model_asset_for_entity(&binding.asset_path, entity_id)?;
        let t = self.world.get::<Transform>(entity_id)?;
        let entity_model = t.to_matrix();

        let (clip, time_s) = self
            .active_model_clips
            .get(&entity_id)
            .filter(|a| a.playing && !a.finished)
            .and_then(|a| {
                asset
                    .clips
                    .iter()
                    .find(|c| c.name == a.clip_name)
                    .map(|clip| (Some(clip), a.time_s))
            })
            .unwrap_or((None, 0.0));

        let globals = asset_joint_globals_with_clip(&asset, clip, time_s);
        Some((asset, globals, entity_model))
    }
}
