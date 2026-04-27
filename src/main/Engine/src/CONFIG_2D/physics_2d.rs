// ── Física 2D — simulación en plano XY usando Rapier3D ───────────────────────
//
// Rapier3D se usa con los ejes Z de traslación y rotación en X/Y bloqueados,
// lo que equivale a un mundo físico 2D. Gravedad en -Y (9.81 u/s²).
//
// Tipos de cuerpo soportados:
//   "dynamic"   — afectado por gravedad y colisiones.
//   "static"    — no se mueve (suelo, plataformas).

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
        }
    }
}

impl PhysicsWorld2D {
    pub(crate) fn new() -> Self { Self::default() }

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
    ) {
        // Eliminar cuerpo previo si existe
        if let Some(handle) = self.entity_bodies.remove(&entity) {
            self.entity_body_types.remove(&entity);
            self.remove_body(handle);
        }
        if !enabled { return; }

        let hx = half_ext[0].max(0.01);
        let hy = half_ext[1].max(0.01);

        let handle = match body_type {
            "static" => {
                let body = RigidBodyBuilder::fixed()
                    .translation(vector![position[0], position[1], 0.0])
                    .build();
                let handle = self.bodies.insert(body);
                let col = ColliderBuilder::cuboid(hx, hy, 0.01).build();
                self.colliders.insert_with_parent(col, handle, &mut self.bodies);
                handle
            }
            _ => {
                // "dynamic" — bloqueamos Z y rotaciones X/Y para comportamiento 2D puro
                let body = RigidBodyBuilder::dynamic()
                    .translation(vector![position[0], position[1], 0.0])
                    .locked_axes(
                        LockedAxes::TRANSLATION_LOCKED_Z
                        | LockedAxes::ROTATION_LOCKED_X
                        | LockedAxes::ROTATION_LOCKED_Y,
                    )
                    .build();
                let handle = self.bodies.insert(body);
                let col = ColliderBuilder::cuboid(hx, hy, 0.01)
                    .restitution(0.0)
                    .friction(0.5)
                    .build();
                self.colliders.insert_with_parent(col, handle, &mut self.bodies);
                handle
            }
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
    }

    pub(crate) fn has_physics(&self, entity: EntityId) -> bool {
        self.entity_bodies.contains_key(&entity)
    }

    pub(crate) fn get_body_type(&self, entity: EntityId) -> &str {
        self.entity_body_types.get(&entity).map(|s| s.as_str()).unwrap_or("")
    }

    /// Crea un colisionador estático de forma convexa a partir de puntos en espacio de mundo (XY).
    /// Usado por la herramienta de dibujo para crear colisionadores de forma arbitraria.
    #[allow(dead_code)]
    pub(crate) fn add_convex_collider(&mut self, entity: EntityId, points: &[[f32; 2]]) {
        if let Some(handle) = self.entity_bodies.remove(&entity) {
            self.entity_body_types.remove(&entity);
            self.remove_body(handle);
        }
        if points.len() < 3 {
            log::warn!("[physics_2d] add_convex_collider: se necesitan al menos 3 puntos (se recibieron {})", points.len());
            return;
        }

        let rapier_pts: Vec<Point<f32>> = points.iter()
            .map(|p| point![p[0], p[1], 0.0])
            .collect();

        let shape = match SharedShape::convex_hull(&rapier_pts) {
            Some(s) => s,
            None => {
                log::warn!("[physics_2d] no se pudo construir hull convexo para entidad {}", entity);
                return;
            }
        };

        let cx = points.iter().map(|p| p[0]).sum::<f32>() / points.len() as f32;
        let cy = points.iter().map(|p| p[1]).sum::<f32>() / points.len() as f32;

        let body  = RigidBodyBuilder::fixed().translation(vector![cx, cy, 0.0]).build();
        let handle = self.bodies.insert(body);
        // The shape from convex_hull is in world space but we placed body at centroid,
        // so shift the shape to centroid-relative by using a zero-offset collider
        // (rapier hull points are absolute when body is at world origin; here body is
        // at centroid so we must use centroid-relative points).
        let rel_pts: Vec<Point<f32>> = points.iter()
            .map(|p| point![p[0] - cx, p[1] - cy, 0.0])
            .collect();
        let rel_shape = match SharedShape::convex_hull(&rel_pts) {
            Some(s) => s,
            None => shape,
        };
        let col = ColliderBuilder::new(rel_shape).build();
        self.colliders.insert_with_parent(col, handle, &mut self.bodies);

        self.entity_bodies.insert(entity, handle);
        self.entity_body_types.insert(entity, "static".to_string());
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
