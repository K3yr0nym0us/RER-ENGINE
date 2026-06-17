//! Undo/redo de sockets y vínculos a socket (bloqueo de cambio de escena).

use glam::{Quat, Vec3};

use crate::config_3d::entity_attachments::{AttachmentAnchor, EntityAttachmentLocal};
use crate::config_3d::entity_sockets::{EntitySocket, EntitySocketSnapshot};
use crate::ecs::{EntityId, Transform};
use crate::ipc::{send_event, EngineEvent};

use super::types::UndoAction;
use super::State;

impl State {
    pub(crate) fn push_undo_entity_socket_created(
        &mut self,
        entity_id: EntityId,
        socket: EntitySocketSnapshot,
    ) {
        if self.is_applying_undo {
            return;
        }
        self.redo_stack.clear();
        self.undo_stack.push(UndoAction::RemoveEntitySocket {
            entity_id,
            socket,
        });
        self.sync_editor_scenes_undo_dirty_to_renderer();
    }

    pub(crate) fn push_undo_socket_attachment(
        &mut self,
        child_id: EntityId,
        previous_attachment: Option<EntityAttachmentLocal>,
        previous_transform: &Transform,
        applied_attachment: EntityAttachmentLocal,
    ) {
        if self.is_applying_undo {
            return;
        }
        self.undo_stack.push(UndoAction::RestoreSocketAttachment {
            child_id,
            previous_attachment,
            previous_position: previous_transform.position.to_array(),
            previous_rotation: [
                previous_transform.rotation.x,
                previous_transform.rotation.y,
                previous_transform.rotation.z,
                previous_transform.rotation.w,
            ],
            previous_scale: previous_transform.scale.to_array(),
            applied_attachment,
        });
        self.sync_editor_scenes_undo_dirty_to_renderer();
    }

    pub(crate) fn apply_undo_remove_entity_socket(
        &mut self,
        entity_id: EntityId,
        socket: &EntitySocketSnapshot,
    ) {
        self.remove_entity_socket(entity_id, &socket.name);
    }

    pub(crate) fn apply_redo_restore_entity_socket(
        &mut self,
        entity_id: EntityId,
        socket: &EntitySocketSnapshot,
    ) {
        let entity_socket = EntitySocket {
            name: socket.name.clone(),
            bone_name: socket.bone_name.clone(),
            local_position: Vec3::from_array(socket.local_position),
            local_rotation: Quat::from_xyzw(
                socket.local_rotation[0],
                socket.local_rotation[1],
                socket.local_rotation[2],
                socket.local_rotation[3],
            )
            .normalize(),
        };
        let _ = self.upsert_entity_socket(entity_id, entity_socket);
    }

    pub(crate) fn apply_undo_socket_attachment(
        &mut self,
        child_id: EntityId,
        previous_attachment: Option<EntityAttachmentLocal>,
        previous_position: [f32; 3],
        previous_rotation: [f32; 4],
        previous_scale: [f32; 3],
    ) {
        self.entity_attachments.remove(&child_id);
        if let Some(attachment) = previous_attachment {
            self.entity_attachments.insert(child_id, attachment.clone());
            match &attachment.anchor {
                AttachmentAnchor::Entity(parent_id) => {
                    self.sync_attached_children_of(*parent_id);
                }
                AttachmentAnchor::Socket { .. } => {
                    self.sync_socket_attached_children();
                }
            }
        } else if let Some(t) = self.world.get_mut::<Transform>(child_id) {
            t.position = Vec3::from_array(previous_position);
            t.rotation = Quat::from_xyzw(
                previous_rotation[0],
                previous_rotation[1],
                previous_rotation[2],
                previous_rotation[3],
            )
            .normalize();
            t.scale = Vec3::from_array(previous_scale);
            self.sync_entity_physics_collider(child_id);
        }
        self.emit_child_attachment_state(child_id);
    }

    pub(crate) fn apply_redo_socket_attachment(
        &mut self,
        child_id: EntityId,
        applied_attachment: &EntityAttachmentLocal,
    ) {
        self.entity_attachments
            .insert(child_id, applied_attachment.clone());
        self.sync_socket_attached_children();
        if let AttachmentAnchor::Socket {
            host_entity_id,
            socket_name,
        } = &applied_attachment.anchor
        {
            send_event(&EngineEvent::EntitySocketAttached {
                host_id: *host_entity_id,
                socket_name: socket_name.clone(),
                child_ids: vec![child_id],
            });
        }
    }

    pub(crate) fn emit_child_attachment_state(&self, child_id: EntityId) {
        let attachment = self.entity_attachments.get(&child_id);
        let (attach_parent_id, attach_socket_host_id, attach_socket_name) =
            match attachment.map(|a| &a.anchor) {
                Some(AttachmentAnchor::Entity(parent_id)) => (Some(*parent_id), None, None),
                Some(AttachmentAnchor::Socket {
                    host_entity_id,
                    socket_name,
                }) => (None, Some(*host_entity_id), Some(socket_name.clone())),
                None => (None, None, None),
            };
        send_event(&EngineEvent::EntityAttachmentRestored {
            child_id,
            attach_parent_id,
            attach_socket_host_id,
            attach_socket_name,
        });
    }
}
