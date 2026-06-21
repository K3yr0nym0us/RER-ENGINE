// ── Teletransporte de entidades 2D ───────────────────────────────────────────
//
// Sincroniza el Rapier body con la posición del Transform cuando un script
// mueve una entidad durante una animación, evitando que physics.step()
// sobreescriba la posición en el siguiente frame.

use rapier3d::prelude::*;

use crate::ecs::EntityId;

use super::PhysicsWorld2D;

impl PhysicsWorld2D {
    /// Sincroniza el body fisico con una mutacion EXTERNA del `Transform`.
    ///
    /// Usar este helper cuando el editor o una ruta de compatibilidad ya cambió
    /// la posicion visual y solo hace falta alinear el cuerpo Rapier.
    /// Para movimiento normal de gameplay debe usarse `move_physics_entity()`.
    pub(crate) fn sync_body_from_transform(&mut self, entity: EntityId, x: f32, y: f32) {
        self.teleport_entity(entity, x, y);
    }

    /// Teletransporta el Rapier body de la entidad a la posición indicada (XY).
    ///
    /// NO USAR para movimiento normal de entidades.
    /// - Movimiento de gameplay: `move_physics_entity()`.
    /// - Mutacion externa de `Transform`: `sync_body_from_transform()`.
    ///
    /// Esta API debe reservarse para teletransportes reales o para la
    /// resincronizacion puntual del cuerpo fisico tras una mutacion externa.
    pub(crate) fn teleport_entity(&mut self, entity: EntityId, x: f32, y: f32) {
        if let Some(&handle) = self.entity_bodies.get(&entity) {
            if let Some(body) = self.bodies.get_mut(handle) {
                body.set_translation(Vector::new(x, y, 0.0), true);
                body.set_linvel(Vector::ZERO, true);
            }
            self.kinematic_actor_vel.remove(&entity);
        }
    }
}
