use super::*;

impl State {
    pub(super) fn cmd_sockets(&mut self, cmd: EngineCommand) -> Option<EngineCommand> {
        match cmd {
            EngineCommand::Only3d(EngineCommand3dOnly::ListEntityBones { entity_id }) => {
                let bones = self.list_entity_bone_names(entity_id);
                crate::engine3d::ipc::send_event_silent(
                    &crate::engine3d::ipc::EngineEvent::EntityBonesList { entity_id, bones },
                );
            }
            EngineCommand::Only3d(EngineCommand3dOnly::ListEntitySockets { entity_id }) => {
                let sockets = self.list_entity_sockets(entity_id);
                send_event(&crate::engine3d::ipc::EngineEvent::EntitySocketsList {
                    entity_id,
                    sockets,
                });
            }
            EngineCommand::Only3d(EngineCommand3dOnly::UpsertEntitySocket {
                entity_id,
                name,
                bone_name,
                local_position,
                local_rotation,
            }) => {
                use glam::{Quat, Vec3};
                let socket = crate::engine3d::config_3d::entity_sockets::EntitySocket {
                    name,
                    bone_name,
                    local_position: Vec3::from_array(local_position),
                    local_rotation: Quat::from_xyzw(
                        local_rotation[0],
                        local_rotation[1],
                        local_rotation[2],
                        local_rotation[3],
                    ),
                };
                match self.upsert_entity_socket(entity_id, socket) {
                    Ok(()) => {}
                    Err(message) => {
                        send_event(&crate::engine3d::ipc::EngineEvent::Error { message });
                    }
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::RemoveEntitySocket { entity_id, name }) => {
                self.remove_entity_socket(entity_id, &name);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::AttachToSocket {
                child_ids,
                host_id,
                socket_name,
            }) => {
                self.attach_to_socket(&child_ids, host_id, &socket_name);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::DetachFromSocket { child_id }) => {
                self.detach_from_socket(child_id);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetSocketBonePickMode {
                entity_id,
                active,
            }) => {
                self.set_socket_bone_pick_mode(entity_id, active);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetBonePhysicsEditorEntity {
                entity_id,
                active,
            }) => {
                self.set_bone_physics_editor_entity(entity_id, active);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetBonePhysicsPickMode {
                entity_id,
                active,
            }) => {
                self.set_bone_physics_pick_mode(entity_id, active);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetBonePhysics {
                entity_id,
                bone_name,
                mode,
            }) => {
                use crate::engine3d::config_3d::bone_physics::parse_bone_physics_mode;
                match parse_bone_physics_mode(&mode) {
                    Some(parsed) => {
                        if let Err(message) = self.set_bone_physics(entity_id, &bone_name, parsed) {
                            send_event(&crate::engine3d::ipc::EngineEvent::Error { message });
                        }
                    }
                    None => {
                        send_event(&crate::engine3d::ipc::EngineEvent::Error {
                            message: format!("Modo de física por hueso inválido: {mode}"),
                        });
                    }
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::RemoveBonePhysics {
                entity_id,
                bone_name,
            }) => {
                self.remove_bone_physics(entity_id, &bone_name);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::ListEntityBonePhysics { entity_id }) => {
                let entries = self.list_entity_bone_physics(entity_id);
                crate::engine3d::ipc::send_event_silent(
                    &crate::engine3d::ipc::EngineEvent::EntityBonePhysicsList {
                        entity_id,
                        entries,
                    },
                );
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetProjectileConfig {
                id,
                speed,
                lifetime_s,
                affected_by_gravity,
                gravity_scale,
                align_to_velocity,
                muzzle_socket,
                bounceable,
                max_bounces,
                bounce_speed_loss,
                hardness,
            }) => {
                let prev = self.projectile_config_for(id);
                self.set_projectile_config(
                    id,
                    crate::engine3d::config_3d::projectiles::ProjectileConfig {
                        speed,
                        lifetime_s,
                        affected_by_gravity: affected_by_gravity
                            .unwrap_or(prev.affected_by_gravity),
                        gravity_scale: gravity_scale.unwrap_or(prev.gravity_scale),
                        align_to_velocity: align_to_velocity.unwrap_or(prev.align_to_velocity),
                        muzzle_socket: muzzle_socket.or(prev.muzzle_socket),
                        bounceable: bounceable.unwrap_or(prev.bounceable),
                        max_bounces: max_bounces.unwrap_or(prev.max_bounces),
                        bounce_speed_loss: bounce_speed_loss.unwrap_or(prev.bounce_speed_loss),
                        hardness: hardness.unwrap_or(prev.hardness),
                    },
                );
            }
            EngineCommand::Only3d(EngineCommand3dOnly::FireProjectile {
                template_id,
                from_id,
                dir_x,
                dir_y,
                dir_z,
            }) => {
                let _ = self.fire_projectile_from_template(
                    template_id,
                    from_id,
                    glam::Vec3::new(dir_x, dir_y, dir_z),
                );
            }
            other => return Some(other),
        }
        None
    }
}
