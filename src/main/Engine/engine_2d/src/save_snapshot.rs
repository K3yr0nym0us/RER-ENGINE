use crate::ecs::Transform;
use crate::engine::State;
use crate::engine::AnimationState;
use crate::ipc::{
    send_event, EngineEvent, SaveAnimationFrameSnapshot, SaveAnimationSnapshot,
    SaveAssetRefSnapshot, SaveCamera2dSnapshot, SaveEntitySnapshot, SavePlayerTransformSnapshot,
    SaveSceneSnapshotPayload, SaveScriptSnapshot, SaveWorldSnapshot,
};

impl State {
    pub(crate) fn export_save_snapshot(&self) {
        let scene = self.build_save_scene_snapshot();
        send_event(&EngineEvent::SaveSnapshotReady { scene });
    }

    fn build_save_scene_snapshot(&self) -> SaveSceneSnapshotPayload {
        let player_id = self.find_player_entity();
        let mut entities = Vec::new();

        for (id, _name) in self.world.query::<crate::ecs::NameComponent>() {
            if player_id == Some(id) {
                continue;
            }
            let Some(meta) = self.resolve_entity_save_meta(id) else {
                continue;
            };
            let Some(t) = self.world.get::<Transform>(id) else {
                continue;
            };

            let physics_enabled = self.physics_2d.has_physics(id);
            let physics_type = if physics_enabled {
                Some(self.physics_2d.get_body_type(id).to_string())
            } else {
                None
            };

            let name = self.entity_display_name(id);

            let animations = {
                let list: Vec<SaveAnimationSnapshot> = self
                    .animations
                    .get(&id)
                    .map(|map| {
                        map.iter()
                            .map(|(anim_name, anim)| self.animation_to_snapshot(id, anim_name, anim))
                            .collect()
                    })
                    .unwrap_or_default();
                if list.is_empty() {
                    None
                } else {
                    Some(list)
                }
            };

            let scripts = {
                let list: Vec<SaveScriptSnapshot> = self
                    .save_registry
                    .script_sources
                    .get(&id)
                    .map(|sources| {
                        sources
                            .iter()
                            .map(|s| SaveScriptSnapshot {
                                name: s.name.clone(),
                                source: s.source.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if list.is_empty() {
                    None
                } else {
                    Some(list)
                }
            };

            let control_bindings = self.control_bindings_by_entity.get(&id).cloned();

            entities.push(SaveEntitySnapshot {
                id,
                name,
                kind: meta.kind.clone(),
                path: meta.path.clone(),
                position: t.position.to_array(),
                rotation: [
                    t.rotation.x,
                    t.rotation.y,
                    t.rotation.z,
                    t.rotation.w,
                ],
                scale: t.scale.to_array(),
                physics_enabled: if physics_enabled { Some(true) } else { Some(false) },
                physics_type,
                points: meta.points,
                animations,
                scripts,
                control_bindings,
                visual_model_path: meta.visual_model_path.clone(),
            });
        }

        entities.sort_by_key(|e| e.id);

        let player_transform = player_id.and_then(|id| self.build_player_transform_snapshot(id));

        let camera2d = self.camera_2d.as_ref().map(|c| SaveCamera2dSnapshot {
            x: c.x,
            y: c.y,
            half_h: c.half_h,
        });

        SaveSceneSnapshotPayload {
            world: SaveWorldSnapshot {
                world_width: self.grid_config.world_width,
                world_height: self.grid_config.world_height,
                world_depth: 0.0,
                grid_visible: self.grid_config.visible,
                grid_cell_size: self.grid_config.cell_size,
                gravity: self.physics_2d.gravity_magnitude(),
                target_fps: self.target_fps,
            },
            background_path: self.background_path.clone(),
            entities,
            player_transform,
            camera2d,
            sprites: self
                .sprite_store
                .iter()
                .map(|(path, (name, _, _))| SaveAssetRefSnapshot {
                    name: name.clone(),
                    path: path.clone(),
                })
                .collect(),
            models: Vec::new(),
            sounds: self
                .sound_store
                .iter()
                .map(|(path, name)| SaveAssetRefSnapshot {
                    name: name.clone(),
                    path: path.clone(),
                })
                .collect(),
            backgrounds: self
                .background_store
                .iter()
                .map(|(path, name)| SaveAssetRefSnapshot {
                    name: name.clone(),
                    path: path.clone(),
                })
                .collect(),
        }
    }

    fn find_player_entity(&self) -> Option<u32> {
        self.character_entities
            .iter()
            .copied()
            .find(|&id| self.is_player_entity(id))
    }

    fn build_player_transform_snapshot(&self, id: u32) -> Option<SavePlayerTransformSnapshot> {
        let t = self.world.get::<Transform>(id)?;
        let control_bindings = self.control_bindings_by_entity.get(&id).cloned();
        let visual_model_path = self
            .save_registry
            .meta
            .get(&id)
            .and_then(|m| m.visual_model_path.clone());

        Some(SavePlayerTransformSnapshot {
            position: t.position.to_array(),
            scale: t.scale.to_array(),
            yaw: 0.0,
            pitch: 0.0,
            fov_y: 0.0,
            frustum_distance: 0.0,
            visual_model_path,
            control_bindings,
        })
    }

    fn animation_to_snapshot(
        &self,
        entity_id: u32,
        name: &str,
        anim: &AnimationState,
    ) -> SaveAnimationSnapshot {
        let is_default = self
            .default_animation_by_entity
            .get(&entity_id)
            .map(|d| d == name)
            .unwrap_or(false);
        // Autoría del sprite (no la dirección runtime del personaje al guardar).
        // .save facing_right=true → arte dibujado mirando derecha → flip_horizontal=false en motor.
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
                .map(|f| {
                    let (pivot_x, pivot_y) = f.resolved_pivot(anim.logical_w, anim.logical_h);
                    SaveAnimationFrameSnapshot {
                        path: f.path.clone(),
                        pivot_x,
                        pivot_y,
                        src_x: f.src_x,
                        src_y: f.src_y,
                        src_w: f.src_w,
                        src_h: f.src_h,
                    }
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
        }
    }
}
