//! Proyectiles 2D: config de plantilla + instancias cinemáticas (editor/play).

use glam::{Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::config_2d::ProjectileMarker;
use crate::ecs::{EntityId, MeshComponent, Transform};
use crate::engine::State;
use crate::ipc::{EngineEvent, send_event};

pub const DEFAULT_PROJECTILE_SPEED: f32 = 20.0;
pub const DEFAULT_PROJECTILE_LIFETIME_S: f32 = 3.0;
pub const DEFAULT_MAX_BOUNCES: u32 = 3;
pub const DEFAULT_BOUNCE_SPEED_LOSS: f32 = 0.2;
/// Dirección mundo por defecto al disparar plantilla con dir cero (test fire / trayectoria fija).
pub const DEFAULT_PROJECTILE_DIR_2D: Vec2 = Vec2::X;

fn default_align_to_velocity() -> bool {
    true
}

fn default_max_bounces() -> u32 {
    DEFAULT_MAX_BOUNCES
}

fn default_bounce_speed_loss() -> f32 {
    DEFAULT_BOUNCE_SPEED_LOSS
}

fn default_gravity_scale() -> f32 {
    1.0
}

/// Parámetros de disparo persistidos en entidades `kind == projectile`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectileConfig {
    pub speed: f32,
    pub lifetime_s: f32,
    #[serde(default)]
    pub affected_by_gravity: bool,
    #[serde(default = "default_gravity_scale")]
    pub gravity_scale: f32,
    #[serde(default = "default_align_to_velocity")]
    pub align_to_velocity: bool,
    #[serde(default)]
    pub bounceable: bool,
    #[serde(default = "default_max_bounces")]
    pub max_bounces: u32,
    #[serde(default = "default_bounce_speed_loss")]
    pub bounce_speed_loss: f32,
}

impl Default for ProjectileConfig {
    fn default() -> Self {
        Self {
            speed: DEFAULT_PROJECTILE_SPEED,
            lifetime_s: DEFAULT_PROJECTILE_LIFETIME_S,
            affected_by_gravity: false,
            gravity_scale: default_gravity_scale(),
            align_to_velocity: true,
            bounceable: false,
            max_bounces: DEFAULT_MAX_BOUNCES,
            bounce_speed_loss: DEFAULT_BOUNCE_SPEED_LOSS,
        }
    }
}

impl ProjectileConfig {
    pub fn clamped(self) -> Self {
        Self {
            speed: self.speed.max(0.0),
            lifetime_s: self.lifetime_s.max(0.05),
            affected_by_gravity: self.affected_by_gravity,
            gravity_scale: self.gravity_scale.max(0.0),
            align_to_velocity: self.align_to_velocity,
            bounceable: self.bounceable,
            max_bounces: self.max_bounces.min(64),
            bounce_speed_loss: self.bounce_speed_loss.clamp(0.0, 1.0),
        }
    }
}

/// Instancia runtime (no se serializa en `.save`).
#[derive(Clone, Debug)]
pub(crate) struct ActiveProjectile {
    pub entity_id: EntityId,
    pub velocity: Vec2,
    pub age: f32,
    pub lifetime_s: f32,
    pub affected_by_gravity: bool,
    pub gravity_scale: f32,
    pub align_to_velocity: bool,
    /// Offset angular de la plantilla respecto a alinear +X local a la velocidad.
    pub visual_angle_offset: f32,
    pub bounceable: bool,
    pub bounces_left: u32,
    pub bounce_speed_loss: f32,
    pub exclude_entities: Vec<EntityId>,
}

fn quat_forward_x_angle(rotation: Quat) -> f32 {
    let forward = rotation * Vec3::X;
    forward.y.atan2(forward.x)
}

fn projectile_rotation_for_velocity(visual_angle_offset: f32, velocity: Vec2) -> Quat {
    let vel = velocity.normalize_or_zero();
    if vel.length_squared() <= 1e-8 {
        return Quat::from_rotation_z(visual_angle_offset);
    }
    Quat::from_rotation_z(vel.y.atan2(vel.x) + visual_angle_offset)
}

impl State {
    pub(crate) fn projectile_config_for(&self, entity_id: EntityId) -> ProjectileConfig {
        self.entity_projectile_config
            .get(&entity_id)
            .cloned()
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
            .insert(entity_id, config.clone().clamped());
    }

    pub(crate) fn emit_projectile_config_changed(&self, entity_id: EntityId) {
        let config = self.projectile_config_for(entity_id);
        send_event(&EngineEvent::ProjectileConfigChanged {
            entity_id,
            speed: config.speed,
            lifetime_s: config.lifetime_s,
            affected_by_gravity: config.affected_by_gravity,
            gravity_scale: config.gravity_scale,
            align_to_velocity: config.align_to_velocity,
            bounceable: config.bounceable,
            max_bounces: config.max_bounces,
            bounce_speed_loss: config.bounce_speed_loss,
        });
    }

