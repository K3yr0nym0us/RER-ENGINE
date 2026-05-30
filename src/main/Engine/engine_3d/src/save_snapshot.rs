use crate::engine::State;
use crate::ipc::{
    send_event, EngineEvent, SaveAssetRefSnapshot, SaveSceneSnapshotPayload, SaveWorldSnapshot,
};
use crate::save_entity_3d::{
    build_config_camera_snapshot, build_config_editor_camera_snapshot, build_entity_3d_snapshot,
    build_player_snapshot,
};

impl State {
    pub(crate) fn export_save_snapshot(&self) {
        let scene = self.build_save_scene_snapshot();
        send_event(&EngineEvent::SaveSnapshotReady { scene });
    }

    fn build_save_scene_snapshot(&self) -> SaveSceneSnapshotPayload {
        let player_id = self.play_character_entity;
        let editor_camera_id = self.editor_camera_entity;
        let mut entities = Vec::new();

        for (id, _name) in self.world.query::<crate::ecs::NameComponent>() {
            if player_id == Some(id) || editor_camera_id == Some(id) {
                continue;
            }
            let Some(meta) = self.resolve_entity_save_meta(id) else {
                continue;
            };
            let Some(t) = self.world.get::<crate::ecs::Transform>(id) else {
                continue;
            };
            entities.push(build_entity_3d_snapshot(self, id, &meta, t));
        }

        entities.sort_by_key(|e| e.id);

        SaveSceneSnapshotPayload {
            world: SaveWorldSnapshot {
                world_width: self.world_bounds_3d.width,
                world_height: self.world_bounds_3d.height,
                world_depth: self.world_bounds_3d.depth,
                grid_visible: self.grid_config.visible,
                grid_cell_size: self.grid_config.cell_size,
                gravity: self.physics.gravity_magnitude(),
                target_fps: self.target_fps,
                light_ambient: Some(self.directional_light_ambient),
                light_intensity: Some(self.light_intensity),
                shadow_darkness: Some(self.shadow_darkness),
            },
            background_path: self.background_path.clone(),
            entities,
            player: build_player_snapshot(self),
            config_camera: build_config_camera_snapshot(self),
            config_editor_camera: build_config_editor_camera_snapshot(self),
            camera2d: None,
            sprites: Vec::new(),
            models: self
                .model_store
                .iter()
                .map(|(path, entry)| SaveAssetRefSnapshot {
                    name: entry.name.clone(),
                    path: path.clone(),
                    category: entry.category.clone(),
                })
                .collect(),
            sounds: self
                .sound_store
                .iter()
                .map(|(path, name)| SaveAssetRefSnapshot {
                    name: name.clone(),
                    path: path.clone(),
                    category: None,
                })
                .collect(),
            backgrounds: self
                .background_store
                .iter()
                .map(|(path, name)| SaveAssetRefSnapshot {
                    name: name.clone(),
                    path: path.clone(),
                    category: None,
                })
                .collect(),
        }
    }

    pub(crate) fn animation_to_snapshot(
        &self,
        entity_id: u32,
        name: &str,
        anim: &crate::engine::AnimationState,
    ) -> crate::ipc::SaveAnimationSnapshot {
        use crate::ipc::{SaveAnimationFrameSnapshot, SaveAnimationSnapshot, SaveScriptSnapshot};

        let is_default = self
            .default_animation_by_entity
            .get(&entity_id)
            .map(|d| d == name)
            .unwrap_or(false);
        let facing_right = Some(!anim.flip_horizontal);

        SaveAnimationSnapshot {
            name: name.to_string(),
            fps: anim.fps,
            loop_: anim.loop_,
            is_default: if is_default { Some(true) } else { None },
            facing_right,
            logical_w: anim.logical_w,
            logical_h: anim.logical_h,
            audio_path: None,
            frames: anim
                .frames
                .iter()
                .map(|f| SaveAnimationFrameSnapshot {
                    path: f.path.clone(),
                    pivot_x: f.pivot_x,
                    pivot_y: f.pivot_y,
                    src_x: f.src_x,
                    src_y: f.src_y,
                    src_w: f.src_w,
                    src_h: f.src_h,
                })
                .collect(),
            scripts: anim
                .scripts
                .iter()
                .map(|s| SaveScriptSnapshot {
                    name: s.name.clone(),
                    source: s.source.clone(),
                })
                .collect(),
            is_cancelable: if anim.is_cancelable { Some(true) } else { None },
            embedded_in_model: None,
        }
    }
}
