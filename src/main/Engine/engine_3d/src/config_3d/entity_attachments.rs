//! Vínculo padre–hijo entre entidades (accesorios, armas, etc.).
//! Offset de posición y rotación en espacio local del padre; la escala del hijo no se hereda.

use std::collections::HashSet;

use glam::{Quat, Vec3};

use crate::ecs::{EntityId, Transform};
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};

#[derive(Clone, Debug)]
pub(crate) struct EntityAttachmentLocal {
    pub parent_id: EntityId,
    pub local_position: Vec3,
    pub local_rotation: Quat,
    /// Escala mundial del hijo (independiente del padre; evita deformar accesorios).
    pub child_world_scale: Vec3,
}

pub(crate) fn compute_local_attachment(
    parent: &Transform,
    child: &Transform,
) -> (Vec3, Quat, Vec3) {
    let parent_inv = parent.rotation.inverse();
    let local_position = parent_inv * (child.position - parent.position);
    let local_rotation = (parent_inv * child.rotation).normalize();
    (local_position, local_rotation, child.scale)
}

pub(crate) fn world_transform_from_attachment(
    parent: &Transform,
    local_position: Vec3,
    local_rotation: Quat,
    child_world_scale: Vec3,
) -> Transform {
    Transform {
        position: parent.position + parent.rotation * local_position,
        rotation: (parent.rotation * local_rotation).normalize(),
        scale: child_world_scale,
    }
}

impl State {
    pub(crate) fn is_entity_merge_forbidden(&self, id: EntityId) -> bool {
        self.ground_entity_id() == Some(id)
            || self.sun_entity == Some(id)
            || self.editor_camera_entity == Some(id)
            || self.background_entity == Some(id)
            || self.collider_entities.contains(&id)
            || self.execution_area_entities.contains(&id)
    }

    pub(crate) fn merge_entities(&mut self, ids: &[EntityId]) {
        if ids.len() < 2 {
            send_event(&EngineEvent::Error {
                message: "Selecciona al menos dos entidades para fusionar.".to_string(),
            });
            return;
        }

        let parent_id = self
            .play_character_entity
            .filter(|player_id| ids.contains(player_id))
            .or_else(|| self.selected_entity.filter(|id| ids.contains(id)))
            .or_else(|| ids.first().copied());

        let Some(parent_id) = parent_id else {
            return;
        };

        if self.is_entity_merge_forbidden(parent_id) {
            log::warn!(
                "[Fusión] entidad {} no puede ser padre de una fusión",
                parent_id
            );
            return;
        }

        let mut merged = 0usize;
        for &child_id in ids {
            if child_id == parent_id {
                continue;
            }
            if self.is_entity_merge_forbidden(child_id) {
                continue;
            }
            if self.world.get::<Transform>(child_id).is_none() {
                continue;
            }
            if self.entity_attachment_chain_includes(parent_id, child_id) {
                log::warn!(
                    "[Fusión] omitiendo {}: crearía un ciclo con el padre {}",
                    child_id,
                    parent_id
                );
                continue;
            }

            self.detach_children_of(child_id);
            self.detach_entity_attachment(child_id);

            let Some(parent_t) = self.world.get::<Transform>(parent_id).cloned() else {
                continue;
            };
            let Some(child_t) = self.world.get::<Transform>(child_id).cloned() else {
                continue;
            };
            let (local_position, local_rotation, child_world_scale) =
                compute_local_attachment(&parent_t, &child_t);
            self.entity_attachments.insert(
                child_id,
                EntityAttachmentLocal {
                    parent_id,
                    local_position,
                    local_rotation,
                    child_world_scale,
                },
            );
            merged += 1;
        }

        if merged > 0 {
            let child_ids: Vec<EntityId> = self
                .entity_attachments
                .iter()
                .filter(|(_, attachment)| attachment.parent_id == parent_id)
                .map(|(id, _)| *id)
                .filter(|id| ids.contains(id))
                .collect();
            log::info!(
                "[Fusión] {} entidad(es) vinculadas al padre {}",
                merged,
                parent_id
            );
            send_event(&EngineEvent::EntitiesMerged {
                parent_id,
                child_ids,
            });
        } else {
            send_event(&EngineEvent::Error {
                message: "No se pudo fusionar ninguna entidad (revisa la selección)."
                    .to_string(),
            });
        }
    }

    fn entity_attachment_chain_includes(&self, start: EntityId, target: EntityId) -> bool {
        let mut current = Some(start);
        let mut visited = HashSet::new();
        while let Some(id) = current {
            if id == target {
                return true;
            }
            if !visited.insert(id) {
                break;
            }
            current = self
                .entity_attachments
                .get(&id)
                .map(|attachment| attachment.parent_id);
        }
        false
    }

