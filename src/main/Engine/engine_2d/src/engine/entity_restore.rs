use crate::ipc::{ControlBindingsData, EngineCommand, EngineCommandCommon, EntityRestoreAnimation, EntityRestorePhysics, EntityRestoreScript, EntityRestoreTransform};

use super::State;

impl State {
    pub(crate) fn apply_entity_restore_inner(
        &mut self,
        id: u32,
        name: Option<String>,
        transform: &EntityRestoreTransform,
        physics: Option<&EntityRestorePhysics>,
        animations: Option<&[EntityRestoreAnimation]>,
        scripts: Option<&[EntityRestoreScript]>,
        control_bindings: Option<&ControlBindingsData>,
        omit_scale: bool,
        skip_transform: bool,
        apply_initial_animation_frame: bool,
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
                rotation: Some(transform.rotation),
                scale: if omit_scale {
                    None
                } else {
                    Some(transform.scale)
                },
                track_undo: Some(false),
                position_axis: None,
                scale_axis: None,
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
        if let Some(anims) = animations {
            for anim in anims {
                self.handle_command(EngineCommand::Common(EngineCommandCommon::SetAnimation {
                    id,
                    name: anim.name.clone(),
                    frames: anim.frames.clone(),
                    fps: anim.fps,
                    loop_: anim.loop_,
                    flip_horizontal: anim.flip_horizontal,
                    audio_path: anim.audio_path.clone(),
                    logical_w: None,
                    logical_h: None,
                    scripts: anim.scripts.clone(),
                    is_cancelable: anim.is_cancelable,
                }));
            }
            if let Some(default) = anims.iter().find(|a| a.is_default) {
                self.handle_command(EngineCommand::Common(EngineCommandCommon::SetDefaultAnimation {
                    id,
                    name: default.name.clone(),
                }));
            } else {
                self.handle_command(EngineCommand::Common(EngineCommandCommon::SetDefaultAnimation {
                    id,
                    name: String::new(),
                }));
            }
            if apply_initial_animation_frame {
                let preview_anim = anims.iter().find(|a| a.is_default).or(anims.first());
                if let Some(first_anim) = preview_anim {
                    if let Some(first_frame) = first_anim.frames.first() {
                        let (logical_w, logical_h) = self
                            .animations
                            .get(&id)
                            .and_then(|by_name| by_name.get(&first_anim.name))
                            .map(|a| (a.logical_w, a.logical_h))
                            .unwrap_or((64, 64));
                        let (pivot_x, pivot_y) =
                            first_frame.resolved_pivot(logical_w, logical_h);
                        self.play_animation_frame(
                            id,
                            &first_frame.path,
                            pivot_x,
                            pivot_y,
                            logical_w,
                            logical_h,
                            first_frame
                                .src_x
                                .zip(first_frame.src_y)
                                .zip(first_frame.src_w.zip(first_frame.src_h))
                                .map(|((x, y), (w, h))| (x, y, w, h)),
                            false,
                        );
                    }
                }
            }
        }
        if let Some(script_list) = scripts {
            for script in script_list {
                self.handle_command(EngineCommand::Common(EngineCommandCommon::LoadScript {
                    id,
                    path: script.path.clone(),
                    source: script.source.clone(),
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
