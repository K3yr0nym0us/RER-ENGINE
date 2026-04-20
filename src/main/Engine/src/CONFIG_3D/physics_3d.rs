// ── Física 3D — integración con Rapier3D ─────────────────────────────────────
//
// Solo activo en el modo 3D. Mantiene rigid bodies y colliders, avanza la
// simulación cada frame y sincroniza posiciones de vuelta al ECS.

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

        // Sincronizar posiciones de vuelta al ECS
        let entities: Vec<EntityId> = ecs.entities().to_vec();
        for id in entities {
            if let Some(rb_comp) = ecs.get::<RigidBodyComponent>(id).cloned() {
                if let Some(body) = self.bodies.get(rb_comp.handle) {
                    let t = body.translation();
                    let r = body.rotation();
                    if let Some(transform) = ecs.get_mut::<Transform>(id) {
                        transform.position = glam::Vec3::new(t.x, t.y, t.z);
                        transform.rotation = glam::Quat::from_xyzw(r.i, r.j, r.k, r.w);
                    }
                }
            }
        }
    }
}
