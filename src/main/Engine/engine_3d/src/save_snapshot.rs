use crate::engine::State;
use crate::assets::registry::relative_rerasset_manifest_path;
use rer_engine_shared::assets::AssetState;
use crate::ipc::{
    send_event, EngineEvent, SaveAssetRefSnapshot, SavePlayerUiTextBoxSnapshot,
    SaveSceneSnapshotPayload, SaveWorldSnapshot,
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

    pub(crate) fn build_save_scene_snapshot(&self) -> SaveSceneSnapshotPayload {
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
                .filter_map(|(path, entry)| {
                    let model_id = entry.model_id.as_ref()?;
                    let reg = self.imported_model_registry.get(model_id)?;
                    if reg.state != AssetState::Ready {
                        log::debug!(
                            "[save] biblioteca omitida (import no listo): {} ({})",
                            entry.name,
                            model_id
                        );
                        return None;
                    }
                    if !reg.rerasset_path.is_file() {
                        log::warn!(
                            "[save] biblioteca omitida (sin .rerasset): {} ({})",
                            entry.name,
                            model_id
                        );
                        return None;
                    }
                    Some(SaveAssetRefSnapshot {
                        name: entry.name.clone(),
                        path: path.clone(),
                        category: entry.category.clone(),
                        model_id: Some(model_id.clone()),
                        asset: Some(relative_rerasset_manifest_path(model_id)),
                    })
                })
                .collect(),
            sounds: self
                .sound_store
                .iter()
                .map(|(path, name)| SaveAssetRefSnapshot {
                    name: name.clone(),
                    path: path.clone(),
                    category: None,
                    model_id: None,
                    asset: None,
                })
                .collect(),
            backgrounds: self
                .background_store
                .iter()
                .map(|(path, name)| SaveAssetRefSnapshot {
                    name: name.clone(),
                    path: path.clone(),
                    category: None,
                    model_id: None,
                    asset: None,
                })
                .collect(),
            fonts: self
                .font_store
                .iter()
                .map(|(path, name)| SaveAssetRefSnapshot {
                    name: name.clone(),
                    path: path.clone(),
                    category: None,
                    model_id: None,
                    asset: None,
                })
                .collect(),
            hud_images: self
                .hud_image_store
                .iter()
                .map(|(path, meta)| SaveAssetRefSnapshot {
                    name: meta.name.clone(),
                    path: path.clone(),
                    category: None,
                    model_id: None,
                    asset: None,
                })
                .collect(),
            player_ui_text_boxes: self.export_player_ui_text_boxes_snapshot(),
            player_ui_buttons: self.export_player_ui_buttons_snapshot(),
            player_ui_images: self.export_player_ui_images_snapshot(),
            player_ui_objects: self.export_player_ui_objects_snapshot(),
        }
    }

    fn export_player_ui_text_boxes_snapshot(&self) -> Vec<SavePlayerUiTextBoxSnapshot> {
        let mut out = Vec::new();
        for (key, boxes) in &self.player_ui_text_boxes {
            let Some((scope, screen_id)) = key.split_once(':') else {
                continue;
            };
            for b in boxes {
                out.push(SavePlayerUiTextBoxSnapshot {
                    scope: scope.to_string(),
                    screen_id: screen_id.to_string(),
                    id: b.id,
                    font_path: b.font_path.clone(),
                    font_name: b.font_name.clone(),
                    text: b.text.clone(),
                    center_x: b.center_x,
                    center_y: b.center_y,
                    width: b.width,
                    height: b.height,
                    z_index: b.z_index,
                    locked: b.locked,
                });
            }
        }
        out.sort_by(|a, b| {
            (a.scope.as_str(), a.screen_id.as_str(), a.id).cmp(&(
                b.scope.as_str(),
                b.screen_id.as_str(),
                b.id,
            ))
        });
        out
    }

    fn export_player_ui_buttons_snapshot(&self) -> Vec<crate::ipc::SavePlayerUiButtonSnapshot> {
        let mut out = Vec::new();
        for (key, buttons) in &self.player_ui_buttons {
            let Some((scope, screen_id)) = key.split_once(':') else {
                continue;
            };
            for b in buttons {
                out.push(crate::ipc::SavePlayerUiButtonSnapshot {
                    scope: scope.to_string(),
                    screen_id: screen_id.to_string(),
                    id: b.id,
                    shape_type: b.shape_type.clone(),
                    round: b.round,
                    background_color: b.background_color,
                    texture_path: b.texture_path.clone(),
                    transparency_background: b.transparency_background,
                    text: b.text.clone(),
                    text_color: b.text_color,
                    transparency_text: b.transparency_text,
                    font_path: b.font_path.clone(),
                    font_name: b.font_name.clone(),
                    border_color: b.border_color,
                    border_weight: b.border_weight,
                    center_x: b.center_x,
                    center_y: b.center_y,
                    width: b.width,
                    height: b.height,
                    source_aspect: Some(b.source_aspect),
                    z_index: b.z_index,
                    locked: b.locked,
                });
            }
        }
        out.sort_by(|a, b| {
            (a.scope.as_str(), a.screen_id.as_str(), a.id).cmp(&(
                b.scope.as_str(),
                b.screen_id.as_str(),
                b.id,
            ))
        });
        out
    }

    fn export_player_ui_images_snapshot(&self) -> Vec<crate::ipc::SavePlayerUiImageSnapshot> {
        let mut out = Vec::new();
        for (key, images) in &self.player_ui_images {
            let Some((scope, screen_id)) = key.split_once(':') else {
                continue;
            };
            for img in images {
                out.push(crate::ipc::SavePlayerUiImageSnapshot {
                    scope: scope.to_string(),
                    screen_id: screen_id.to_string(),
                    id: img.id,
                    image_path: img.image_path.clone(),
                    image_name: img.image_name.clone(),
                    center_x: img.center_x,
                    center_y: img.center_y,
                    width: img.width,
                    height: img.height,
                    source_aspect: img.source_aspect,
                    z_index: img.z_index,
                    locked: img.locked,
                });
            }
        }
        out.sort_by(|a, b| {
            (a.scope.as_str(), a.screen_id.as_str(), a.id).cmp(&(
                b.scope.as_str(),
                b.screen_id.as_str(),
                b.id,
            ))
        });
        out
    }

    fn export_player_ui_objects_snapshot(&self) -> Vec<crate::ipc::SavePlayerUiObjectSnapshot> {
        let mut out = Vec::new();
        for (key, objects) in &self.player_ui_objects {
            let Some((scope, screen_id)) = key.split_once(':') else {
                continue;
            };
            for obj in objects {
                if obj.vertices.len() < 3 {
                    continue;
                }
                out.push(crate::ipc::SavePlayerUiObjectSnapshot {
                    scope: scope.to_string(),
                    screen_id: screen_id.to_string(),
                    id: obj.id,
                    vertices: obj.vertices.clone(),
                    fill_color: obj.fill_color,
                    texture_path: obj.texture_path.clone(),
                    z_index: obj.z_index,
                    locked: obj.locked,
                });
            }
        }
        out.sort_by(|a, b| {
            (a.scope.as_str(), a.screen_id.as_str(), a.id).cmp(&(
                b.scope.as_str(),
                b.screen_id.as_str(),
                b.id,
            ))
        });
        out
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
