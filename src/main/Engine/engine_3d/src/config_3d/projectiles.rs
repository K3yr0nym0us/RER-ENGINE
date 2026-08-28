//! Proyectiles 3D: config de plantilla + instancias cinemáticas (editor/play).

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::ecs::{EntityId, Transform};
use crate::engine::State;
use crate::ipc::{EngineEvent, send_event};

pub const DEFAULT_PROJECTILE_SPEED: f32 = 20.0;
pub const DEFAULT_PROJECTILE_LIFETIME_S: f32 = 3.0;
pub const DEFAULT_MUZZLE_SOCKET: &str = "muzzle";
pub const DEFAULT_MAX_BOUNCES: u32 = 3;
pub const DEFAULT_BOUNCE_SPEED_LOSS: f32 = 0.2;
/// Umbral de `SurfacePbr.metallic` para considerar la superficie “metal”.
pub const METAL_BOUNCE_THRESHOLD: f32 = 0.5;

fn default_align_to_velocity() -> bool {
    true
}

fn default_muzzle_socket() -> Option<String> {
    Some(DEFAULT_MUZZLE_SOCKET.to_string())
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

/// Parámetros de disparo persistidos en entidades categoría `projectile`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectileConfig {
    pub speed: f32,
    pub lifetime_s: f32,
    /// Si true, aplica gravedad del mundo (multiplicada por `gravity_scale`).
    #[serde(default)]
    pub affected_by_gravity: bool,
    /// Multiplicador de gravedad del mundo (solo si `affected_by_gravity`).
    #[serde(default = "default_gravity_scale")]
    pub gravity_scale: f32,
    /// Orientar el mesh al vector de velocidad (-Z local = forward).
    #[serde(default = "default_align_to_velocity")]
    pub align_to_velocity: bool,
    /// Socket de origen al disparar desde `from_id` (default `muzzle`).
    #[serde(default = "default_muzzle_socket")]
    pub muzzle_socket: Option<String>,
    /// Si true, rebota al impactar superficies metálicas (`SurfacePbr.metallic`).
    #[serde(default)]
    pub bounceable: bool,
    /// Rebotes restantes iniciales (solo si `bounceable`).
    #[serde(default = "default_max_bounces")]
    pub max_bounces: u32,
    /// Fracción de velocidad perdida por rebote (0..1).
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
            muzzle_socket: default_muzzle_socket(),
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
            muzzle_socket: self
                .muzzle_socket
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
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
    pub velocity: Vec3,
    pub age: f32,
    pub lifetime_s: f32,
    pub affected_by_gravity: bool,
    pub gravity_scale: f32,
    pub align_to_velocity: bool,
    /// Offset visual de la plantilla respecto a alinear -Z local a la velocidad.
    pub visual_offset: Quat,
    pub bounceable: bool,
    pub bounces_left: u32,
    pub bounce_speed_loss: f32,
    /// Entidades a excluir del raycast de impacto (plantilla / tirador).
    pub exclude_entities: Vec<EntityId>,
}

