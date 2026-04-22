// ── Física 3D — integración con Rapier3D ─────────────────────────────────────
//
// Solo activo en el modo 3D. Mantiene rigid bodies y colliders, avanza la
// simulación cada frame y sincroniza posiciones de vuelta al ECS.

use std::collections::HashMap;

use rapier3d::prelude::*;

use crate::ecs::{EntityId, Transform, World};

// ── Handle para referenciar un rigid body desde el ECS ───────────────────────
#[derive(Clone, Debug)]
pub(crate) struct RigidBodyComponent {
    pub(crate) handle: RigidBodyHandle,
}

// ── Mundo físico ─────────────────────────────────────────────────────────────
pub(crate) struct PhysicsWorld {
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
    // Tracking per-entity: qué entidades tienen física y de qué tipo.
    entity_bodies:      HashMap<EntityId, RigidBodyHandle>,
    entity_body_types:  HashMap<EntityId, String>,
}

impl Default for PhysicsWorld {
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
        }
    }
}

impl PhysicsWorld {
    pub(crate) fn new() -> Self { Self::default() }

    // ── Añadir cuerpos ───────────────────────────────────────────────────────

    /// Rigid body dinámico (cae por gravedad) con colisionador esférico.
    pub(crate) fn add_dynamic_sphere(
        &mut self,
        position: [f32; 3],
        radius:   f32,
    ) -> RigidBodyHandle {
        let body = RigidBodyBuilder::dynamic()
            .translation(vector![position[0], position[1], position[2]])
            .build();
        let handle = self.bodies.insert(body);
        let collider = ColliderBuilder::ball(radius).restitution(0.5).build();
        self.colliders.insert_with_parent(collider, handle, &mut self.bodies);
        handle
    }

    /// Plano estático (suelo) en y = 0.
    pub(crate) fn add_static_ground(&mut self) -> ColliderHandle {
        let collider = ColliderBuilder::halfspace(
            UnitVector::new_normalize(vector![0.0, 1.0, 0.0])
        ).build();
        self.colliders.insert(collider)
    }

    /// Caja dinámica.
    pub(crate) fn add_dynamic_box(
        &mut self,
        position:     [f32; 3],
        half_extents: [f32; 3],
    ) -> RigidBodyHandle {
        let body = RigidBodyBuilder::dynamic()
            .translation(vector![position[0], position[1], position[2]])
            .build();
        let handle = self.bodies.insert(body);
        let collider = ColliderBuilder::cuboid(
            half_extents[0], half_extents[1], half_extents[2],
        ).restitution(0.3).build();
        self.colliders.insert_with_parent(collider, handle, &mut self.bodies);
        handle
    }

    /// Caja estática (no se mueve, bloquea otros cuerpos).
    pub(crate) fn add_static_box(
        &mut self,
        position:     [f32; 3],
        half_extents: [f32; 3],
    ) -> RigidBodyHandle {
        let body = RigidBodyBuilder::fixed()
            .translation(vector![position[0], position[1], position[2]])
            .build();
        let handle = self.bodies.insert(body);
        let collider = ColliderBuilder::cuboid(
            half_extents[0], half_extents[1], half_extents[2],
        ).build();
        self.colliders.insert_with_parent(collider, handle, &mut self.bodies);
        handle
    }

    /// Caja cinemática (se mueve por código, no por gravedad).
    pub(crate) fn add_kinematic_box(
        &mut self,
        position:     [f32; 3],
        half_extents: [f32; 3],
    ) -> RigidBodyHandle {
        let body = RigidBodyBuilder::kinematic_position_based()
            .translation(vector![position[0], position[1], position[2]])
            .build();
        let handle = self.bodies.insert(body);
        let collider = ColliderBuilder::cuboid(
            half_extents[0], half_extents[1], half_extents[2],
        ).build();
        self.colliders.insert_with_parent(collider, handle, &mut self.bodies);
        handle
    }

    // ── Gestión de física por entidad ─────────────────────────────────────────

    /// Activa o desactiva física en una entidad. Si ya tiene cuerpo, lo reemplaza.
    pub(crate) fn set_entity_physics(
        &mut self,
        entity:    EntityId,
        enabled:   bool,
        body_type: &str,
        position:  [f32; 3],
        half_ext:  [f32; 3],
    ) {
        // Eliminar cuerpo previo si existe
        if let Some(handle) = self.entity_bodies.remove(&entity) {
            self.entity_body_types.remove(&entity);
            self.remove_body(handle);
        }
        if !enabled { return; }

        let half = [
            half_ext[0].max(0.01),
            half_ext[1].max(0.01),
            half_ext[2].max(0.01),
        ];
        let handle = match body_type {
            "static"    => self.add_static_box(position, half),
            "kinematic" => self.add_kinematic_box(position, half),
            _           => self.add_dynamic_box(position, half),
        };
        self.entity_bodies.insert(entity, handle);
        self.entity_body_types.insert(entity, body_type.to_string());
    }

    /// Elimina el cuerpo físico de una entidad si tiene uno.
    pub(crate) fn remove_entity_body(&mut self, entity: EntityId) {
        if let Some(handle) = self.entity_bodies.remove(&entity) {
            self.entity_body_types.remove(&entity);
            self.remove_body(handle);
        }
    }

    /// Devuelve true si la entidad tiene física activa.
    pub(crate) fn has_physics(&self, entity: EntityId) -> bool {
        self.entity_bodies.contains_key(&entity)
    }

    /// Devuelve el tipo de cuerpo de la entidad ("dynamic" | "static" | "kinematic" | "").
    pub(crate) fn get_body_type(&self, entity: EntityId) -> &str {
        self.entity_body_types.get(&entity).map(|s| s.as_str()).unwrap_or("")
    }

    // ── Acceso directo ───────────────────────────────────────────────────────

    pub(crate) fn body_mut(&mut self, handle: RigidBodyHandle) -> Option<&mut RigidBody> {
        self.bodies.get_mut(handle)
    }

    pub(crate) fn remove_body(&mut self, handle: RigidBodyHandle) {
        self.bodies.remove(
            handle,
            &mut self.island_manager,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            true,
        );
    }

    // ── Paso de simulación ───────────────────────────────────────────────────

    /// Avanza la simulación `dt` segundos y escribe posiciones de vuelta en el ECS.
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

        // Sincronizar posiciones de vuelta al ECS (solo cuerpos dinámicos se mueven)
        let pairs: Vec<(EntityId, RigidBodyHandle)> =
            self.entity_bodies.iter().map(|(&e, &h)| (e, h)).collect();
        for (entity, handle) in pairs {
            if let Some(body) = self.bodies.get(handle) {
                if body.is_dynamic() {
                    let t = body.translation();
                    let r = body.rotation();
                    if let Some(transform) = ecs.get_mut::<Transform>(entity) {
                        transform.position = glam::Vec3::new(t.x, t.y, t.z);
                        transform.rotation = glam::Quat::from_xyzw(r.i, r.j, r.k, r.w);
                    }
                }
            }
        }
    }
}