    pub(crate) fn detach_entity_attachment(&mut self, child_id: EntityId) {
        self.entity_attachments.remove(&child_id);
    }

    pub(crate) fn detach_children_of(&mut self, parent_id: EntityId) {
        self.entity_attachments
            .retain(|_, attachment| attachment.parent_id != parent_id);
    }

    pub(crate) fn clear_entity_attachments_for_removed(&mut self, id: EntityId) {
        self.entity_attachments.remove(&id);
        self.entity_attachments
            .retain(|_, attachment| attachment.parent_id != id);
    }

    pub(crate) fn recapture_entity_attachment(&mut self, child_id: EntityId) {
        let Some(attachment) = self.entity_attachments.get(&child_id).cloned() else {
            return;
        };
        let Some(parent_t) = self.world.get::<Transform>(attachment.parent_id).cloned() else {
            return;
        };
        let Some(child_t) = self.world.get::<Transform>(child_id).cloned() else {
            return;
        };
        let (local_position, local_rotation, child_world_scale) =
            compute_local_attachment(&parent_t, &child_t);
        if let Some(attachment) = self.entity_attachments.get_mut(&child_id) {
            attachment.local_position = local_position;
            attachment.local_rotation = local_rotation;
            attachment.child_world_scale = child_world_scale;
        }
    }

    pub(crate) fn sync_attached_children_of(&mut self, parent_id: EntityId) {
        let children: Vec<EntityId> = self
            .entity_attachments
            .iter()
            .filter(|(_, attachment)| attachment.parent_id == parent_id)
            .map(|(id, _)| *id)
            .collect();
        if children.is_empty() {
            return;
        }

        let Some(parent_t) = self.world.get::<Transform>(parent_id).cloned() else {
            return;
        };

        for child_id in children {
            let Some(attachment) = self.entity_attachments.get(&child_id).cloned() else {
                continue;
            };
            let world_t = world_transform_from_attachment(
                &parent_t,
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

    /// Tras mover entidades (gizmo o panel): sincroniza hijos o recaptura offset local.
    pub(crate) fn handle_entity_attachment_after_transform(
        &mut self,
        changed_ids: &[EntityId],
    ) {
        if changed_ids.is_empty() {
            return;
        }
        let changed: HashSet<EntityId> = changed_ids.iter().copied().collect();

        for &id in changed_ids {
            if self.entity_attachments.contains_key(&id) {
                let parent_id = self.entity_attachments[&id].parent_id;
                if !changed.contains(&parent_id) {
                    self.recapture_entity_attachment(id);
                }
            }
        }

        for &id in changed_ids {
            let has_children = self
                .entity_attachments
                .values()
                .any(|attachment| attachment.parent_id == id);
            if !has_children {
                continue;
            }
            let any_child_selected = self.entity_attachments.iter().any(|(child_id, attachment)| {
                attachment.parent_id == id && changed.contains(child_id)
            });
            if !any_child_selected {
                self.sync_attached_children_of(id);
            }
        }
    }

    pub(crate) fn restore_entity_attachments_from_saved(
        &mut self,
        saved: &[SavedEntityAttachment],
    ) {
        self.entity_attachments.clear();
        for entry in saved {
            if self.world.get::<Transform>(entry.entity_id).is_none()
                || self.world.get::<Transform>(entry.parent_id).is_none()
            {
                log::warn!(
                    "[Fusión] omitiendo vínculo guardado {} → {} (entidad ausente)",
                    entry.entity_id,
                    entry.parent_id
                );
                continue;
            }
            self.entity_attachments.insert(
                entry.entity_id,
                EntityAttachmentLocal {
                    parent_id: entry.parent_id,
                    local_position: Vec3::from_array(entry.local_position),
                    local_rotation: Quat::from_xyzw(
                        entry.local_rotation[0],
                        entry.local_rotation[1],
                        entry.local_rotation[2],
                        entry.local_rotation[3],
                    ),
                    child_world_scale: Vec3::from_array(entry.child_world_scale),
                },
            );
        }

        if self.entity_attachments.is_empty() {
            return;
        }

        let count = self.entity_attachments.len();
        let parent_ids: HashSet<EntityId> = self
            .entity_attachments
            .values()
            .map(|a| a.parent_id)
            .collect();
        for parent_id in parent_ids {
            self.sync_attached_children_of(parent_id);
        }
        log::info!("[Fusión] {count} vínculo(s) restaurados desde manifest");
        send_event(&EngineEvent::EntitiesAttachmentsRestored { count });
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SavedEntityAttachment {
    pub entity_id: EntityId,
    pub parent_id: EntityId,
    pub local_position: [f32; 3],
    pub local_rotation: [f32; 4],
    pub child_world_scale: [f32; 3],
}
