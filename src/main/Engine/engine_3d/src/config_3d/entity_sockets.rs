//! Sockets nombrados anclados a huesos de esqueleto (armas, escudos, etc.).

use std::collections::{HashMap, HashSet};

use glam::{Mat4, Quat, Vec3};

use crate::config_3d::entity_attachments::{
    compute_local_attachment, world_transform_from_attachment, AttachmentAnchor,
    EntityAttachmentLocal,
};
use crate::config_3d::model_animation::asset_joint_globals_with_clip;
use crate::config_3d::model_asset::ModelAsset;
use crate::ecs::{EntityId, Transform};
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};

/// Punto de anclaje en un hueso (persistido en el host).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EntitySocket {
    pub name: String,
    pub bone_name: String,
    pub local_position: Vec3,
    pub local_rotation: Quat,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct EntitySocketSnapshot {
    pub name: String,
    pub bone_name: String,
    pub local_position: [f32; 3],
    pub local_rotation: [f32; 4],
}

impl EntitySocket {
    pub(crate) fn to_snapshot(&self) -> EntitySocketSnapshot {
        EntitySocketSnapshot {
            name: self.name.clone(),
            bone_name: self.bone_name.clone(),
            local_position: self.local_position.to_array(),
            local_rotation: [
                self.local_rotation.x,
                self.local_rotation.y,
                self.local_rotation.z,
                self.local_rotation.w,
            ],
        }
    }
}

/// Resuelve índice de joint por nombre exacto (case-insensitive).
pub(crate) fn resolve_joint_index(asset: &ModelAsset, bone_name: &str) -> Option<usize> {
    let needle = bone_name.trim();
    if needle.is_empty() {
        return None;
    }
    asset
        .joint_names
        .iter()
        .position(|n| n.eq_ignore_ascii_case(needle))
}

pub(crate) fn transform_from_mat4(m: Mat4) -> Transform {
    let (scale, rotation, translation) = m.to_scale_rotation_translation();
    Transform {
        position: translation,
        rotation,
        scale,
    }
}

/// Transform mundial del hueso (posición + rotación) para una entidad skinned.
pub(crate) fn bone_world_transform(
    state: &State,
    host_id: EntityId,
    bone_name: &str,
) -> Option<Transform> {
    let binding = state.model_animation_bindings.get(&host_id)?;
    let asset = state.get_model_asset_for_entity(&binding.asset_path, host_id)?;
    let ji = resolve_joint_index(&asset, bone_name)?;

    let (clip, time_s) = state
        .active_model_clips
        .get(&host_id)
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
    let global = *globals.get(ji)?;

    let entity_t = state.world.get::<Transform>(host_id)?;
    let entity_model = entity_t.to_matrix();
    let bone_mat = entity_model * asset.mesh_normalize * global;
    Some(transform_from_mat4(bone_mat))
}

pub(crate) fn socket_world_transform(
    bone: &Transform,
    socket: &EntitySocket,
) -> Transform {
    Transform {
        position: bone.position + bone.rotation * socket.local_position,
        rotation: (bone.rotation * socket.local_rotation).normalize(),
        scale: Vec3::ONE,
    }
}

impl State {
    pub(crate) fn entity_has_skinned_model(&self, id: EntityId) -> bool {
        self.model_animation_bindings.contains_key(&id)
    }

    pub(crate) fn list_entity_bone_names(&self, entity_id: EntityId) -> Vec<String> {
        let Some(binding) = self.model_animation_bindings.get(&entity_id) else {
            return Vec::new();
        };
        let Some(asset) = self.get_model_asset_for_entity(&binding.asset_path, entity_id) else {
            return Vec::new();
        };
        asset.joint_names.clone()
    }

    pub(crate) fn list_entity_sockets(&self, entity_id: EntityId) -> Vec<EntitySocketSnapshot> {
        self.entity_sockets
            .get(&entity_id)
            .map(|sockets| sockets.iter().map(EntitySocket::to_snapshot).collect())
            .unwrap_or_default()
    }

