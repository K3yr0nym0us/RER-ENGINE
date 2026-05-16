// ── Física 3D — integración con Rapier3D ─────────────────────────────────────

use std::collections::HashMap;

use glam::Vec3;
use rapier3d::prelude::*;
use rapier3d::parry::query::ShapeCastOptions;

use rer_engine_shared::DEFAULT_GRAVITY_MAGNITUDE;

use crate::config_3d::WorldBounds3D;
use crate::ecs::{EntityId, Transform, World};

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct RigidBodyComponent {
    pub(crate) handle: RigidBodyHandle,
}

pub(crate) struct PhysicsWorld {
    gravity: Vector<f32>,
    integration_params: IntegrationParameters,
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
    query_pipeline: QueryPipeline,
    entity_bodies: HashMap<EntityId, RigidBodyHandle>,
    entity_body_types: HashMap<EntityId, String>,
    entity_colliders: HashMap<EntityId, ColliderHandle>,
    world_bounds_colliders: Vec<ColliderHandle>,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self {
            gravity: vector![0.0, -DEFAULT_GRAVITY_MAGNITUDE, 0.0],
            integration_params: IntegrationParameters::default(),
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            query_pipeline: QueryPipeline::new(),
            entity_bodies: HashMap::new(),
            entity_body_types: HashMap::new(),
            entity_colliders: HashMap::new(),
            world_bounds_colliders: Vec::new(),
        }
    }
}

impl PhysicsWorld {
    fn refresh_queries(&mut self) {
        self.query_pipeline.update(&self.colliders);
    }

    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn body_count(&self) -> u32 {
        self.bodies.len() as u32
    }

    /// Gravedad positiva hacia abajo (m/s²), p. ej. 9.81 en la Tierra.
    pub(crate) fn gravity_magnitude(&self) -> f32 {
        self.gravity.y.abs()
    }

    pub(crate) fn set_gravity(&mut self, gravity_y: f32) {
        self.gravity = vector![0.0, gravity_y, 0.0];
    }

    #[allow(dead_code)]
    pub(crate) fn add_dynamic_sphere(
        &mut self,
        position: [f32; 3],
        radius: f32,
    ) -> RigidBodyHandle {
        let body = RigidBodyBuilder::dynamic()
            .translation(vector![position[0], position[1], position[2]])
            .build();
        let handle = self.bodies.insert(body);
        let collider = ColliderBuilder::ball(radius).restitution(0.5).build();
        self.colliders.insert_with_parent(collider, handle, &mut self.bodies);
        handle
    }

    pub(crate) fn add_static_ground(&mut self) -> ColliderHandle {
        let collider =
            ColliderBuilder::halfspace(UnitVector::new_normalize(vector![0.0, 1.0, 0.0])).build();
        self.colliders.insert(collider)
    }

    pub(crate) fn add_dynamic_box(
        &mut self,
        position: [f32; 3],
        half_extents: [f32; 3],
    ) -> (RigidBodyHandle, ColliderHandle) {
        let body = RigidBodyBuilder::dynamic()
            .translation(vector![position[0], position[1], position[2]])
            .build();
        let handle = self.bodies.insert(body);
        let collider = ColliderBuilder::cuboid(half_extents[0], half_extents[1], half_extents[2])
            .restitution(0.3)
            .build();
        let collider_handle =
            self.colliders
                .insert_with_parent(collider, handle, &mut self.bodies);
        (handle, collider_handle)
    }

    pub(crate) fn add_static_box(
        &mut self,
        position: [f32; 3],
        half_extents: [f32; 3],
    ) -> (RigidBodyHandle, ColliderHandle) {
        let body = RigidBodyBuilder::fixed()
            .translation(vector![position[0], position[1], position[2]])
            .build();
        let handle = self.bodies.insert(body);
        let collider = ColliderBuilder::cuboid(half_extents[0], half_extents[1], half_extents[2])
            .build();
        let collider_handle =
            self.colliders
                .insert_with_parent(collider, handle, &mut self.bodies);
        (handle, collider_handle)
    }

