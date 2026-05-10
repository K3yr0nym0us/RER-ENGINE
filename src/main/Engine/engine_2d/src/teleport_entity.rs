// ── Teletransporte de entidades 2D ───────────────────────────────────────────
//
// Sincroniza el Rapier body con la posición del Transform cuando un script
// mueve una entidad durante una animación, evitando que physics.step()
// sobreescriba la posición en el siguiente frame.

use rapier3d::prelude::*;

use crate::ecs::EntityId;

use super::PhysicsWorld2D;

impl PhysicsWorld2D {
    /// Teletransporta el Rapier body de la entidad a la posición indicada (XY).
    /// Necesario cuando los scripts mueven la entidad durante una animación para
    /// mantener el Rapier body sincronizado y evitar que physics.step() sobreescriba
    /// la posición del Transform en el siguiente frame.
    pub(crate) fn teleport_entity(&mut self, entity: EntityId, x: f32, y: f32) {
        if let Some(&handle) = self.entity_bodies.get(&entity) {
            if let Some(body) = self.bodies.get_mut(handle) {
                body.set_translation(vector![x, y, 0.0], true);
                body.set_linvel(vector![0.0, 0.0, 0.0], true);
            }
        }
    }
}
