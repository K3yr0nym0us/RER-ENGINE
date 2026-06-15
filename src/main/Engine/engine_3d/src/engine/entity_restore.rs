use crate::ipc::{AnimScriptData, AnimationFrameData, ControlBindingsData, EntityRestorePhysics, EntityRestoreTransform, EngineCommand, EngineCommandCommon, SaveAnimationSnapshot, SaveScriptSnapshot};

use super::State;

pub(crate) fn apply_entity_scripts_snapshots(
    state: &mut State,
    id: u32,
    scripts: Option<&[SaveScriptSnapshot]>,
) {
    let Some(list) = scripts else { return };
    for script in list {
        state.handle_command(EngineCommand::Common(EngineCommandCommon::LoadScript {
            id,
            path: script.name.clone(),
            source: script.source.clone(),
        }));
    }
}

pub(crate) fn apply_entity_animations_snapshots(
    state: &mut State,
    id: u32,
    animations: Option<&[SaveAnimationSnapshot]>,
) {
    let Some(list) = animations else { return };
    for anim in list {
        if anim.embedded_in_model == Some(true) || anim.frames.is_empty() {
            continue;
        }
        let frames: Vec<AnimationFrameData> = anim
            .frames
            .iter()
            .map(|f| AnimationFrameData {
                path: f.path.clone(),
                pivot_x: Some(f.pivot_x),
                pivot_y: Some(f.pivot_y),
                src_x: f.src_x,
                src_y: f.src_y,
                src_w: f.src_w,
                src_h: f.src_h,
            })
            .collect();
        let anim_scripts: Vec<AnimScriptData> = anim
            .scripts
            .iter()
            .map(|s| AnimScriptData {
                name: s.name.clone(),
                source: s.source.clone(),
            })
            .collect();
        state.handle_command(EngineCommand::Common(EngineCommandCommon::SetAnimation {
            id,
            name: anim.name.clone(),
            frames,
            fps: anim.fps,
            loop_: anim.loop_,
            flip_horizontal: !(anim.facing_right.unwrap_or(true)),
            audio_path: anim.audio_path.clone(),
            logical_w: Some(anim.logical_w),
            logical_h: Some(anim.logical_h),
            scripts: anim_scripts,
            is_cancelable: anim.is_cancelable.unwrap_or(true),
        }));
        if anim.is_default.unwrap_or(false) {
            state.handle_command(EngineCommand::Common(EngineCommandCommon::SetDefaultAnimation {
                id,
                name: anim.name.clone(),
            }));
        }
    }
}

impl State {
    pub(crate) fn apply_entity_restore_inner(
        &mut self,
        id: u32,
        name: Option<String>,
        transform: &EntityRestoreTransform,
        physics: Option<&EntityRestorePhysics>,
        control_bindings: Option<&ControlBindingsData>,
        omit_scale: bool,
        skip_transform: bool,
    ) {
        if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
            self.handle_command(EngineCommand::Common(EngineCommandCommon::SetEntityName {
                id,
                name,
                force: true,
            }));
        }
        if !skip_transform {
            self.handle_command(EngineCommand::Common(EngineCommandCommon::SetTransform {
                id,
                position: Some(transform.position),
                position_axis: None,
                rotation: Some(transform.rotation),
                scale: if omit_scale {
                    None
                } else {
                    Some(transform.scale)
                },
                scale_axis: None,
                track_undo: Some(false),
                body_rotation_only: None,
                rotation_euler_delta: None,
                rotation_euler_degrees: None,
            }));
        }
        if let Some(physics) = physics {
            if physics.enabled {
                self.handle_command(EngineCommand::Common(EngineCommandCommon::SetPhysics {
                    id,
                    enabled: true,
                    body_type: physics.body_type.clone(),
                }));
            }
        }
        if let Some(bindings) = control_bindings {
            self.handle_command(EngineCommand::Common(EngineCommandCommon::SetControlBindings {
                id,
                bindings: bindings.clone(),
            }));
        }
    }
}