    pub(crate) fn add_kinematic_box(
        &mut self,
        position: [f32; 3],
        half_extents: [f32; 3],
    ) -> (RigidBodyHandle, ColliderHandle) {
        let body = RigidBodyBuilder::kinematic_position_based()
            .translation(vector![position[0], position[1], position[2]])
            .build();
        let handle = self.bodies.insert(body);
        let collider = ColliderBuilder::cuboid(half_extents[0], half_extents[1], half_extents[2])
            .build();
        let collider_handle =
            self.colliders
                .insert_with_parent(collider, handle, &mut self.bodies);
        (handle, collider_handle)
    }

    pub(crate) fn set_entity_physics(
        &mut self,
        entity: EntityId,
        enabled: bool,
        body_type: &str,
        position: [f32; 3],
        half_ext: [f32; 3],
    ) {
        if let Some(handle) = self.entity_bodies.remove(&entity) {
            self.entity_body_types.remove(&entity);
            self.entity_colliders.remove(&entity);
            self.remove_body(handle);
        }
        if !enabled {
            return;
        }

        let half = [
            half_ext[0].max(0.01),
            half_ext[1].max(0.01),
            half_ext[2].max(0.01),
        ];
        let (handle, collider_handle) = match body_type {
            "static" => self.add_static_box(position, half),
            "kinematic" => self.add_kinematic_box(position, half),
            _ => self.add_dynamic_box(position, half),
        };
        self.entity_bodies.insert(entity, handle);
        self.entity_colliders.insert(entity, collider_handle);
        self.entity_body_types.insert(entity, body_type.to_string());
        self.refresh_queries();
    }

    pub(crate) fn remove_entity_body(&mut self, entity: EntityId) {
        if let Some(handle) = self.entity_bodies.remove(&entity) {
            self.entity_body_types.remove(&entity);
            self.entity_colliders.remove(&entity);
            self.remove_body(handle);
            self.refresh_queries();
        }
    }

    pub(crate) fn has_physics(&self, entity: EntityId) -> bool {
        self.entity_bodies.contains_key(&entity)
    }