    pub(crate) fn is_runtime_projectile(&self, entity_id: EntityId) -> bool {
        self.active_projectiles
            .iter()
            .any(|p| p.entity_id == entity_id)
    }

    fn projectile_hit_accepts_entity(&self, entity: EntityId) -> bool {
        !self.execution_area_entities.contains(&entity)
    }

    /// Dispara un clon cinemático de la plantilla `template_id`.
    /// Origen siempre en la plantilla. `from_id` opcional: tirador a excluir de impactos.
    pub(crate) fn fire_projectile_from_template(
        &mut self,
        template_id: EntityId,
        from_id: Option<EntityId>,
        dir: Vec2,
    ) -> Option<EntityId> {
        let meta = self.resolve_entity_save_meta(template_id)?;
        if meta.kind != "projectile" {
            log::warn!("[proyectil-2d] plantilla {template_id} no es kind projectile");
            return None;
        }
        if self.world.get::<ProjectileMarker>(template_id).is_none() {
            log::warn!("[proyectil-2d] plantilla {template_id} sin ProjectileMarker");
            return None;
        }
        let template_t = self.world.get::<Transform>(template_id)?.clone();
        let mesh = self.world.get::<MeshComponent>(template_id)?.clone();
        let config = self.projectile_config_for(template_id);
        let shooter_id = from_id.filter(|&id| id != 0 && id != template_id);

        let origin_xy = Vec2::new(template_t.position.x, template_t.position.y);

        let dir = {
            let len = dir.length();
            if len > f32::EPSILON {
                dir / len
            } else {
                DEFAULT_PROJECTILE_DIR_2D
            }
        };
        let dir = {
            let len = dir.length();
            if len <= f32::EPSILON {
                log::warn!("[proyectil-2d] fire ignorado: dirección nula tras resolver forward");
                return None;
            }
            dir / len
        };

        let visual_angle_offset = quat_forward_x_angle(template_t.rotation) - dir.y.atan2(dir.x);

        let entity_name =
            self.next_numbered_entity_name(&format!("ProjectileRuntime_{template_id}"));
        let entity_id = self.world.spawn(Some(&entity_name));
        self.world.insert(entity_id, mesh);
        self.world.insert(
            entity_id,
            Transform {
                position: Vec3::new(origin_xy.x, origin_xy.y, template_t.position.z),
                rotation: template_t.rotation,
                scale: template_t.scale,
            },
        );

        let mut exclude = vec![template_id, entity_id];
        if let Some(shooter) = shooter_id {
            exclude.push(shooter);
        }
        exclude.sort_unstable();
        exclude.dedup();

        self.active_projectiles.push(ActiveProjectile {
            entity_id,
            velocity: dir * config.speed,
            age: 0.0,
            lifetime_s: config.lifetime_s,
            affected_by_gravity: config.affected_by_gravity,
            gravity_scale: config.gravity_scale,
            align_to_velocity: config.align_to_velocity,
            visual_angle_offset,
            bounceable: config.bounceable,
            bounces_left: if config.bounceable {
                config.max_bounces
            } else {
                0
            },
            bounce_speed_loss: config.bounce_speed_loss,
            exclude_entities: exclude,
        });
        log::info!(
            "[proyectil-2d] disparado id={entity_id} plantilla={template_id} speed={} lifetime={} gravity={}×{} bounce={}",
            config.speed,
            config.lifetime_s,
            config.affected_by_gravity,
            config.gravity_scale,
            config.bounceable
        );
        Some(entity_id)
    }

