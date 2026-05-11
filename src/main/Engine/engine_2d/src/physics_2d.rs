// ── Física 2D — simulación en plano XY ───────────────────────────────────────
//
// Este binario implementa comportamiento 2D. El backend de física usa la crate
// `rapier3d` por compatibilidad técnica, pero con ejes bloqueados para mantener
// dinámica estrictamente en XY. Gravedad en -Y (9.81 u/s²).
//
// Tipos de cuerpo soportados:
//   "dynamic"   — afectado por gravedad y colisiones.
//   "static"    — no se mueve (suelo, plataformas).
//
// Funciones extraídas a archivos propios (submódulos vía #[path]):
//   teleport_entity      — sincroniza Rapier body con el Transform.
//   move_physics_entity  — mueve con velocidad lineal respetando colisiones.

#[path = "teleport_entity.rs"]
mod teleport_entity;

#[path = "move_physics_entity.rs"]
mod move_physics_entity;

use std::collections::HashMap;

use rapier3d::prelude::*;

use crate::ecs::{EntityId, Transform, World};

// ── Mundo físico 2D ───────────────────────────────────────────────────────────
pub(crate) struct PhysicsWorld2D {
    gravity:            Vector<f32>,
    integration_params: IntegrationParameters,
    physics_pipeline:   PhysicsPipeline,
    island_manager:     IslandManager,
    broad_phase:        DefaultBroadPhase,
    narrow_phase:       NarrowPhase,
    bodies:             RigidBodySet,
    colliders:          ColliderSet,
    impulse_joints:     ImpulseJointSet,
    multibody_joints:   MultibodyJointSet,
    ccd_solver:         CCDSolver,
    query_pipeline:     QueryPipeline,
    entity_bodies:      HashMap<EntityId, RigidBodyHandle>,
    entity_body_types:  HashMap<EntityId, String>,
    /// Collider handle associated to each entity, used by move_physics_entity
    /// to query the narrow phase and detect blocking contacts before applying velocity.
    entity_colliders:   HashMap<EntityId, ColliderHandle>,
}

impl Default for PhysicsWorld2D {
    fn default() -> Self {
        Self {
            gravity:            vector![0.0, -9.81, 0.0],
            integration_params: IntegrationParameters::default(),
            physics_pipeline:   PhysicsPipeline::new(),
            island_manager:     IslandManager::new(),
            broad_phase:        DefaultBroadPhase::new(),
            narrow_phase:       NarrowPhase::new(),
            bodies:             RigidBodySet::new(),
            colliders:          ColliderSet::new(),
            impulse_joints:     ImpulseJointSet::new(),
            multibody_joints:   MultibodyJointSet::new(),
            ccd_solver:         CCDSolver::new(),
            query_pipeline:     QueryPipeline::new(),
            entity_bodies:      HashMap::new(),
            entity_body_types:  HashMap::new(),
            entity_colliders:   HashMap::new(),
        }
    }
}

impl PhysicsWorld2D {
    pub(crate) fn new() -> Self { Self::default() }

    /// Número de rigid bodies activos en el mundo físico 2D.
    pub(crate) fn body_count(&self) -> u32 { self.bodies.len() as u32 }

    /// Cambia la gravedad del mundo físico 2D en el eje Y (negativo = hacia abajo).
    pub(crate) fn set_gravity(&mut self, gravity_y: f32) {
        self.gravity = vector![0.0, gravity_y, 0.0];
    }

    // ── Gestión de cuerpos por entidad ────────────────────────────────────────

    /// Activa o desactiva física en una entidad 2D.
    /// position: centro de la entidad en unidades de mundo (XY).
    /// half_ext: semidimensiones de la caja colisionadora (XY; Z se ignora).
    pub(crate) fn set_entity_physics(
        &mut self,
        entity:    EntityId,
        enabled:   bool,
        body_type: &str,
        position:  [f32; 3],
        half_ext:  [f32; 3],
        collider_offset: [f32; 3],
    ) {
        // Eliminar cuerpo previo si existe (incluyendo su collider handle)
        if let Some(handle) = self.entity_bodies.remove(&entity) {
            self.entity_body_types.remove(&entity);
            self.entity_colliders.remove(&entity);
            self.remove_body(handle);
        }
        if !enabled { return; }

        let hx = half_ext[0].max(0.01);
        let hy = half_ext[1].max(0.01);
        let offset = vector![collider_offset[0], collider_offset[1], collider_offset[2]];

        let handle = match body_type {
            "static" => {
                let body = RigidBodyBuilder::fixed()
                    .translation(vector![position[0], position[1], 0.0])
                    .build();
                let handle = self.bodies.insert(body);
                let col = ColliderBuilder::cuboid(hx, hy, 0.01)
                    .translation(offset)
                    .build();
                let col_handle = self.colliders.insert_with_parent(col, handle, &mut self.bodies);
                self.entity_colliders.insert(entity, col_handle);
                handle
            }
            _ => {
                // "dynamic" — bloqueamos Z y rotaciones X/Y para comportamiento 2D puro.
                // CCD habilitado: garantiza que Rapier no permita traversal incluso a
                // velocidades altas, actuando como última línea de defensa.
                let body = RigidBodyBuilder::dynamic()
                    .translation(vector![position[0], position[1], 0.0])
                    .locked_axes(
                        LockedAxes::TRANSLATION_LOCKED_Z
                        | LockedAxes::ROTATION_LOCKED_X
                        | LockedAxes::ROTATION_LOCKED_Y,
                    )
                    .ccd_enabled(true)
                    .build();
                let handle = self.bodies.insert(body);
                let col = ColliderBuilder::cuboid(hx, hy, 0.01)
                    .translation(offset)
                    .restitution(0.0)
                    .friction(0.5)
                    .build();
                let col_handle = self.colliders.insert_with_parent(col, handle, &mut self.bodies);
                self.entity_colliders.insert(entity, col_handle);
                handle
            }
        };
        self.entity_bodies.insert(entity, handle);
        self.entity_body_types.insert(entity, body_type.to_string());
    }

