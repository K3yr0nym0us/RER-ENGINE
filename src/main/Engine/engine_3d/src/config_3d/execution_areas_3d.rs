use std::collections::HashSet;

use glam::Vec3;

use crate::config_3d::plane_tools::PLANE_TOOL_PHYSICS_DEPTH;
use crate::ecs::{EntityId, Transform};
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};

impl State {
    /// Detecta entradas a triggers 3D (planos execution area) en modo preview.
    pub(crate) fn update_execution_areas_3d(&mut self) {
        if !self.preview_playing {
            self.execution_overlaps.clear();
            return;
        }

        let trigger_ids = self.execution_area_entities.clone();
        let actor_ids = self.execution_area_trigger_actors();
        let mut next_overlaps = HashSet::new();

        for trigger_id in trigger_ids {
            let Some(trigger_t) = self.world.get::<Transform>(trigger_id).cloned() else {
                continue;
            };

            for &actor_id in &actor_ids {
                let Some((point, radius)) = self.execution_area_actor_probe(actor_id) else {
                    continue;
                };
                if !Self::point_in_plane_trigger(&trigger_t, point, radius) {
                    continue;
                }

                next_overlaps.insert((trigger_id, actor_id));
                if self.execution_overlaps.contains(&(trigger_id, actor_id)) {
                    continue;
                }

                let has_attached_script = self.script_engine.entity_has_scripts(trigger_id);
                send_event(&EngineEvent::TriggerEntered {
                    trigger_id,
                    actor_id,
                    has_attached_script: Some(has_attached_script),
                });

                let trigger_snapshot = self.build_script_snapshot(trigger_id);
                let actor_snapshot = self.build_script_snapshot(actor_id);
                match self.script_engine.run_trigger_enter_hook(
                    trigger_id,
                    actor_id,
                    trigger_snapshot.as_ref(),
                    actor_snapshot.as_ref(),
                ) {
                    Ok(commands) => self.apply_script_commands(commands),
                    Err(e) => {
                        log::warn!("[trigger] error ejecutando script en área {trigger_id}: {e}");
                    }
                }
            }
        }

        let exited: Vec<_> = self
            .execution_overlaps
            .iter()
            .filter(|pair| !next_overlaps.contains(*pair))
            .cloned()
            .collect();
        for (trigger_id, actor_id) in exited {
            send_event(&EngineEvent::TriggerExited {
                trigger_id,
                actor_id,
            });
        }

        self.execution_overlaps = next_overlaps;
    }

    fn execution_area_trigger_actors(&self) -> Vec<EntityId> {
        let mut ids = self.character_entities.clone();
        if let Some(pc) = self.play_character_entity {
            if !ids.contains(&pc) {
                ids.push(pc);
            }
        }
        ids
    }

    fn execution_area_actor_probe(&self, actor_id: EntityId) -> Option<(Vec3, f32)> {
        if self.play_character_entity == Some(actor_id) {
            let feet = self.play_character_feet_position();
            let radius = self
                .world
                .get::<Transform>(actor_id)
                .map(|t| self.play_character_capsule_radius_world(t.scale))
                .unwrap_or(0.35);
            return Some((feet, radius));
        }
        let t = self.world.get::<Transform>(actor_id)?;
        let radius = t
            .scale
            .x
            .abs()
            .max(t.scale.z.abs())
            .max(t.scale.y.abs())
            * 0.35;
        Some((t.position, radius))
    }

    fn point_in_plane_trigger(trigger_t: &Transform, point: Vec3, actor_radius: f32) -> bool {
        let local = trigger_t.rotation.conjugate() * (point - trigger_t.position);
        let hx = trigger_t.scale.x.abs() * 0.5 + actor_radius;
        let hy = trigger_t.scale.y.abs() * 0.5 + actor_radius;
        let hz = PLANE_TOOL_PHYSICS_DEPTH * 0.5 + actor_radius;
        local.x.abs() <= hx && local.y.abs() <= hy && local.z.abs() <= hz
    }
}
