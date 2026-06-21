// ── Física 3D — integración con Rapier3D ─────────────────────────────────────

use std::collections::{HashMap, HashSet};

use glam::Vec3;
use rapier3d::na::Unit;
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::*;

use rer_engine_shared::DEFAULT_GRAVITY_MAGNITUDE;

use crate::config_3d::WorldBounds3D;
use crate::ecs::{EntityId, Transform, World};

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct RigidBodyComponent {
    pub(crate) handle: RigidBodyHandle,
}

pub(crate) struct PhysicsWorld {
    gravity: Vector,
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
    entity_bodies: HashMap<EntityId, RigidBodyHandle>,
    entity_body_types: HashMap<EntityId, String>,
    entity_colliders: HashMap<EntityId, ColliderHandle>,
    world_bounds_colliders: Vec<ColliderHandle>,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self {
            gravity: Vector::new(0.0, -DEFAULT_GRAVITY_MAGNITUDE, 0.0),
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
            entity_bodies: HashMap::new(),
            entity_body_types: HashMap::new(),
            entity_colliders: HashMap::new(),
            world_bounds_colliders: Vec::new(),
        }
    }
}

/// Centro del collider cuando `transform.position` está en los pies (mallas FBX con pivote en suelo).
pub(crate) fn physics_center_from_feet_position(feet: [f32; 3], half: [f32; 3]) -> [f32; 3] {
    [feet[0], feet[1] + half[1], feet[2]]
}