    pub(crate) fn get_body_type(&self, entity: EntityId) -> &str {
        self.entity_body_types
            .get(&entity)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    #[allow(dead_code)]
    pub(crate) fn body_mut(&mut self, handle: RigidBodyHandle) -> Option<&mut RigidBody> {
        self.bodies.get_mut(handle)
    }

    /// Recrea el cuerpo/collider según el transform actual (editor: mover y escalar).
    pub(crate) fn sync_entity_physics_from_transform(
        &mut self,
        entity: EntityId,
        position: [f32; 3],
        half_extents: [f32; 3],
    ) {
        if !self.entity_bodies.contains_key(&entity) {
            return;
        }
        let body_type = self
            .entity_body_types
            .get(&entity)
            .cloned()
            .unwrap_or_else(|| "static".to_string());
        self.set_entity_physics(entity, true, &body_type, position, half_extents);
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

    fn clear_collider_handles(&mut self, handles: Vec<ColliderHandle>) {
        for handle in handles {
            self.colliders
                .remove(handle, &mut self.island_manager, &mut self.bodies, true);
        }
    }

    pub(crate) fn rebuild_world_bounds_colliders(&mut self, bounds: &WorldBounds3D) {
        const WALL_THICKNESS: f32 = 0.5;

        let handles = std::mem::take(&mut self.world_bounds_colliders);
        self.clear_collider_handles(handles);

        let hx = (bounds.width * 0.5).max(0.5);
        let hy = (bounds.height * 0.5).max(0.5);
        let hz = (bounds.depth * 0.5).max(0.5);

        let floor = ColliderBuilder::cuboid(hx, WALL_THICKNESS, hz)
            .translation(vector![0.0, -WALL_THICKNESS, 0.0])
            .build();
        self.world_bounds_colliders.push(self.colliders.insert(floor));

        let left_wall = ColliderBuilder::cuboid(WALL_THICKNESS, hy, hz + WALL_THICKNESS)
            .translation(vector![-hx - WALL_THICKNESS, hy, 0.0])
            .build();
        self.world_bounds_colliders
            .push(self.colliders.insert(left_wall));

        let right_wall = ColliderBuilder::cuboid(WALL_THICKNESS, hy, hz + WALL_THICKNESS)
            .translation(vector![hx + WALL_THICKNESS, hy, 0.0])
            .build();
        self.world_bounds_colliders
            .push(self.colliders.insert(right_wall));

        let back_wall = ColliderBuilder::cuboid(hx + WALL_THICKNESS, hy, WALL_THICKNESS)
            .translation(vector![0.0, hy, -hz - WALL_THICKNESS])
            .build();
        self.world_bounds_colliders
            .push(self.colliders.insert(back_wall));

        let front_wall = ColliderBuilder::cuboid(hx + WALL_THICKNESS, hy, WALL_THICKNESS)
            .translation(vector![0.0, hy, hz + WALL_THICKNESS])
            .build();
        self.world_bounds_colliders
            .push(self.colliders.insert(front_wall));

        self.refresh_queries();
    }

    pub(crate) fn is_character_grounded(
        &mut self,
        position: Vec3,
        radius: f32,
        extra_distance: f32,
    ) -> bool {
        self.refresh_queries();

        let radius = radius.max(0.05);
        let feet_y = position.y - radius;
        let probe = extra_distance.max(0.05) + 0.15;
        let filter = QueryFilter::default();

        // Raycast desde los pies (Godot/Unity floor check) — detecta cajas y suelo.
        let ray_origin = point![position.x, feet_y + 0.04, position.z];
        let ray_dir = vector![0.0, -1.0, 0.0];
        let ray = Ray::new(ray_origin, ray_dir);
        if let Some((_, hit)) = self.query_pipeline.cast_ray_and_get_normal(
            &self.bodies,
            &self.colliders,
            &ray,
            probe,
            true,
            filter,
        ) {
            if hit.time_of_impact <= probe && hit.normal.y > 0.45 {
                return true;
            }
        }

        // Proyección bajo las suelas: superficie cercana en XZ (cajas, escalones).
        let sole = point![position.x, feet_y, position.z];
        if let Some((_, proj)) = self.query_pipeline.project_point(
            &self.bodies,
            &self.colliders,
            &sole,
            true,
            filter,
        ) {
            let gap_y = feet_y - proj.point.y;
            let horiz = ((position.x - proj.point.x).powi(2) + (position.z - proj.point.z).powi(2))
                .sqrt();
            if gap_y.abs() <= probe && gap_y >= -0.08 && horiz <= radius + 0.25 {
                return true;
            }
        }

        // Respaldo: shape-cast desde el centro del cuerpo.
        let shape = Ball::new(radius);
        let shape_pos = Isometry::translation(position.x, position.y, position.z);
        let shape_vel = vector![0.0, -(radius + probe), 0.0];
        if let Some((_, hit_data)) = self.query_pipeline.cast_shape(
            &self.bodies,
            &self.colliders,
            &shape_pos,
            &shape_vel,
            &shape,
            ShapeCastOptions {
                max_time_of_impact: 1.0,
                target_distance: 0.05,
                stop_at_penetration: true,
                compute_impact_geometry_on_penetration: true,
            },
            filter,
        ) {
            let normal = hit_data.normal2.into_inner();
            if normal.y > 0.45 {
                return true;
            }
        }

        false
    }

    /// Desplazamiento tipo `move_and_slide` (Godot) / `CharacterController.Move` (Unity):
    /// primero horizontal, luego vertical; devuelve posición final y si hay suelo bajo los pies.
    pub(crate) fn move_character_slide(
        &mut self,
        start: Vec3,
        velocity: Vec3,
        dt: f32,
        radius: f32,
        ground_probe: f32,
    ) -> (Vec3, bool) {
        let mut position = start;

        let horizontal = Vec3::new(velocity.x, 0.0, velocity.z) * dt;
        if horizontal.length_squared() > 1e-6 {
            position = self.move_character_sphere(position, horizontal, radius);
        }

        let vertical_delta = velocity.y * dt;
        if vertical_delta.abs() > 1e-6 {
            let before_y = position.y;
            let vertical_motion = Vec3::Y * vertical_delta;
            position = self.move_character_sphere(position, vertical_motion, radius);
            // Si el shape-cast bloquea el salto por contacto con el suelo, forzar subida mínima.
            if velocity.y > 0.0 && (position.y - before_y) < vertical_delta * 0.25 {
                position.y = before_y + vertical_delta;
            }
        }

        let on_floor = if ground_probe > 0.0 {
            self.is_character_grounded(position, radius, ground_probe)
        } else {
            false
        };
        (position, on_floor)
    }

    pub(crate) fn move_character_sphere(
        &mut self,
        start: Vec3,
        movement: Vec3,
        radius: f32,
    ) -> Vec3 {
        if movement.length_squared() <= 1e-6 {
            return start;
        }

        self.refresh_queries();

        let shape = Ball::new(radius.max(0.05));
        let mut current = start;
        let mut remaining = movement;

        for _ in 0..3 {
            if remaining.length_squared() <= 1e-6 {
                break;
            }

            let shape_pos = Isometry::translation(current.x, current.y, current.z);
            let shape_vel = vector![remaining.x, remaining.y, remaining.z];
            let hit = self.query_pipeline.cast_shape(
                &self.bodies,
                &self.colliders,
                &shape_pos,
                &shape_vel,
                &shape,
                ShapeCastOptions {
                    max_time_of_impact: 1.0,
                    target_distance: 0.02,
                    stop_at_penetration: false,
                    compute_impact_geometry_on_penetration: false,
                },
                QueryFilter::default(),
            );

            let Some((_, hit_data)) = hit else {
                current += remaining;
                break;
            };

            let toi = hit_data.time_of_impact.clamp(0.0, 1.0);
            current += remaining * toi;

            let normal = hit_data.normal2.into_inner();
            let slide_normal = Vec3::new(normal.x, normal.y, normal.z).normalize_or_zero();
            let slide = remaining - slide_normal * remaining.dot(slide_normal);
            let remainder_factor = (1.0 - toi).max(0.0);
            if slide.length_squared() <= 1e-6 || remainder_factor <= 1e-3 {
                break;
            }

            remaining = slide * remainder_factor;
        }

        current
    }

    pub(crate) fn step(
        &mut self,
        dt: f32,
        ecs: &mut World,
        skip_ecs_sync: &[EntityId],
    ) {
        if self.entity_bodies.is_empty() {
            return;
        }

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

        let pairs: Vec<(EntityId, RigidBodyHandle)> =
            self.entity_bodies.iter().map(|(&e, &h)| (e, h)).collect();
        for (entity, handle) in pairs {
            if skip_ecs_sync.contains(&entity) {
                continue;
            }
            if let Some(body) = self.bodies.get(handle) {
                if body.is_dynamic() {
                    let t = body.translation();
                    let r = body.rotation();
                    if let Some(transform) = ecs.get_mut::<Transform>(entity) {
                        transform.position = glam::Vec3::new(t.x, t.y, t.z);
                        transform.rotation =
                            glam::Quat::from_xyzw(r.i, r.j, r.k, r.w);
                    }
                }
            }
        }

        self.refresh_queries();
    }
}
