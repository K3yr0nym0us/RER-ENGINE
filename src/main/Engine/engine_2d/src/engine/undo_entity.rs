use glam::{Quat, Vec3 as GlamVec3};

use crate::config_2d::{CharacterMarker, ColliderMarker, ExecutionAreaMarker, ScenarioMarker};
use crate::ecs::{MeshComponent, Transform};
use crate::entity_save_meta::EntitySaveMeta;
use crate::ipc::{EngineEvent, send_event};

use super::State;
use super::types::{EntityUndoSnapshot, UndoAction, UndoEntityKind};

impl State {
    pub(crate) fn capture_entity_undo_snapshot(&self, id: u32) -> Option<EntityUndoSnapshot> {
        let t = self.world.get::<Transform>(id)?;
        let mesh = self.world.get::<MeshComponent>(id)?.clone();
        let name = self
            .world
            .get::<crate::ecs::NameComponent>(id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| format!("Entity {id}"));

        let points = self
            .save_registry
            .meta
            .get(&id)
            .and_then(|m| m.points)
            .or_else(|| self.collider_points_from_transform(id));

        let kind = if let Some(m) = self.world.get::<CharacterMarker>(id) {
            UndoEntityKind::Character(m.clone())
        } else if let Some(m) = self.world.get::<ScenarioMarker>(id) {
            UndoEntityKind::Scenario(m.clone())
        } else if self.world.get::<ColliderMarker>(id).is_some() {
            let points = points?;
            UndoEntityKind::Collider { points }
        } else if self.world.get::<ExecutionAreaMarker>(id).is_some() {
            let points = points?;
            UndoEntityKind::ExecutionArea { points }
        } else {
            return None;
        };

        Some(EntityUndoSnapshot {
            id,
            name,
            transform_position: t.position.to_array(),
            transform_rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
            transform_scale: t.scale.to_array(),
            mesh,
            kind,
        })
    }

    pub(crate) fn push_remove_entity_undo(&mut self, id: u32) {
        let Some(snapshot) = self.capture_entity_undo_snapshot(id) else {
            log::warn!("[undo] entidad {id} sin snapshot — redo no disponible");
            return;
        };
        self.undo_stack.push(UndoAction::RemoveEntity { snapshot });
        self.redo_stack.clear();
    }

    pub(crate) fn restore_entity_from_undo_snapshot(
        &mut self,
        snapshot: &EntityUndoSnapshot,
    ) -> bool {
        if !self.world.spawn_with_id(snapshot.id, Some(&snapshot.name)) {
            log::warn!("[redo] no se pudo reinsertar entidad {}", snapshot.id);
            return false;
        }

        self.world.insert(
            snapshot.id,
            Transform {
                position: GlamVec3::from_array(snapshot.transform_position),
                rotation: Quat::from_xyzw(
                    snapshot.transform_rotation[0],
                    snapshot.transform_rotation[1],
                    snapshot.transform_rotation[2],
                    snapshot.transform_rotation[3],
                ),
                scale: GlamVec3::from_array(snapshot.transform_scale),
            },
        );
        self.world.insert(snapshot.id, snapshot.mesh.clone());

        match &snapshot.kind {
            UndoEntityKind::Character(marker) => {
                self.world.insert(snapshot.id, marker.clone());
                if !self.character_entities.contains(&snapshot.id) {
                    self.character_entities.push(snapshot.id);
                }
                self.scenario_entities.retain(|&e| e != snapshot.id);
                self.save_registry.register_meta(
                    snapshot.id,
                    EntitySaveMeta {
                        kind: "character".to_string(),
                        path: marker.path.clone(),
                        visual_model_path: None,
                        points: None,
                    },
                );
                send_event(&EngineEvent::CharacterLoaded {
                    id: snapshot.id,
                    path: marker.path.clone(),
                    img_width: marker.img_width,
                    img_height: marker.img_height,
                    default_pivot_x: marker.img_width as f32 * 0.5,
                    default_pivot_y: marker.img_height as f32,
                });
            }
            UndoEntityKind::Scenario(marker) => {
                self.world.insert(snapshot.id, marker.clone());
                if !self.scenario_entities.contains(&snapshot.id) {
                    self.scenario_entities.push(snapshot.id);
                }
                self.character_entities.retain(|&e| e != snapshot.id);
                self.save_registry.register_meta(
                    snapshot.id,
                    EntitySaveMeta {
                        kind: "scenario".to_string(),
                        path: marker.path.clone(),
                        visual_model_path: None,
                        points: None,
                    },
                );
                send_event(&EngineEvent::ScenarioLoaded {
                    id: snapshot.id,
                    path: marker.path.clone(),
                    name: Some(snapshot.name.clone()),
                    img_width: marker.img_width,
                    img_height: marker.img_height,
                    default_pivot_x: marker.img_width as f32 * 0.5,
                    default_pivot_y: marker.img_height as f32,
                });
            }
            UndoEntityKind::Collider { points } => {
                self.world.insert(snapshot.id, ColliderMarker {});
                if !self.collider_entities.contains(&snapshot.id) {
                    self.collider_entities.push(snapshot.id);
                }
                let pos = snapshot.transform_position;
                let scale = snapshot.transform_scale;
                self.physics_2d.set_entity_physics(
                    snapshot.id,
                    true,
                    "static",
                    [pos[0], pos[1], 0.0],
                    [scale[0] * 0.5, scale[1] * 0.5, 0.01],
                    [0.0, 0.0, 0.0],
                );
                self.save_registry.register_meta(
                    snapshot.id,
                    EntitySaveMeta {
                        kind: "collider".to_string(),
                        path: "[Colisionador]".to_string(),
                        visual_model_path: None,
                        points: Some(*points),
                    },
                );
                send_event(&EngineEvent::ColliderCreated {
                    id: snapshot.id,
                    points: *points,
                });
            }
            UndoEntityKind::ExecutionArea { points } => {
                self.world.insert(snapshot.id, ExecutionAreaMarker {});
                if !self.execution_area_entities.contains(&snapshot.id) {
                    self.execution_area_entities.push(snapshot.id);
                }
                self.save_registry.register_meta(
                    snapshot.id,
                    EntitySaveMeta {
                        kind: "execution_area".to_string(),
                        path: "[ExecutionArea]".to_string(),
                        visual_model_path: None,
                        points: Some(*points),
                    },
                );
                send_event(&EngineEvent::ExecutionAreaCreated {
                    id: snapshot.id,
                    points: *points,
                });
            }
        }

        log::info!("[redo] entidad {} restaurada", snapshot.id);
        true
    }
}
