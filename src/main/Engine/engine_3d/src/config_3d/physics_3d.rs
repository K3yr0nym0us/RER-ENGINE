// ── Física 3D — integración con Rapier3D ─────────────────────────────────────

use std::collections::HashMap;

use glam::Vec3;
use rapier3d::prelude::*;
use rapier3d::parry::query::ShapeCastOptions;

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
    scene_colliders: Vec<ColliderHandle>,
    world_bounds_colliders: Vec<ColliderHandle>,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self {
            gravity: vector![0.0, -9.81, 0.0],
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
            scene_colliders: Vec::new(),
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

    #[allow(dead_code)]
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

    pub(crate) fn add_scene_static_box(
        &mut self,
        position: [f32; 3],
        half_extents: [f32; 3],
    ) -> ColliderHandle {
        let collider = ColliderBuilder::cuboid(
            half_extents[0].max(0.01),
            half_extents[1].max(0.01),
            half_extents[2].max(0.01),
        )
        .translation(vector![position[0], position[1], position[2]])
        .build();
        let handle = self.colliders.insert(collider);
        self.scene_colliders.push(handle);
        self.refresh_queries();
        handle
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

    pub(crate) fn set_entity_body_position(&mut self, entity: EntityId, position: [f32; 3]) {
        let Some(&handle) = self.entity_bodies.get(&entity) else {
            return;
        };
        if let Some(body) = self.bodies.get_mut(handle) {
            body.set_translation(vector![position[0], position[1], position[2]], true);
        }
        self.refresh_queries();
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

    pub(crate) fn clear_scene_colliders(&mut self) {
        let handles = std::mem::take(&mut self.scene_colliders);
        self.clear_collider_handles(handles);
        self.refresh_queries();
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

    pub(crate) fn step(&mut self, dt: f32, ecs: &mut World) {
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

        self.refresh_queries();
    }
}
