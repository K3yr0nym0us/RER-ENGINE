use std::collections::{HashMap, HashSet};

use crate::ecs::{EntityId, World};

pub(crate) struct PhysicsWorld {
    active: HashSet<EntityId>,
    body_types: HashMap<EntityId, String>,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self {
            active: HashSet::new(),
            body_types: HashMap::new(),
        }
    }
}

impl PhysicsWorld {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn body_count(&self) -> u32 {
        self.active.len() as u32
    }

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

    pub(crate) fn step(&mut self, _dt: f32, _ecs: &mut World) {}
}
