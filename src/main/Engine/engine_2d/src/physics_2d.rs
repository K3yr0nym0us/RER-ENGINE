// ── Física 2D — simulación en plano XY ───────────────────────────────────────
//
// Este binario implementa comportamiento 2D. El backend de física usa la crate
// `rapier3d` por compatibilidad técnica, pero con ejes bloqueados para mantener
// dinámica estrictamente en XY. Gravedad en -Y (9.81 u/s²).
//
// Tipos de cuerpo soportados:
//   "dynamic"   — afectado por gravedad y colisiones.
//   "static"    — no se mueve (suelo, plataformas).
//   "kinematic" — KinematicPositionBased + KinematicCharacterController de Rapier
//                 (similar a CharacterBody/move_and_slide de Godot: deslices iterados,
//                 snap al suelo). La entrada de velocidad viene de kinematic_actor_vel.
//
// Funciones extraídas a archivos propios (submódulos vía #[path]):
//   teleport_entity         — sincroniza Rapier body con el Transform.
//   move_physics_entity     — mueve con velocidad lineal respetando colisiones.
//   kinematic_gravity       — gravedad manual + colisiones para cuerpos kinematic.

#[path = "config_2d/physics_2d/teleport_entity.rs"]
mod teleport_entity;

#[path = "config_2d/physics_2d/move_physics_entity.rs"]
mod move_physics_entity;

#[path = "config_2d/physics_2d/kinematic_gravity.rs"]
mod kinematic_gravity;

use std::collections::{HashMap, HashSet};

use rapier3d::prelude::*;
use rer_engine_shared::DEFAULT_GRAVITY_MAGNITUDE;
use rapier3d::control::{CharacterLength, KinematicCharacterController};

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
    /// Set de entidades cuyo collider ya fue inicializado con la forma correcta.
    /// Las siguientes llamadas solo actualizan el offset (translation local) sin
    /// recrear el collider, preservando los contactos de Rapier.
    collider_shape_set: HashSet<EntityId>,
    pub(crate) debug_mode: bool,
    /// Velocidad planar (XY) pedida por scripts para KinematicPositionBased.
    /// Rapier no aplica `set_linvel` a ese tipo; `step()` la consume aquí.
    kinematic_actor_vel: HashMap<EntityId, Vector<f32>>,
    /// Dirección horizontal bloqueada por colisión en el último step kinematic.
    /// -1.0 = bloqueado hacia izquierda, 1.0 = bloqueado hacia derecha.
    blocked_horizontal_sign: HashMap<EntityId, f32>,
    /// Deslizamiento multi-paso + snap al suelo (API tipo Godot CharacterBody).
    kinematic_character: KinematicCharacterController,
}

fn default_kinematic_character() -> KinematicCharacterController {
    let mut cc = KinematicCharacterController::default();
    cc.snap_to_ground = Some(CharacterLength::Relative(0.14));
    cc.offset = CharacterLength::Relative(0.012);
    cc
}

impl Default for PhysicsWorld2D {
    fn default() -> Self {
        Self {
            gravity:            vector![0.0, -DEFAULT_GRAVITY_MAGNITUDE, 0.0],
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
            collider_shape_set: HashSet::new(),
            debug_mode:         false,
            kinematic_actor_vel: HashMap::new(),
            blocked_horizontal_sign: HashMap::new(),
            kinematic_character: default_kinematic_character(),
        }
    }
}

impl PhysicsWorld2D {
    pub(crate) fn new() -> Self { Self::default() }

    pub(crate) fn set_debug_mode(&mut self, on: bool) {
        self.debug_mode = on;
    }

    /// Número de rigid bodies activos en el mundo físico 2D.
    pub(crate) fn body_count(&self) -> u32 { self.bodies.len() as u32 }

