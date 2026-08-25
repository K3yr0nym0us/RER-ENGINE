// ── Intención de velocidad para KinematicPositionBased ──
//
// Rapier ignora `set_linvel` en ese tipo.  Los valores viven en
// `kinematic_actor_vel` y `PhysicsWorld2D::step()` integra gravedad + shape-cast.

use rapier3d::prelude::*;

use super::PhysicsWorld2D;
use crate::ecs::EntityId;

impl PhysicsWorld2D {
    /// Marca velocidad objetivo horizontal y suma salto vertical para el siguiente `step()`.
    ///
    /// Nota de contrato actual: `gravity` y `dt` se conservan en la firma por
    /// compatibilidad con las llamadas existentes, aunque esta implementacion
    /// mantiene la integracion real centralizada en `PhysicsWorld2D::step()`.
    pub(crate) fn apply_kinematic_gravity(
        &mut self,
        entity: EntityId,
        speed_x: f32,
        jump_speed_y: f32,
        _gravity: f32,
        _dt: f32,
        on_ground: Option<&mut bool>,
    ) -> bool {
        let Some(&body_handle) = self.entity_bodies.get(&entity) else {
            return false;
        };
        if self.get_body_type(entity) != "kinematic" {
            return false;
        }
        let vy_from_rapier = self
            .bodies
            .get(body_handle)
            .map(|b| b.linvel().y)
            .unwrap_or(0.0);
        let mut cur = self
            .kinematic_actor_vel
            .remove(&entity)
            .unwrap_or(Vector::new(speed_x, vy_from_rapier, 0.0));
        cur.x = speed_x;
        cur.y += jump_speed_y;
        self.kinematic_actor_vel.insert(entity, cur);
        if let Some(ground_ref) = on_ground {
            *ground_ref = false;
        }
        true
    }

    /// Suma un impulso planar a la intención guardada.
    pub(crate) fn apply_kinematic_impulse(
        &mut self,
        entity: EntityId,
        dir_x: f32,
        dir_y: f32,
        impulse: f32,
    ) -> bool {
        let Some(&body_handle) = self.entity_bodies.get(&entity) else {
            return false;
        };
        if self.get_body_type(entity) != "kinematic" {
            return false;
        }
        let len = (dir_x * dir_x + dir_y * dir_y).sqrt();
        if len <= 1e-6 {
            return true;
        }
        let v = self
            .bodies
            .get(body_handle)
            .map(|b| b.linvel())
            .unwrap_or(Vector::ZERO);
        let mut cur = self.kinematic_actor_vel.remove(&entity).unwrap_or(v);
        cur.x += dir_x / len * impulse;
        cur.y += dir_y / len * impulse;
        self.kinematic_actor_vel.insert(entity, cur);
        true
    }
}