    pub(crate) fn tick_projectiles(&mut self, dt: f32) {
        if self.active_projectiles.is_empty() || dt <= 0.0 {
            return;
        }
        let gravity = self.physics_2d.gravity_magnitude();
        let mut expired = Vec::new();
        let mut hits: Vec<(EntityId, Option<EntityId>, Vec3, bool)> = Vec::new();

        let count = self.active_projectiles.len();
        for idx in 0..count {
            let entity_id = self.active_projectiles[idx].entity_id;
            self.active_projectiles[idx].age += dt;
            if self.active_projectiles[idx].age >= self.active_projectiles[idx].lifetime_s {
                expired.push(entity_id);
                continue;
            }
            if self.active_projectiles[idx].affected_by_gravity
                && self.active_projectiles[idx].gravity_scale > 0.0
            {
                self.active_projectiles[idx].velocity.y -=
                    gravity * self.active_projectiles[idx].gravity_scale * dt;
            }
            let Some(prev_t) = self.world.get::<Transform>(entity_id).cloned() else {
                expired.push(entity_id);
                continue;
            };
            let prev = Vec2::new(prev_t.position.x, prev_t.position.y);
            let delta = self.active_projectiles[idx].velocity * dt;
            let dist = delta.length();
            let next = prev + delta;
            let exclude_entities = self.active_projectiles[idx].exclude_entities.clone();

            let mut hit_pos = None;
            let mut hit_entity = None;
            let mut hit_normal = Vec2::Y;
            if dist > 1e-6
                && let Some((toi, hit_entity_id, normal)) =
                    self.physics_2d.raycast_projectile_hit_xy(
                        prev,
                        delta / dist,
                        dist,
                        &exclude_entities,
                        |entity| self.projectile_hit_accepts_entity(entity),
                    )
            {
                let pos_xy = prev + (delta / dist) * toi;
                hit_pos = Some(Vec3::new(pos_xy.x, pos_xy.y, prev_t.position.z));
                hit_entity = Some(hit_entity_id);
                hit_normal = if normal.length_squared() > 1e-8 {
                    normal.normalize()
                } else {
                    Vec2::Y
                };
            }

            if let Some(pos) = hit_pos {
                let bounceable = self.active_projectiles[idx].bounceable;
                let bounces_left = self.active_projectiles[idx].bounces_left;
                let can_bounce = bounceable && bounces_left > 0;

                if can_bounce {
                    let bounce_speed_loss = self.active_projectiles[idx].bounce_speed_loss;
                    let visual_angle_offset = self.active_projectiles[idx].visual_angle_offset;
                    let align_to_velocity = self.active_projectiles[idx].align_to_velocity;
                    let retain = (1.0 - bounce_speed_loss).max(0.0);
                    let v = self.active_projectiles[idx].velocity;
                    let reflected = v - 2.0 * v.dot(hit_normal) * hit_normal;
                    self.active_projectiles[idx].velocity = reflected * retain;
                    self.active_projectiles[idx].bounces_left =
                        self.active_projectiles[idx].bounces_left.saturating_sub(1);
                    if let Some(hit_id) = hit_entity {
                        let exclude = &mut self.active_projectiles[idx].exclude_entities;
                        if !exclude.contains(&hit_id) {
                            exclude.push(hit_id);
                        }
                    }
                    let nudge = hit_normal * 0.02;
                    if let Some(t) = self.world.get_mut::<Transform>(entity_id) {
                        t.position.x = pos.x + nudge.x;
                        t.position.y = pos.y + nudge.y;
                        if align_to_velocity {
                            t.rotation = projectile_rotation_for_velocity(
                                visual_angle_offset,
                                self.active_projectiles[idx].velocity,
                            );
                        }
                    }
                    hits.push((entity_id, hit_entity, pos, true));
                    continue;
                }

                let visual_angle_offset = self.active_projectiles[idx].visual_angle_offset;
                let align_to_velocity = self.active_projectiles[idx].align_to_velocity;
                if let Some(t) = self.world.get_mut::<Transform>(entity_id) {
                    t.position = pos;
                    if align_to_velocity {
                        t.rotation = projectile_rotation_for_velocity(
                            visual_angle_offset,
                            self.active_projectiles[idx].velocity,
                        );
                    }
                }
                hits.push((entity_id, hit_entity, pos, false));
                expired.push(entity_id);
                continue;
            }

            let visual_angle_offset = self.active_projectiles[idx].visual_angle_offset;
            let align_to_velocity = self.active_projectiles[idx].align_to_velocity;
            if let Some(t) = self.world.get_mut::<Transform>(entity_id) {
                t.position.x = next.x;
                t.position.y = next.y;
                if align_to_velocity {
                    t.rotation = projectile_rotation_for_velocity(
                        visual_angle_offset,
                        self.active_projectiles[idx].velocity,
                    );
                }
            }
        }

        for (projectile_id, hit_entity_id, position, bounced) in hits {
            send_event(&EngineEvent::ProjectileHit {
                projectile_id,
                hit_entity_id,
                position: position.to_array(),
                bounced,
            });
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
        self.physics_2d.remove_entity_body(id);
        self.scenario_entities.retain(|&e| e != id);
        self.character_entities.retain(|&e| e != id);
        self.projectile_entities.retain(|&e| e != id);
        self.collider_entities.retain(|&e| e != id);
        self.execution_area_entities.retain(|&e| e != id);
        self.animations.remove(&id);
        self.active_animations.remove(&id);
        self.anim_saved_transforms.remove(&id);
        self.control_bindings_by_entity.remove(&id);
        self.script_engine.detach_entity(id);
        self.save_registry.remove_entity(id);
        self.entity_projectile_config.remove(&id);
        self.world.despawn(id);
        send_event(&EngineEvent::EntityRemoved {
            id,
            kind: "projectile".to_string(),
            points: None,
        });
    }
}
