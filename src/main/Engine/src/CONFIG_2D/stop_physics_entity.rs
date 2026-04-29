// ── Detención forzada de entidades 2D ────────────────────────────────────────
//
// Pone a cero la velocidad lineal y angular del Rapier body para detener
// instantáneamente una entidad en movimiento desde un script Lua.
// Útil al terminar una acción de movimiento para evitar que la inercia
// arrastre al personaje más allá de lo deseado.

use rapier3d::prelude::*;

use crate::ecs::EntityId;

use super::PhysicsWorld2D;

impl PhysicsWorld2D {
    /// Detiene instantáneamente la entidad poniendo a cero sus velocidades
    /// lineal y angular. No altera la posición ni el tipo de cuerpo.
    pub(crate) fn stop_entity(&mut self, entity: EntityId) {
        if let Some(&handle) = self.entity_bodies.get(&entity) {
            if let Some(body) = self.bodies.get_mut(handle) {
                body.set_linvel(vector![0.0, 0.0, 0.0], true);
                body.set_angvel(vector![0.0, 0.0, 0.0], true);
            }
        }
    }
}