    pub(crate) fn upsert_entity_socket(
        &mut self,
        entity_id: EntityId,
        socket: EntitySocket,
    ) -> Result<(), String> {
        if !self.entity_has_skinned_model(entity_id) {
            return Err("La entidad no tiene modelo con esqueleto.".to_string());
        }
        let name = socket.name.trim().to_string();
        if name.is_empty() {
            return Err("El nombre del socket no puede estar vacío.".to_string());
        }
        let bone_name = socket.bone_name.trim().to_string();
        if bone_name.is_empty() {
            return Err("Debes seleccionar un hueso.".to_string());
        }
        let binding = self
            .model_animation_bindings
            .get(&entity_id)
            .ok_or_else(|| "Modelo sin esqueleto.".to_string())?;
        let asset = self
            .get_model_asset_for_entity(&binding.asset_path, entity_id)
            .ok_or_else(|| "Asset de modelo no disponible.".to_string())?;
        if resolve_joint_index(&asset, &bone_name).is_none() {
            return Err(format!("Hueso no encontrado: {bone_name}"));
        }

        let sockets = self.entity_sockets.entry(entity_id).or_default();
        let created_new = if let Some(existing) =
            sockets.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&name))
        {
            *existing = EntitySocket {
                name: name.clone(),
                bone_name: bone_name.clone(),
                local_position: socket.local_position,
                local_rotation: socket.local_rotation.normalize(),
            };
            false
        } else if sockets.iter().any(|s| s.name.eq_ignore_ascii_case(&name)) {
            return Err(format!("Ya existe un socket llamado {name}"));
        } else {
            sockets.push(EntitySocket {
                name: name.clone(),
                bone_name: bone_name.clone(),
                local_position: socket.local_position,
                local_rotation: socket.local_rotation.normalize(),
            });
            true
        };

        self.bone_index_cache
            .retain(|(host, _), _| *host != entity_id);

        let snapshots = self.list_entity_sockets(entity_id);
        if created_new {
            log::info!(
                "[Socket] Se creó un socket con nombre {name} asignado al hueso {bone_name}."
            );
            if let Some(snapshot) = snapshots
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(&name))
            {
                self.push_undo_entity_socket_created(entity_id, snapshot.clone());
            }
        }
        send_event(&EngineEvent::EntitySocketsChanged {
            entity_id,
            sockets: snapshots,
        });
        Ok(())
    }

    pub(crate) fn remove_entity_socket(&mut self, entity_id: EntityId, name: &str) {
        let Some(sockets) = self.entity_sockets.get_mut(&entity_id) else {
            return;
        };
        let trimmed = name.trim();
        sockets.retain(|s| !s.name.eq_ignore_ascii_case(trimmed));
        if sockets.is_empty() {
            self.entity_sockets.remove(&entity_id);
        }

        self.entity_attachments.retain(|_, attachment| {
            !matches!(
                &attachment.anchor,
                AttachmentAnchor::Socket { host_entity_id, socket_name }
                    if *host_entity_id == entity_id
                        && socket_name.eq_ignore_ascii_case(trimmed)
            )
        });

        self.bone_index_cache
            .retain(|(host, _), _| *host != entity_id);

        let snapshots = self.list_entity_sockets(entity_id);
        send_event(&EngineEvent::EntitySocketsChanged {
            entity_id,
            sockets: snapshots,
        });
    }

    pub(crate) fn attach_to_socket(
        &mut self,
        child_ids: &[EntityId],
        host_id: EntityId,
        socket_name: &str,
    ) {
        let socket_name = socket_name.trim();
        if socket_name.is_empty() {
            send_event(&EngineEvent::Error {
                message: "Selecciona un socket válido.".to_string(),
            });
            return;
        }

        if self.is_entity_merge_forbidden(host_id) {
            send_event(&EngineEvent::Error {
                message: "Esta entidad no puede tener sockets como host.".to_string(),
            });
            return;
        }

        let socket = self
            .entity_sockets
            .get(&host_id)
            .and_then(|sockets| {
                sockets
                    .iter()
                    .find(|s| s.name.eq_ignore_ascii_case(socket_name))
                    .cloned()
            });
        let Some(socket) = socket else {
            send_event(&EngineEvent::Error {
                message: format!("Socket no encontrado: {socket_name}"),
            });
            return;
        };

        let Some(socket_world) = self.socket_world_for_entity(host_id, &socket) else {
            send_event(&EngineEvent::Error {
                message: "No se pudo calcular la posición del socket.".to_string(),
            });
            return;
        };

        let mut attached = 0usize;
        let mut attached_ids = Vec::new();
        let track_undo = !self.is_applying_undo;
        if track_undo {
            self.redo_stack.clear();
        }

        for &child_id in child_ids {
            if child_id == host_id {
                continue;
            }
            if self.is_entity_merge_forbidden(child_id) {
                continue;
            }
            if self.world.get::<Transform>(child_id).is_none() {
                continue;
            }

            self.detach_children_of(child_id);
            let previous_attachment = self.entity_attachments.get(&child_id).cloned();
            self.detach_entity_attachment(child_id);

            let Some(child_t) = self.world.get::<Transform>(child_id).cloned() else {
                continue;
            };
            let (local_position, local_rotation, child_world_scale) =
                compute_local_attachment(&socket_world, &child_t);

            let applied_attachment = EntityAttachmentLocal {
                anchor: AttachmentAnchor::Socket {
                    host_entity_id: host_id,
                    socket_name: socket.name.clone(),
                },
                local_position,
                local_rotation,
                child_world_scale,
            };
            self.entity_attachments
                .insert(child_id, applied_attachment.clone());
            if track_undo {
                self.push_undo_socket_attachment(
                    child_id,
                    previous_attachment,
                    &child_t,
                    applied_attachment,
                );
            }
            attached += 1;
            attached_ids.push(child_id);
        }

        if attached > 0 {
            self.sync_socket_attached_children();
            for &child_id in &attached_ids {
                let label = self
                    .entity_display_name(child_id)
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| format!("#{child_id}"));
                log::info!(
                    "[Socket] La entidad {label} fue vinculada al socket {}.",
                    socket.name
                );
            }
            send_event(&EngineEvent::EntitySocketAttached {
                host_id,
                socket_name: socket.name.clone(),
                child_ids: attached_ids,
            });
        } else {
            send_event(&EngineEvent::Error {
                message: "No se pudo vincular ninguna entidad al socket.".to_string(),
            });
        }
    }

    pub(crate) fn detach_from_socket(&mut self, child_id: EntityId) {
        let was_socket = self
            .entity_attachments
            .get(&child_id)
            .map(|a| matches!(a.anchor, AttachmentAnchor::Socket { .. }))
            .unwrap_or(false);
        if !was_socket {
            return;
        }
        self.detach_entity_attachment(child_id);
        self.emit_child_attachment_state(child_id);
        let label = self
            .entity_display_name(child_id)
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| format!("#{child_id}"));
        log::info!("[Socket] La entidad {label} fue desvinculada del socket.");
    }

    fn socket_world_for_entity(
        &self,
        host_id: EntityId,
        socket: &EntitySocket,
    ) -> Option<Transform> {
        let bone = bone_world_transform(self, host_id, &socket.bone_name)?;
        Some(socket_world_transform(&bone, socket))
    }

    fn find_socket_on_host(&self, host_id: EntityId, socket_name: &str) -> Option<EntitySocket> {
        self.entity_sockets.get(&host_id).and_then(|sockets| {
            sockets
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(socket_name))
                .cloned()
        })
    }

    /// Sincroniza hijos enganchados a sockets; agrupa por host para reutilizar globals.
    pub(crate) fn sync_socket_attached_children(&mut self) {
        let socket_children: Vec<(EntityId, EntityId, String)> = self
            .entity_attachments
            .iter()
            .filter_map(|(child_id, attachment)| match &attachment.anchor {
                AttachmentAnchor::Socket {
                    host_entity_id,
                    socket_name,
                } => Some((*child_id, *host_entity_id, socket_name.clone())),
                _ => None,
            })
            .collect();

        if socket_children.is_empty() {
            return;
        }

        let mut hosts: HashSet<EntityId> = HashSet::new();
        for (_, host_id, _) in &socket_children {
            hosts.insert(*host_id);
        }

        let mut host_socket_worlds: HashMap<(EntityId, String), Transform> = HashMap::new();
        for host_id in hosts {
            let socket_names: HashSet<String> = socket_children
                .iter()
                .filter(|(_, h, _)| *h == host_id)
                .map(|(_, _, name)| name.clone())
                .collect();
            for socket_name in socket_names {
                let Some(socket) = self.find_socket_on_host(host_id, &socket_name) else {
                    continue;
                };
                if let Some(world) = self.socket_world_for_entity(host_id, &socket) {
                    host_socket_worlds.insert((host_id, socket_name), world);
                }
            }
        }

        for (child_id, host_id, socket_name) in socket_children {
            let Some(anchor_world) = host_socket_worlds.get(&(host_id, socket_name)) else {
                continue;
            };
            let Some(attachment) = self.entity_attachments.get(&child_id).cloned() else {
                continue;
            };
            let world_t = world_transform_from_attachment(
                anchor_world,
                attachment.local_position,
                attachment.local_rotation,
                attachment.child_world_scale,
            );
            if let Some(t) = self.world.get_mut::<Transform>(child_id) {
                *t = world_t;
            }
            self.sync_entity_physics_collider(child_id);
        }
    }

    pub(crate) fn recapture_socket_attachment(&mut self, child_id: EntityId) {
        let Some(attachment) = self.entity_attachments.get(&child_id).cloned() else {
            return;
        };
        let AttachmentAnchor::Socket {
            host_entity_id,
            socket_name,
        } = &attachment.anchor
        else {
            return;
        };
        let Some(socket) = self.find_socket_on_host(*host_entity_id, socket_name) else {
            return;
        };
        let Some(socket_world) = self.socket_world_for_entity(*host_entity_id, &socket) else {
            return;
        };
        let Some(child_t) = self.world.get::<Transform>(child_id).cloned() else {
            return;
        };
        let (local_position, local_rotation, child_world_scale) =
            compute_local_attachment(&socket_world, &child_t);
        if let Some(attachment) = self.entity_attachments.get_mut(&child_id) {
            attachment.local_position = local_position;
            attachment.local_rotation = local_rotation;
            attachment.child_world_scale = child_world_scale;
        }
    }

    pub(crate) fn clear_entity_sockets_for_removed(&mut self, id: EntityId) {
        self.entity_sockets.remove(&id);
        self.bone_index_cache.retain(|(host, _), _| *host != id);
        self.entity_attachments.retain(|_, attachment| {
            !matches!(
                &attachment.anchor,
                AttachmentAnchor::Socket { host_entity_id, .. } if *host_entity_id == id
            )
        });
    }

    pub(crate) fn restore_entity_sockets_from_saved(
        &mut self,
        entity_id: EntityId,
        saved: &[EntitySocketSnapshot],
    ) {
        if saved.is_empty() {
            return;
        }
        let mut sockets = Vec::with_capacity(saved.len());
        for entry in saved {
            sockets.push(EntitySocket {
                name: entry.name.clone(),
                bone_name: entry.bone_name.clone(),
                local_position: Vec3::from_array(entry.local_position),
                local_rotation: Quat::from_xyzw(
                    entry.local_rotation[0],
                    entry.local_rotation[1],
                    entry.local_rotation[2],
                    entry.local_rotation[3],
                )
                .normalize(),
            });
        }
        self.entity_sockets.insert(entity_id, sockets);
    }

    pub(crate) fn emit_entity_sockets_if_any(&self, entity_id: EntityId) {
        if self.entity_sockets.contains_key(&entity_id) {
            send_event(&EngineEvent::EntitySocketsChanged {
                entity_id,
                sockets: self.list_entity_sockets(entity_id),
            });
        }
    }
}