    /// Actualiza la posición del rigid body en el mundo físico (sin recrearlo).
    #[allow(dead_code)]
    pub(crate) fn set_entity_body_position(&mut self, entity: EntityId, position: [f32; 3]) {
        if let Some(&handle) = self.entity_bodies.get(&entity) {
            if let Some(body) = self.bodies.get_mut(handle) {
                body.set_translation(vector![position[0], position[1], 0.0], true);
            }
        }
    }

    /// Actualiza collider in-place: cambia forma y offset sin eliminar el
    /// collider handle, preservando datos de CCD y contactos de Rapier.
    pub(crate) fn update_entity_collider(
        &mut self,
        entity:   EntityId,
        half_ext: [f32; 3],
        collider_offset: [f32; 3],
    ) {
        let Some(&body_handle) = self.entity_bodies.get(&entity) else {
            return;
        };
        let Some(&col_handle) = self.entity_colliders.get(&entity) else {
            let hx = half_ext[0].max(0.01);
            let hy = half_ext[1].max(0.01);
            let off = vector![collider_offset[0], collider_offset[1], collider_offset[2]];
            let col = ColliderBuilder::cuboid(hx, hy, 0.01)
                .translation(off)
                .restitution(0.0)
                .friction(0.5)
                .build();
            let new_handle = self.colliders.insert_with_parent(col, body_handle, &mut self.bodies);
            self.entity_colliders.insert(entity, new_handle);
            return;
        };

        let hx = half_ext[0].max(0.01);
        let hy = half_ext[1].max(0.01);

        if let Some(col) = self.colliders.get_mut(col_handle) {
            col.set_shape(SharedShape::cuboid(hx, hy, 0.01));
            col.set_position_wrt_parent(Isometry::translation(
                collider_offset[0],
                collider_offset[1],
                collider_offset[2],
            ));
        }
    }

    pub(crate) fn remove_entity_body(&mut self, entity: EntityId) {
        if let Some(handle) = self.entity_bodies.remove(&entity) {
            self.entity_body_types.remove(&entity);
            self.entity_colliders.remove(&entity);
            self.remove_body(handle);
        }
    }

    fn remove_body(&mut self, handle: RigidBodyHandle) {
        self.bodies.remove(
            handle,
            &mut self.island_manager,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            true,
        );
    }

    /// Limpia todos los cuerpos físicos (al cambiar de escena).
    pub(crate) fn clear(&mut self) {
        let handles: Vec<RigidBodyHandle> = self.entity_bodies.values().copied().collect();
        for h in handles { self.remove_body(h); }
        self.entity_bodies.clear();
        self.entity_body_types.clear();
        self.entity_colliders.clear();
    }

    pub(crate) fn has_physics(&self, entity: EntityId) -> bool {
        self.entity_bodies.contains_key(&entity)
    }

    pub(crate) fn get_body_type(&self, entity: EntityId) -> &str {
        self.entity_body_types.get(&entity).map(|s| s.as_str()).unwrap_or("")
    }

    // ── Paso de simulación ────────────────────────────────────────────────────

    pub(crate) fn step(&mut self, dt: f32, ecs: &mut World) {
        if self.entity_bodies.is_empty() { return; }

        self.integration_params.dt = dt.clamp(0.0001, 0.05);

        self.physics_pipeline.step(
            &self.gravity,
            &self.integration_params,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            Some(&mut self.query_pipeline),
            &(),
            &(),
        );

        // Sincronizar solo cuerpos dinámicos de vuelta al ECS
        let pairs: Vec<(EntityId, RigidBodyHandle)> =
            self.entity_bodies.iter().map(|(&e, &h)| (e, h)).collect();
        for (entity, handle) in pairs {
            if let Some(body) = self.bodies.get(handle) {
                if body.is_dynamic() {
                    let t = body.translation();
                    if let Some(transform) = ecs.get_mut::<Transform>(entity) {
                        transform.position.x = t.x;
                        transform.position.y = t.y;
                        // Z no se toca — lo gestiona el editor para orden de capas
                    }
                }
            }
        }
    }
}
