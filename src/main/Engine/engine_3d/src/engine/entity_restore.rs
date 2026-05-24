use crate::ipc::{
    ControlBindingsData, EntityRestorePhysics, EntityRestoreTransform, EngineCommand,
};

use super::State;

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
            self.handle_command(EngineCommand::SetEntityName {
                id,
                name,
                force: true,
            });
        }
        if !skip_transform {
            self.handle_command(EngineCommand::SetTransform {
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
            });
        }
        if let Some(physics) = physics {
            if physics.enabled {
                self.handle_command(EngineCommand::SetPhysics {
                    id,
                    enabled: true,
                    body_type: physics.body_type.clone(),
                });
            }
        }
        if let Some(bindings) = control_bindings {
            self.handle_command(EngineCommand::SetControlBindings {
                id,
                bindings: bindings.clone(),
            });
        }
    }
}
