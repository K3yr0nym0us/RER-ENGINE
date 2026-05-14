use std::collections::{HashMap, HashSet};

use crate::ecs::{EntityId, World};

pub(crate) struct PhysicsWorld2D {
    active: HashSet<EntityId>,
    body_types: HashMap<EntityId, String>,
}

impl Default for PhysicsWorld2D {
    fn default() -> Self {
        Self {
            active: HashSet::new(),
            body_types: HashMap::new(),
        }
    }
}

impl PhysicsWorld2D {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn body_count(&self) -> u32 {
        self.active.len() as u32
    }

    pub(crate) fn set_gravity(&mut self, _gravity_y: f32) {}

    pub(crate) fn set_entity_physics(
        &mut self,
        entity: EntityId,
        enabled: bool,
        body_type: &str,
        _position: [f32; 3],
        _half_ext: [f32; 3],
    ) {
        if enabled {
            self.active.insert(entity);
            self.body_types.insert(entity, body_type.to_string());
        } else {
            self.active.remove(&entity);
            self.body_types.remove(&entity);
        }
    }

    pub(crate) fn remove_entity_body(&mut self, entity: EntityId) {
        self.active.remove(&entity);
        self.body_types.remove(&entity);
    }

    pub(crate) fn has_physics(&self, entity: EntityId) -> bool {
        self.active.contains(&entity)
    }

    pub(crate) fn get_body_type(&self, entity: EntityId) -> &str {
        self.body_types
            .get(&entity)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub(crate) fn teleport_entity(&mut self, _entity: EntityId, _x: f32, _y: f32) {}

    pub(crate) fn move_physics_entity(
        &mut self,
        entity: EntityId,
        _speed: f32,
        _dir_x: f32,
        _dir_y: f32,
        _dt: f32,
    ) -> bool {
        self.active.contains(&entity)
    }

    pub(crate) fn step(&mut self, _dt: f32, _ecs: &mut World) {}
}