    /// Gravedad positiva hacia abajo (m/s²).
    pub(crate) fn gravity_magnitude(&self) -> f32 {
        self.gravity.y.abs()
    }

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
            self.collider_shape_set.remove(&entity);
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
            "kinematic" => {
                // "kinematic" — position-based con gravedad manual en step().
                // La gravedad + shape cast + set_next_kinematic_position se
                // manejan en step() para detectar colisiones antes de mover.
                let body = RigidBodyBuilder::kinematic_position_based()
                    .translation(vector![position[0], position[1], 0.0])
                    .locked_axes(
                        LockedAxes::TRANSLATION_LOCKED_Z
                        | LockedAxes::ROTATION_LOCKED_X
                        | LockedAxes::ROTATION_LOCKED_Y,
                    )
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

    /// Permite recomponer la caja de colisión en el siguiente sync (p. ej. tras cambiar frame).
    pub(crate) fn clear_collider_shape(&mut self, entity: EntityId) {
        self.collider_shape_set.remove(&entity);
    }

    /// Sincroniza el offset local del collider preservando la forma ya creada.
    ///
    /// Contrato actual:
    /// - Primera llamada: crea el collider completo con la forma indicada.
    /// - Llamadas siguientes: solo actualizan el offset local.
    ///
    /// Esto conserva el comportamiento actual del motor y evita reiniciar
    /// contactos en Rapier, pero NO recompone la forma tras cambios de escala.
    pub(crate) fn sync_entity_collider_offset_preserving_shape(
        &mut self,
        entity:   EntityId,
        half_ext: [f32; 3],
        collider_offset: [f32; 3],
    ) {
        let Some(&body_handle) = self.entity_bodies.get(&entity) else { return };
        let hx = half_ext[0].max(0.01);
        let hy = half_ext[1].max(0.01);
        let offset = vector![collider_offset[0], collider_offset[1], collider_offset[2]];

        if !self.collider_shape_set.contains(&entity) {
            // Primera vez: crear collider completo (remove previo si existe + insert nuevo)
            if let Some(old_handle) = self.entity_colliders.remove(&entity) {
                self.colliders.remove(old_handle, &mut self.island_manager, &mut self.bodies, true);
            }
            let col = ColliderBuilder::cuboid(hx, hy, 0.01)
                .translation(offset)
                .restitution(0.0)
                .friction(0.5)
                .build();
            let col_handle = self.colliders.insert_with_parent(col, body_handle, &mut self.bodies);
            self.entity_colliders.insert(entity, col_handle);
            self.collider_shape_set.insert(entity);
            if self.debug_mode {
                log::debug!("[collider] entidad {entity} shape inicial: ({hx:.4},{hy:.4}) offset=({:.4},{:.4})", collider_offset[0], collider_offset[1]);
            }
        } else if let Some(&col_handle) = self.entity_colliders.get(&entity) {
            // Ya tiene collider — solo mover el offset sin tocar la forma.
            if let Some(collider) = self.colliders.get_mut(col_handle) {
                collider.set_translation(offset);
            }
        }
    }

    /// Wrapper legacy: mantener por compatibilidad interna mientras se migra el
    /// codigo a nombres mas explicitos. No recrea la forma en llamadas sucesivas.
    #[allow(dead_code)]
    pub(crate) fn update_entity_collider(
        &mut self,
        entity:   EntityId,
        half_ext: [f32; 3],
        collider_offset: [f32; 3],
    ) {
        self.sync_entity_collider_offset_preserving_shape(entity, half_ext, collider_offset);
    }
    pub(crate) fn remove_entity_body(&mut self, entity: EntityId) {
        if let Some(handle) = self.entity_bodies.remove(&entity) {
            self.entity_body_types.remove(&entity);
            self.entity_colliders.remove(&entity);
            self.collider_shape_set.remove(&entity);
            self.kinematic_actor_vel.remove(&entity);
            self.blocked_horizontal_sign.remove(&entity);
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
        self.collider_shape_set.clear();
        self.kinematic_actor_vel.clear();
        self.blocked_horizontal_sign.clear();
    }

    pub(crate) fn has_physics(&self, entity: EntityId) -> bool {
        self.entity_bodies.contains_key(&entity)
    }

    pub(crate) fn get_body_type(&self, entity: EntityId) -> &str {
        self.entity_body_types.get(&entity).map(|s| s.as_str()).unwrap_or("")
    }

    /// Registra velocidad planar para un actor kinematic (Rapier ignora `set_linvel` en KinematicPositionBased).
    pub(crate) fn set_kinematic_actor_vel_xy(&mut self, entity: EntityId, vel: Vector<f32>) {
        if self.get_body_type(entity) == "kinematic" {
            self.kinematic_actor_vel.insert(entity, vel);
        }
    }

    pub(crate) fn is_horizontal_blocked(&self, entity: EntityId, dir_x: f32) -> bool {
        const EPS: f32 = 1e-6;
        if dir_x.abs() <= EPS {
            return false;
        }
        let Some(sign) = self.blocked_horizontal_sign.get(&entity).copied() else {
            return false;
        };
        sign * dir_x > EPS
    }

    // ── Paso de simulación ────────────────────────────────────────────────────

    pub(crate) fn step(&mut self, dt: f32, ecs: &mut World) {
        if self.entity_bodies.is_empty() { return; }

        self.integration_params.dt = dt.clamp(0.0001, 0.05);

        // ── KinematicPositionBased + CharacterController (estilo Godot move_and_slide) ──
        // Rapier ignora `set_linvel`; la entrada de scripts va a `kinematic_actor_vel`.

        #[derive(Clone, Copy)]
        struct KData {
            entity:     EntityId,
            handle:     RigidBodyHandle,
            col_handle: ColliderHandle,
            linvel:     Vector<f32>,
            pos:        Vector<f32>,
            shape_pos:  Isometry<f32>,
        }

        let kdata: Vec<KData> = self.entity_bodies.iter()
            .filter_map(|(&entity, &handle)| {
                (self.get_body_type(entity) == "kinematic").then(|| {
                    let col_handle = *self.entity_colliders.get(&entity)?;
                    let body = self.bodies.get(handle)?;
                    let shape_pos = self.colliders.get(col_handle)
                        .map(|c| *c.position())
                        .unwrap_or(*body.position());
                    Some(KData {
                        entity,
                        handle,
                        col_handle,
                        linvel: *body.linvel(),
                        pos: *body.translation(),
                        shape_pos,
                    })
                }).flatten()
            }).collect();

        let dt_clamped = self.integration_params.dt;
        let cc = self.kinematic_character;

        struct KResult {
            entity: EntityId,
            handle: RigidBodyHandle,
            next_pos: Vector<f32>,
            blocked_sign: Option<f32>,
        }
        let results: Vec<KResult> = kdata.iter().filter_map(|d| {
            let script = self.kinematic_actor_vel.remove(&d.entity);
            // Godot CharacterBody: la velocidad horizontal la fija cada frame el input;
            // sin comando => 0 (no arrastrar velocidad interpolada lateral de Rapier).
            let vx = script.as_ref().map(|s| s.x).unwrap_or(0.0);
            let vy_base = script.map(|s| s.y).unwrap_or(d.linvel.y);
            let vy = vy_base + self.gravity.y * dt_clamped;
            let desired_translation = vector![vx * dt_clamped, vy * dt_clamped, 0.0];

            let character_col = self.colliders.get(d.col_handle)?;
            let filter      = QueryFilter::default().exclude_collider(d.col_handle);
            let movement    = cc.move_shape(
                dt_clamped,
                &self.bodies,
                &self.colliders,
                &self.query_pipeline,
                character_col.shape(),
                &d.shape_pos,
                desired_translation,
                filter,
                |_collision| (),
            );

            // Traslación rígida del centro del body = misma Δ que sobre el volumen swept.
            let next_pos = d.pos + movement.translation;

            // Si la traslación horizontal real es mucho menor que la deseada,
            // consideramos que chocó contra pared en esa dirección.
            let blocked_sign = {
                let desired_x = desired_translation.x;
                let actual_x = movement.translation.x;
                if desired_x.abs() > 1e-4 && actual_x.abs() < desired_x.abs() * 0.25 {
                    Some(desired_x.signum())
                } else {
                    None
                }
            };

            Some(KResult {
                entity: d.entity,
                handle: d.handle,
                next_pos,
                blocked_sign,
            })
        }).collect();

        self.blocked_horizontal_sign.clear();
        for r in &results {
            if let Some(s) = r.blocked_sign {
                self.blocked_horizontal_sign.insert(r.entity, s);
            }
        }

        // Aplicar set_next_kinematic_position
        for r in &results {
            if let Some(body) = self.bodies.get_mut(r.handle) {
                body.set_next_kinematic_position(Isometry::translation(
                    r.next_pos.x, r.next_pos.y, r.next_pos.z,
                ));
            }
        }

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

        // Sincronizar cuerpos dinámicos y kinematic de vuelta al ECS
        let pairs: Vec<(EntityId, RigidBodyHandle)> =
            self.entity_bodies.iter().map(|(&e, &h)| (e, h)).collect();
        for (entity, handle) in pairs {
            if let Some(body) = self.bodies.get(handle) {
                if body.is_dynamic() || body.is_kinematic() {
                    let t = body.translation();
                    if let Some(transform) = ecs.get_mut::<Transform>(entity) {
                        transform.position.x = t.x;
                        transform.position.y = t.y;
                    }
                }
            }
        }
    }
}