fn projectile_rotation_for_velocity(visual_offset: Quat, velocity: Vec3) -> Quat {
    let vel = velocity.normalize_or_zero();
    if vel.length_squared() <= 1e-8 {
        return visual_offset;
    }
    Quat::from_rotation_arc(Vec3::NEG_Z, vel) * visual_offset
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
            muzzle_socket: config.muzzle_socket.clone(),
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

    /// Busca socket `name` en `host_id` o en hijos enganchados a ese host (arma en personaje).
    pub(crate) fn resolve_muzzle_world(
        &self,
        host_id: EntityId,
        socket_name: &str,
    ) -> Option<Transform> {
        if let Some(socket) = self.find_socket_on_host(host_id, socket_name)
            && let Some(world) = self.socket_world_for_entity(host_id, &socket)
        {
            return Some(world);
        }
        let child_hosts: Vec<EntityId> = self
            .entity_attachments
            .iter()
            .filter_map(|(child_id, attachment)| match &attachment.anchor {
                crate::config_3d::entity_attachments::AttachmentAnchor::Socket {
                    host_entity_id,
                    ..
                } if *host_entity_id == host_id => Some(*child_id),
                _ => None,
            })
            .collect();
        for child_id in child_hosts {
            if let Some(socket) = self.find_socket_on_host(child_id, socket_name)
                && let Some(world) = self.socket_world_for_entity(child_id, &socket)
            {
                return Some(world);
            }
        }
        None
    }

    /// Dispara un clon cinemático de la plantilla `template_id`.
    /// Funciona en editor y en play (`from_id == 0` / `None` → origen = plantilla / muzzle).
    pub(crate) fn fire_projectile_from_template(
        &mut self,
        template_id: EntityId,
        from_id: Option<EntityId>,
        dir: Vec3,
    ) -> Option<EntityId> {
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
        let config = self.projectile_config_for(template_id);
        let origin_id = from_id.filter(|&id| id != 0).unwrap_or(template_id);

        let muzzle_name = config
            .muzzle_socket
            .as_deref()
            .unwrap_or(DEFAULT_MUZZLE_SOCKET);
        let muzzle = self.resolve_muzzle_world(origin_id, muzzle_name);

        let (origin, muzzle_rot) = if let Some(m) = muzzle {
            (m.position, Some(m.rotation))
        } else {
            let pos = self
                .world
                .get::<Transform>(origin_id)
                .map(|t| t.position)
                .unwrap_or(template_t.position);
            (pos, None)
        };

        let dir = {
            let len = dir.length();
            if len > f32::EPSILON {
                dir / len
            } else if let Some(rot) = muzzle_rot {
                // Solo el socket muzzle define forward al disparar desde un host (arma/personaje).
                rot * Vec3::NEG_Z
            } else {
                // Plantilla en escena: trayectoria fija en mundo (-Z), independiente de girar el mesh.
                Vec3::NEG_Z
            }
        };
        let dir = {
            let len = dir.length();
            if len <= f32::EPSILON {
                log::warn!("[proyectil] fire ignorado: dirección nula tras resolver forward");
                return None;
            }
            dir / len
        };

        let rotation = [
            template_t.rotation.x,
            template_t.rotation.y,
            template_t.rotation.z,
            template_t.rotation.w,
        ];
        let visual_offset =
            Quat::from_rotation_arc(Vec3::NEG_Z, dir).inverse() * template_t.rotation;
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

        let mut exclude = vec![template_id, origin_id, entity_id];
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
            visual_offset,
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
            "[proyectil] disparado id={entity_id} plantilla={template_id} speed={} lifetime={} gravity={}×{} bounce={}",
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
        let gravity = self.physics.gravity_magnitude();
        let mut expired = Vec::new();
        let mut hits: Vec<(EntityId, Option<EntityId>, Vec3, bool)> = Vec::new();

        for proj in &mut self.active_projectiles {
            proj.age += dt;
            if proj.age >= proj.lifetime_s {
                expired.push(proj.entity_id);
                continue;
            }
            if proj.affected_by_gravity && proj.gravity_scale > 0.0 {
                proj.velocity.y -= gravity * proj.gravity_scale * dt;
            }
            let Some(prev) = self
                .world
                .get::<Transform>(proj.entity_id)
                .map(|t| t.position)
            else {
                expired.push(proj.entity_id);
                continue;
            };
            let delta = proj.velocity * dt;
            let dist = delta.length();
            let next = prev + delta;

            let mut hit_pos = None;
            let mut hit_entity = None;
            let mut hit_normal = Vec3::Y;
            if dist > 1e-6 {
                let exclude: Vec<_> = proj
                    .exclude_entities
                    .iter()
                    .filter_map(|&id| self.physics.collider_handle_for_entity(id))
                    .collect();
                if let Some((toi, handle, normal)) =
                    self.physics
                        .raycast_first_hit(prev, delta / dist, dist, &exclude)
                {
                    let pos = prev + (delta / dist) * toi;
                    hit_pos = Some(pos);
                    hit_entity = self.physics.entity_for_collider(handle);
                    hit_normal = if normal.length_squared() > 1e-8 {
                        normal.normalize()
                    } else {
                        Vec3::Y
                    };
                }
            }

            if let Some(pos) = hit_pos {
                let is_metal = match hit_entity {
                    Some(id) => self
                        .world
                        .get::<crate::ecs::SurfacePbr>(id)
                        .is_some_and(|pbr| pbr.metallic >= METAL_BOUNCE_THRESHOLD),
                    None => false,
                };
                let can_bounce = proj.bounceable && proj.bounces_left > 0 && is_metal;

                if can_bounce {
                    let retain = (1.0 - proj.bounce_speed_loss).max(0.0);
                    let reflected =
                        proj.velocity - 2.0 * proj.velocity.dot(hit_normal) * hit_normal;
                    proj.velocity = reflected * retain;
                    proj.bounces_left = proj.bounces_left.saturating_sub(1);
                    let nudge = hit_normal * 0.02;
                    if let Some(t) = self.world.get_mut::<Transform>(proj.entity_id) {
                        t.position = pos + nudge;
                        if proj.align_to_velocity {
                            t.rotation =
                                projectile_rotation_for_velocity(proj.visual_offset, proj.velocity);
                        }
                    }
                    hits.push((proj.entity_id, hit_entity, pos, true));
                    continue;
                }

                if let Some(t) = self.world.get_mut::<Transform>(proj.entity_id) {
                    t.position = pos;
                    if proj.align_to_velocity {
                        t.rotation =
                            projectile_rotation_for_velocity(proj.visual_offset, proj.velocity);
                    }
                }
                hits.push((proj.entity_id, hit_entity, pos, false));
                expired.push(proj.entity_id);
                continue;
            }

            if let Some(t) = self.world.get_mut::<Transform>(proj.entity_id) {
                t.position = next;
                if proj.align_to_velocity {
                    t.rotation =
                        projectile_rotation_for_velocity(proj.visual_offset, proj.velocity);
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