impl PhysicsWorld {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn query_pipeline<'a>(&'a self, filter: QueryFilter<'a>) -> QueryPipeline<'a> {
        self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            filter,
        )
    }

    pub(crate) fn body_count(&self) -> u32 {
        self.bodies.len() as u32
    }

    /// Gravedad positiva hacia abajo (m/s²), p. ej. 9.81 en la Tierra.
    pub(crate) fn gravity_magnitude(&self) -> f32 {
        self.gravity.y.abs()
    }

    pub(crate) fn set_gravity(&mut self, gravity_y: f32) {
        self.gravity = Vector::new(0.0, gravity_y, 0.0);
    }

    pub(crate) fn add_dynamic_sphere(
        &mut self,
        position: [f32; 3],
        radius: f32,
    ) -> (RigidBodyHandle, ColliderHandle) {
        let body = RigidBodyBuilder::dynamic()
            .translation(Vector::new(position[0], position[1], position[2]))
            .linear_damping(0.12)
            .angular_damping(0.18)
            .build();
        let handle = self.bodies.insert(body);
        let collider = ColliderBuilder::ball(radius.max(0.01))
            .restitution(0.5)
            .build();
        let collider_handle =
            self.colliders
                .insert_with_parent(collider, handle, &mut self.bodies);
        (handle, collider_handle)
    }

    pub(crate) fn add_static_sphere(
        &mut self,
        position: [f32; 3],
        radius: f32,
    ) -> (RigidBodyHandle, ColliderHandle) {
        let body = RigidBodyBuilder::fixed()
            .translation(Vector::new(position[0], position[1], position[2]))
            .build();
        let handle = self.bodies.insert(body);
        let collider = ColliderBuilder::ball(radius.max(0.01)).build();
        let collider_handle =
            self.colliders
                .insert_with_parent(collider, handle, &mut self.bodies);
        (handle, collider_handle)
    }

    pub(crate) fn add_kinematic_sphere(
        &mut self,
        position: [f32; 3],
        radius: f32,
    ) -> (RigidBodyHandle, ColliderHandle) {
        let body = RigidBodyBuilder::kinematic_position_based()
            .translation(Vector::new(position[0], position[1], position[2]))
            .build();
        let handle = self.bodies.insert(body);
        let collider = ColliderBuilder::ball(radius.max(0.01)).build();
        let collider_handle =
            self.colliders
                .insert_with_parent(collider, handle, &mut self.bodies);
        (handle, collider_handle)
    }

    pub(crate) fn set_entity_sphere_physics(
        &mut self,
        entity: EntityId,
        enabled: bool,
        body_type: &str,
        position: [f32; 3],
        radius: f32,
    ) {
        if let Some(handle) = self.entity_bodies.remove(&entity) {
            self.entity_body_types.remove(&entity);
            self.entity_colliders.remove(&entity);
            self.remove_body(handle);
        }
        if !enabled {
            return;
        }

        let radius = radius.max(0.01);
        let (handle, collider_handle) = match body_type {
            "static" => self.add_static_sphere(position, radius),
            "kinematic" => self.add_kinematic_sphere(position, radius),
            _ => self.add_dynamic_sphere(position, radius),
        };
        self.entity_bodies.insert(entity, handle);
        self.entity_colliders.insert(entity, collider_handle);
        self.entity_body_types.insert(entity, body_type.to_string());
    }

    #[allow(dead_code)]
    pub(crate) fn add_dynamic_sphere_legacy(
        &mut self,
        position: [f32; 3],
        radius: f32,
    ) -> RigidBodyHandle {
        self.add_dynamic_sphere(position, radius).0
    }

    pub(crate) fn add_static_ground(&mut self) -> ColliderHandle {
        let normal = Unit::new_unchecked(Vector::Y);
        let collider = ColliderBuilder::halfspace(normal).build();
        self.colliders.insert(collider)
    }

    pub(crate) fn add_static_oriented_box(
        &mut self,
        position: [f32; 3],
        rotation: Rotation,
        half_extents: [f32; 3],
    ) -> (RigidBodyHandle, ColliderHandle) {
        let pose = pose_from_parts(position, rotation);
        let body = RigidBodyBuilder::fixed().pose(pose).build();
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
        self.set_entity_physics_oriented(
            entity,
            enabled,
            body_type,
            position,
            glam::Quat::IDENTITY,
            half_ext,
        );
    }

    /// Caja con rotación (muros invisibles del editor: alineados al yaw del transform).
    pub(crate) fn set_entity_physics_oriented(
        &mut self,
        entity: EntityId,
        enabled: bool,
        body_type: &str,
        position: [f32; 3],
        rotation: glam::Quat,
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
        let rot = glam_quat_to_rotation(rotation);
        let pose = |pos: [f32; 3]| pose_from_parts(pos, rot);
        let (handle, collider_handle) = match body_type {
            "static" => self.add_static_oriented_box(position, rot, half),
            "kinematic" => {
                let body = RigidBodyBuilder::kinematic_position_based()
                    .pose(pose(position))
                    .build();
                let handle = self.bodies.insert(body);
                let collider = ColliderBuilder::cuboid(half[0], half[1], half[2]).build();
                let collider_handle =
                    self.colliders
                        .insert_with_parent(collider, handle, &mut self.bodies);
                (handle, collider_handle)
            }
            _ => {
                let body = RigidBodyBuilder::dynamic()
                    .pose(pose(position))
                    .build();
                let handle = self.bodies.insert(body);
                let collider = ColliderBuilder::cuboid(half[0], half[1], half[2])
                    .restitution(0.3)
                    .build();
                let collider_handle =
                    self.colliders
                        .insert_with_parent(collider, handle, &mut self.bodies);
                (handle, collider_handle)
            }
        };
        self.entity_bodies.insert(entity, handle);
        self.entity_colliders.insert(entity, collider_handle);
        self.entity_body_types.insert(entity, body_type.to_string());
    }

    pub(crate) fn remove_entity_body(&mut self, entity: EntityId) {
        if let Some(handle) = self.entity_bodies.remove(&entity) {
            self.entity_body_types.remove(&entity);
            self.entity_colliders.remove(&entity);
            self.remove_body(handle);
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

    pub(crate) fn collider_handle_for_entity(&self, entity: EntityId) -> Option<ColliderHandle> {
        self.entity_colliders.get(&entity).copied()
    }

    pub(crate) fn entity_collider_entries(&self) -> impl Iterator<Item = (EntityId, ColliderHandle)> + '_ {
        self.entity_colliders
            .iter()
            .map(|(&id, &h)| (id, h))
    }

    /// AABB mundial del collider Rapier (wireframe de debug en editor).
    pub(crate) fn collider_world_aabb(
        &self,
        handle: ColliderHandle,
    ) -> Option<(glam::Vec3, glam::Vec3)> {
        let collider = self.colliders.get(handle)?;
        let aabb = collider.compute_aabb();
        let c = aabb.center();
        let h = aabb.half_extents();
        Some((
            glam::Vec3::new(c.x, c.y, c.z),
            glam::Vec3::new(h.x.max(0.01), h.y.max(0.01), h.z.max(0.01)),
        ))
    }

    /// El jugador FP usa shape-cast (no cuerpo Rapier): empuja dinámicos al contactar.
    fn apply_character_push_to_dynamic_hit(
        &mut self,
        hit_handle: ColliderHandle,
        movement: Vec3,
    ) {
        let body_handle = self
            .colliders
            .get(hit_handle)
            .and_then(|collider| collider.parent());
        let Some(body_handle) = body_handle else {
            return;
        };
        let Some(body) = self.bodies.get_mut(body_handle) else {
            return;
        };
        if !body.is_dynamic() {
            return;
        }
        let horizontal = Vec3::new(movement.x, 0.0, movement.z);
        let speed = horizontal.length();
        if speed <= 1e-4 {
            return;
        }
        let dir = horizontal / speed;
        let mass = body.mass().max(0.05);
        let impulse_strength = (speed * mass * 10.0).clamp(0.5, 140.0);
        body.apply_impulse(
            Vector::new(dir.x * impulse_strength, 0.0, dir.z * impulse_strength),
            true,
        );
        body.wake_up(true);
    }

    /// Distancia al primer obstáculo a lo largo de un rayo (ignora colliders en `exclude_colliders`).
    pub(crate) fn raycast_first_hit_distance(
        &mut self,
        from: Vec3,
        direction: Vec3,
        max_dist: f32,
        exclude_colliders: &[ColliderHandle],
    ) -> Option<f32> {
        if max_dist <= 1e-6 {
            return None;
        }
        let dir = direction.normalize_or_zero();
        if dir.length_squared() < 1e-8 {
            return None;
        }
        let excluded: HashSet<ColliderHandle> = exclude_colliders.iter().copied().collect();
        let filter = if excluded.is_empty() {
            QueryFilter::default()
        } else if excluded.len() == 1 {
            QueryFilter::default().exclude_collider(*excluded.iter().next().unwrap())
        } else {
            QueryFilter {
                predicate: Some(&|handle, _| !excluded.contains(&handle)),
                ..QueryFilter::default()
            }
        };
        let ray = Ray::new(
            Vector::new(from.x, from.y, from.z),
            Vector::new(dir.x, dir.y, dir.z),
        );
        self.query_pipeline(filter)
            .cast_ray_and_get_normal(&ray, max_dist, true)
            .map(|(_, hit)| hit.time_of_impact)
    }

    /// Altura del segmento cilíndrico de la cápsula (total ≈ scale_y con hemisferios).
    pub(crate) fn capsule_half_height_from_scale(scale_y: f32, radius: f32) -> f32 {
        (scale_y * 0.5 - radius).max(0.01)
    }

    pub(crate) fn rebuild_world_bounds_colliders(&mut self, bounds: &WorldBounds3D) {
        const WALL_THICKNESS: f32 = 0.5;

        let handles = std::mem::take(&mut self.world_bounds_colliders);
        self.clear_collider_handles(handles);

        let hx = (bounds.width * 0.5).max(0.5);
        let hy = (bounds.height * 0.5).max(0.5);
        let hz = (bounds.depth * 0.5).max(0.5);

        let floor = ColliderBuilder::cuboid(hx, WALL_THICKNESS, hz)
            .translation(Vector::new(0.0, -WALL_THICKNESS, 0.0))
            .build();
        self.world_bounds_colliders.push(self.colliders.insert(floor));

        let left_wall = ColliderBuilder::cuboid(WALL_THICKNESS, hy, hz + WALL_THICKNESS)
            .translation(Vector::new(-hx - WALL_THICKNESS, hy, 0.0))
            .build();
        self.world_bounds_colliders
            .push(self.colliders.insert(left_wall));

        let right_wall = ColliderBuilder::cuboid(WALL_THICKNESS, hy, hz + WALL_THICKNESS)
            .translation(Vector::new(hx + WALL_THICKNESS, hy, 0.0))
            .build();
        self.world_bounds_colliders
            .push(self.colliders.insert(right_wall));

        let back_wall = ColliderBuilder::cuboid(hx + WALL_THICKNESS, hy, WALL_THICKNESS)
            .translation(Vector::new(0.0, hy, -hz - WALL_THICKNESS))
            .build();
        self.world_bounds_colliders
            .push(self.colliders.insert(back_wall));

        let front_wall = ColliderBuilder::cuboid(hx + WALL_THICKNESS, hy, WALL_THICKNESS)
            .translation(Vector::new(0.0, hy, hz + WALL_THICKNESS))
            .build();
        self.world_bounds_colliders
            .push(self.colliders.insert(front_wall));
    }

    /// Raycast vertical para colocar pies al spawn (solo contacto, sin Y fijo).
    pub(crate) fn find_ground_y_at(&mut self, x: f32, z: f32, from_y: f32, max_down: f32) -> Option<f32> {
        let filter = QueryFilter::default();
        let ray = Ray::new(Vector::new(x, from_y, z), Vector::new(0.0, -1.0, 0.0));
        let hit = self
            .query_pipeline(filter)
            .cast_ray_and_get_normal(&ray, max_down.max(0.1), true);
        if let Some((_, hit)) = hit {
            if hit.normal.y > 0.45 {
                return Some(from_y - hit.time_of_impact + 0.02);
            }
        }
        None
    }

    /// Sondea el suelo bajo los pies con un raycast vertical. Devuelve `Some(ground_y)`
    /// si hay suelo cuya normal cumple `floor_max_angle ≈ 45°` (estilo Godot).
    /// El caller decide si hacer "snap" de los pies a `ground_y`.
    pub(crate) fn floor_probe(
        &mut self,
        feet: Vec3,
        exclude_colliders: &[ColliderHandle],
    ) -> Option<f32> {
        let excluded: HashSet<ColliderHandle> = exclude_colliders.iter().copied().collect();
        let filter = if excluded.is_empty() {
            QueryFilter::default()
        } else if excluded.len() == 1 {
            QueryFilter::default().exclude_collider(*excluded.iter().next().unwrap())
        } else {
            QueryFilter {
                predicate: Some(&|handle, _| !excluded.contains(&handle)),
                ..QueryFilter::default()
            }
        };
        const SKIN: f32 = 0.02;
        const PROBE: f32 = 0.10;
        let origin = Vector::new(feet.x, feet.y + SKIN, feet.z);
        let dir = Vector::new(0.0, -1.0, 0.0);
        let ray = Ray::new(origin, dir);

        if let Some((_, hit)) = self
            .query_pipeline(filter)
            .cast_ray_and_get_normal(&ray, SKIN + PROBE, true)
        {
            if hit.normal.y > 0.707 {
                let ground_y = feet.y + SKIN - hit.time_of_impact;
                return Some(ground_y);
            }
        }
        None
    }

    /// `move_and_slide` con cápsula anclada en los pies (CharacterBody3D-style).
    pub(crate) fn move_character_capsule_at_feet(
        &mut self,
        feet: Vec3,
        up: Vec3,
        velocity: Vec3,
        dt: f32,
        radius: f32,
        half_height: f32,
        _ground_probe: f32,
        exclude_colliders: &[ColliderHandle],
    ) -> (Vec3, bool) {
        let up = up.normalize_or_zero();
        let radius = radius.max(0.05);
        let half_height = half_height.max(0.01);
        let mut feet_pos = feet;

        let horizontal = Vec3::new(velocity.x, 0.0, velocity.z) * dt;
        if horizontal.length_squared() > 1e-6 {
            feet_pos = self.move_capsule_at_feet(
                feet_pos,
                up,
                horizontal,
                radius,
                half_height,
                exclude_colliders,
            );
        }

        let vertical_delta = velocity.y * dt;
        if vertical_delta.abs() > 1e-6 {
            let vertical_motion = Vec3::Y * vertical_delta;
            feet_pos = self.move_capsule_at_feet(
                feet_pos,
                up,
                vertical_motion,
                radius,
                half_height,
                exclude_colliders,
            );
        }

        let on_floor = if velocity.y > 0.0 {
            false
        } else {
            match self.floor_probe(feet_pos, exclude_colliders) {
                Some(ground_y) => {
                    feet_pos.y = ground_y;
                    true
                }
                None => false,
            }
        };

        (feet_pos, on_floor)
    }

    fn move_capsule_at_feet(
        &mut self,
        feet: Vec3,
        up: Vec3,
        movement: Vec3,
        radius: f32,
        half_height: f32,
        exclude_colliders: &[ColliderHandle],
    ) -> Vec3 {
        if movement.length_squared() <= 1e-6 {
            return feet;
        }

        let excluded: HashSet<ColliderHandle> = exclude_colliders.iter().copied().collect();
        let filter = if excluded.is_empty() {
            QueryFilter::default()
        } else if excluded.len() == 1 {
            QueryFilter::default().exclude_collider(*excluded.iter().next().unwrap())
        } else {
            QueryFilter {
                predicate: Some(&|handle, _| !excluded.contains(&handle)),
                ..QueryFilter::default()
            }
        };

        let capsule = Capsule::new_y(half_height, radius);
        let up = up.normalize_or_zero();
        let mut current_feet = feet;
        let mut remaining = movement;

        for _ in 0..3 {
            if remaining.length_squared() <= 1e-6 {
                break;
            }

            let pose = capsule_pose_at_feet(current_feet, up, radius, half_height);
            let falling = remaining.y < -1e-6;
            let queries = self.query_pipeline(filter);
            let hit = queries.cast_shape(
                &pose,
                Vector::new(remaining.x, remaining.y, remaining.z),
                &capsule,
                ShapeCastOptions {
                    max_time_of_impact: 1.0,
                    target_distance: 0.0,
                    stop_at_penetration: falling,
                    compute_impact_geometry_on_penetration: falling,
                },
            );

            let Some((hit_handle, hit_data)) = hit else {
                current_feet += remaining;
                break;
            };

            self.apply_character_push_to_dynamic_hit(hit_handle, remaining);

            let toi = hit_data.time_of_impact.clamp(0.0, 1.0);
            let center = capsule_center_from_feet(current_feet, up, radius, half_height);
            let new_center = center + remaining * toi;
            current_feet = feet_from_capsule_center(new_center, up, radius, half_height);

            if falling {
                let n = hit_data.normal2;
                if n.y > 0.45 {
                    break;
                }
            }

            let normal = hit_data.normal2;
            let slide_normal = Vec3::new(normal.x, normal.y, normal.z).normalize_or_zero();
            let slide = remaining - slide_normal * remaining.dot(slide_normal);
            let remainder_factor = (1.0 - toi).max(0.0);
            if slide.length_squared() <= 1e-6 || remainder_factor <= 1e-3 {
                break;
            }

            remaining = slide * remainder_factor;
        }

        current_feet
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
            self.gravity,
            &self.integration_params,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
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
                        transform.rotation = glam::Quat::from_xyzw(r.x, r.y, r.z, r.w);
                    }
                }
            }
        }
    }
}

fn pose_from_parts(position: [f32; 3], rotation: Rotation) -> Pose {
    Pose::from_parts(
        Vector::new(position[0], position[1], position[2]),
        rotation,
    )
}

fn glam_quat_to_rotation(q: glam::Quat) -> Rotation {
    q
}

fn capsule_center_from_feet(feet: Vec3, up: Vec3, radius: f32, half_height: f32) -> Vec3 {
    feet + up.normalize_or_zero() * (half_height + radius)
}

fn feet_from_capsule_center(center: Vec3, up: Vec3, radius: f32, half_height: f32) -> Vec3 {
    center - up.normalize_or_zero() * (half_height + radius)
}

fn capsule_pose_at_feet(
    feet: Vec3,
    up: Vec3,
    radius: f32,
    half_height: f32,
) -> Pose {
    let up = up.normalize_or_zero();
    let center = capsule_center_from_feet(feet, up, radius, half_height);
    let rot = Rotation::from_rotation_arc(Vector::Y, up);
    Pose::from_parts(center, rot)
}
