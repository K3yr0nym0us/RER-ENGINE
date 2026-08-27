//! Proyectiles 3D: config de plantilla + instancias cinemáticas en play.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::ecs::{EntityId, Transform};
use crate::engine::State;
use crate::ipc::{EngineEvent, send_event};

pub const DEFAULT_PROJECTILE_SPEED: f32 = 20.0;
pub const DEFAULT_PROJECTILE_LIFETIME_S: f32 = 3.0;

/// Parámetros de disparo persistidos en entidades categoría `projectile`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectileConfig {
    pub speed: f32,
    pub lifetime_s: f32,
}

impl Default for ProjectileConfig {
    fn default() -> Self {
        Self {
            speed: DEFAULT_PROJECTILE_SPEED,
            lifetime_s: DEFAULT_PROJECTILE_LIFETIME_S,
        }
    }
}

impl ProjectileConfig {
    pub fn clamped(self) -> Self {
        Self {
            speed: self.speed.max(0.0),
            lifetime_s: self.lifetime_s.max(0.05),
        }
    }
}

/// Instancia runtime (no se serializa en `.save`).
#[derive(Clone, Debug)]
pub(crate) struct ActiveProjectile {
    pub entity_id: EntityId,
    pub velocity: Vec3,
    pub age: f32,
    pub lifetime_s: f32,
}

impl State {
    pub(crate) fn projectile_config_for(&self, entity_id: EntityId) -> ProjectileConfig {
        self.entity_projectile_config
            .get(&entity_id)
            .copied()
            .unwrap_or_default()
            .clamped()
    }

    pub(crate) fn set_projectile_config(&mut self, entity_id: EntityId, config: ProjectileConfig) {
        let config = config.clamped();
        self.entity_projectile_config.insert(entity_id, config);
        self.emit_projectile_config_changed(entity_id);
    }

    pub(crate) fn restore_projectile_config_from_saved(
        &mut self,
        entity_id: EntityId,
        config: &ProjectileConfig,
    ) {
        self.entity_projectile_config
            .insert(entity_id, config.clamped());
    }

    pub(crate) fn emit_projectile_config_changed(&self, entity_id: EntityId) {
        let config = self.projectile_config_for(entity_id);
        send_event(&EngineEvent::ProjectileConfigChanged {
            entity_id,
            speed: config.speed,
            lifetime_s: config.lifetime_s,
        });
    }

    pub(crate) fn is_runtime_projectile(&self, entity_id: EntityId) -> bool {
        self.active_projectiles
            .iter()
            .any(|p| p.entity_id == entity_id)
    }

    /// Dispara un clon cinemático de la plantilla `template_id`.
    /// Funciona en editor y en play (`from_id == 0` / `None` → origen = plantilla).
    pub(crate) fn fire_projectile_from_template(
        &mut self,
        template_id: EntityId,
        from_id: Option<EntityId>,
        dir: Vec3,
    ) -> Option<EntityId> {
        let dir_len = dir.length();
        if dir_len <= f32::EPSILON {
            log::warn!("[proyectil] fire ignorado: dirección nula");
            return None;
        }
        let dir = dir / dir_len;

        let meta = self.resolve_entity_save_meta(template_id)?;
        if meta.entity_category.as_deref() != Some("projectile") {
            log::warn!("[proyectil] plantilla {template_id} no es categoría projectile");
            return None;
        }
        let path = self.entity_asset_path_for_bounds(template_id).or_else(|| {
            if crate::entity_save_meta::is_model_3d_asset_path(&meta.path) {
                Some(meta.path.clone())
            } else {
                None
            }
        })?;
        let template_t = self.world.get::<Transform>(template_id)?.clone();
        let origin_id = from_id.filter(|&id| id != 0).unwrap_or(template_id);
        let origin = self
            .world
            .get::<Transform>(origin_id)
            .map(|t| t.position)
            .unwrap_or(template_t.position);

        let config = self.projectile_config_for(template_id);
        let rotation = [
            template_t.rotation.x,
            template_t.rotation.y,
            template_t.rotation.z,
            template_t.rotation.w,
        ];
        let scale = template_t.scale.to_array();

        let spawned = self.spawn_cached_model_from_save(
            &path,
            origin.to_array(),
            rotation,
            scale,
            Some(&format!("ProjectileRuntime_{template_id}")),
            Some("projectile".to_string()),
            None,
            false,
            "static",
            None,
            false,
        );
        let entity_id = match spawned {
            Ok(id) => id,
            Err(e) => {
                log::error!("[proyectil] no se pudo spawnear clon: {e}");
                return None;
            }
        };

        self.active_projectiles.push(ActiveProjectile {
            entity_id,
            velocity: dir * config.speed,
            age: 0.0,
            lifetime_s: config.lifetime_s,
        });
        log::info!(
            "[proyectil] disparado id={entity_id} plantilla={template_id} speed={} lifetime={}",
            config.speed,
            config.lifetime_s
        );
        Some(entity_id)
    }

    pub(crate) fn tick_projectiles(&mut self, dt: f32) {
        if self.active_projectiles.is_empty() || dt <= 0.0 {
            return;
        }
        let mut expired = Vec::new();
        for proj in &mut self.active_projectiles {
            proj.age += dt;
            if proj.age >= proj.lifetime_s {
                expired.push(proj.entity_id);
                continue;
            }
            if let Some(t) = self.world.get_mut::<Transform>(proj.entity_id) {
                t.position += proj.velocity * dt;
            }
        }
        for id in expired {
            self.despawn_runtime_projectile(id);
        }
    }

    pub(crate) fn clear_active_projectiles(&mut self) {
        let ids: Vec<EntityId> = self
            .active_projectiles
            .iter()
            .map(|p| p.entity_id)
            .collect();
        for id in ids {
            self.despawn_runtime_projectile(id);
        }
        self.active_projectiles.clear();
    }

    /// Elimina un proyectil runtime sin undo de editor.
    pub(crate) fn despawn_runtime_projectile(&mut self, id: EntityId) {
        self.active_projectiles.retain(|p| p.entity_id != id);
        if self.world.get::<Transform>(id).is_none() {
            return;
        }
        self.selected_entities.retain(|&e| e != id);
        if Some(id) == self.selected_entity {
            self.selected_entity = self.selected_entities.last().copied();
        }
        if self.selected_entities.is_empty() && self.selected_entity.is_none() {
            send_event(&EngineEvent::EntityDeselected);
        }
        if Some(id) == self.hovered_entity {
            self.hovered_entity = None;
            send_event(&EngineEvent::EntityUnhovered);
        }
        self.physics.remove_entity_body(id);
        self.scenario_entities.retain(|&e| e != id);
        self.character_entities.retain(|&e| e != id);
        self.collider_entities.retain(|&e| e != id);
        self.execution_area_entities.retain(|&e| e != id);
        self.unbind_model_animations(id);
        self.animations.remove(&id);
        self.active_animations.remove(&id);
        self.anim_saved_transforms.remove(&id);
        self.control_bindings_by_entity.remove(&id);
        self.script_engine.detach_entity(id);
        self.save_registry.remove_entity(id);
        self.entity_blueprint_ids.remove(&id);
        self.entity_colision.remove(&id);
        self.entity_projectile_config.remove(&id);
        self.clear_entity_attachments_for_removed(id);
        self.world.despawn(id);
        send_event(&EngineEvent::EntityRemoved {
            id,
            kind: "model".to_string(),
        });
    }
}
