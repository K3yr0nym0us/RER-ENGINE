//! Export/import helpers — docs/Entities_Model_3D.yaml

use crate::ecs::Transform;
use crate::engine::State;
use crate::entity_save_meta::EntitySaveMeta;
use crate::ipc::{
    SaveAnimationSnapshot, SaveConfigCameraSnapshot,
    SaveConfigEditorCameraSnapshot, SaveEntity3DSnapshot, SaveScriptSnapshot,
};

pub(crate) fn entity_category_for(state: &State, id: u32, meta: &EntitySaveMeta) -> String {
    if state.sun_entity == Some(id) {
        return "sun".to_string();
    }
    if state.ground_entity_id() == Some(id) {
        return "ground".to_string();
    }
    if state.play_character_entity == Some(id) {
        return "player".to_string();
    }
    if state.character_entities.contains(&id) {
        return "character".to_string();
    }
    if meta.entity_category.as_deref() == Some("environment") {
        return "environment".to_string();
    }
    "object".to_string()
}

pub(crate) fn entity_model_for(meta: &EntitySaveMeta) -> String {
    meta.visual_model_path
        .as_ref()
        .filter(|p| !p.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| meta.path.clone())
}

pub(crate) fn entity_colision_for(state: &State, id: u32) -> bool {
    state.entity_colision.get(&id).copied().unwrap_or(true)
}

pub(crate) fn build_entity_3d_snapshot(
    state: &State,
    id: u32,
    meta: &EntitySaveMeta,
    t: &Transform,
) -> SaveEntity3DSnapshot {
    let physics_type = if state.physics.has_physics(id) {
        Some(state.physics.get_body_type(id).to_string())
    } else {
        None
    };

    let animations = {
        let mut list: Vec<SaveAnimationSnapshot> = state
            .animations
            .get(&id)
            .map(|map| {
                map.iter()
                    .map(|(anim_name, anim)| state.animation_to_snapshot(id, anim_name, anim))
                    .collect()
            })
            .unwrap_or_default();
        if list.is_empty() {
            if let Some(binding) = state.model_animation_bindings.get(&id) {
                if let Some(asset) = state.model_assets.get(&binding.asset_path) {
                    let default_name = state.model_clip_defaults.get(&id);
                    list = asset
                        .clips
                        .iter()
                        .map(|c| SaveAnimationSnapshot {
                            name: c.name.clone(),
                            fps: c.fps.round() as u32,
                            loop_: true,
                            is_default: default_name
                                .map(|d| d == &c.name)
                                .filter(|&b| b)
                                .map(|_| true),
                            facing_right: None,
                            logical_w: 1,
                            logical_h: 1,
                            audio_path: None,
                            frames: vec![],
                            scripts: vec![],
                            is_cancelable: None,
                            embedded_in_model: Some(true),
                        })
                        .collect();
                }
            }
        }
        if list.is_empty() {
            None
        } else {
            Some(list)
        }
    };

    let scripts = {
        let list: Vec<SaveScriptSnapshot> = state
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

    let controls = state.control_bindings_by_entity.get(&id).cloned();
    let category = entity_category_for(state, id, meta);
    let controls = if category == "player" {
        controls
    } else {
        None
    };

    SaveEntity3DSnapshot {
        id,
        name: state
            .entity_display_name(id)
            .unwrap_or_else(|| format!("Entity {id}")),
        category,
        model: entity_model_for(meta),
        position: t.position.to_array(),
        rotation: [
            t.rotation.x,
            t.rotation.y,
            t.rotation.z,
            t.rotation.w,
        ],
        scale: t.scale.to_array(),
        physics_type,
        colision: entity_colision_for(state, id),
        animations,
        scripts,
        blueprint_id: state.entity_blueprint_ids.get(&id).cloned(),
        controls,
    }
}

pub(crate) fn build_player_snapshot(state: &State) -> Option<SaveEntity3DSnapshot> {
    let id = state.play_character_entity?;
    let meta = state.resolve_entity_save_meta(id)?;
    let t = state.world.get::<Transform>(id)?;
    Some(build_entity_3d_snapshot(state, id, &meta, t))
}

pub(crate) fn build_config_camera_snapshot(state: &State) -> Option<SaveConfigCameraSnapshot> {
    if !state.has_play_character() {
        return None;
    }
    let (yaw, pitch) = if state.uses_editor_viewport_camera() {
        (state.editor_viewport_yaw, state.editor_viewport_pitch)
    } else {
        (state.camera.yaw, state.camera.pitch)
    };
    Some(SaveConfigCameraSnapshot {
        camera_eye_position: Some(state.play_camera_eye_position.to_array()),
        fps_camera_yaw: Some(state.camera.yaw),
        fps_camera_pitch: Some(state.camera.pitch),
        yaw,
        pitch,
        fov_y: state.camera.fov_y,
        frustum_distance: state.fps_editor_frustum_distance,
        camera_follow_mode: state.play_camera_follow_mode,
    })
}

pub(crate) fn build_config_editor_camera_snapshot(
    state: &State,
) -> Option<SaveConfigEditorCameraSnapshot> {
    let id = state.editor_camera_entity?;
    let t = state.world.get::<Transform>(id)?;
    Some(SaveConfigEditorCameraSnapshot {
        position: t.position.to_array(),
        rotation: Some([
            t.rotation.x,
            t.rotation.y,
            t.rotation.z,
            t.rotation.w,
        ]),
    })
}
