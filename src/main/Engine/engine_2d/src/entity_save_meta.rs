use std::collections::HashMap;

use crate::config_2d::{CharacterMarker, ProjectileMarker, ScenarioMarker};
use crate::ecs::{EntityId, NameComponent, NonSelectable, Transform};
use crate::engine::State;

/// Metadatos de persistencia por entidad (ruta, tipo, puntos). El transform vive en ECS.
#[derive(Clone, Debug)]
pub struct EntitySaveMeta {
    pub kind: String,
    pub path: String,
    pub visual_model_path: Option<String>,
    pub points: Option<[[f32; 2]; 4]>,
}

#[derive(Clone, Debug)]
pub struct ScriptSourceRecord {
    pub name: String,
    pub source: String,
}

pub(crate) struct EntitySaveRegistry {
    pub meta: HashMap<EntityId, EntitySaveMeta>,
    pub script_sources: HashMap<EntityId, Vec<ScriptSourceRecord>>,
}

impl EntitySaveRegistry {
    pub fn new() -> Self {
        Self {
            meta: HashMap::new(),
            script_sources: HashMap::new(),
        }
    }

    pub fn register_meta(&mut self, id: EntityId, meta: EntitySaveMeta) {
        self.meta.insert(id, meta);
    }

    pub fn remove_entity(&mut self, id: EntityId) {
        self.meta.remove(&id);
        self.script_sources.remove(&id);
    }

    pub fn clear(&mut self) {
        self.meta.clear();
        self.script_sources.clear();
    }
}

impl State {
    pub(crate) fn is_player_entity(&self, id: EntityId) -> bool {
        if let Some(m) = self.save_registry.meta.get(&id)
            && (m.path == "[Player]" || m.path.ends_with("[Player]"))
        {
            return true;
        }
        if let Some(c) = self.world.get::<CharacterMarker>(id)
            && (c.path == "[Player]" || c.path.ends_with("[Player]"))
        {
            return true;
        }
        false
    }

    /// Metadatos de persistencia: registro explícito o inferencia desde marcadores del runtime.
    pub(crate) fn resolve_entity_save_meta(&self, id: EntityId) -> Option<EntitySaveMeta> {
        if self.world.get::<NonSelectable>(id).is_some() {
            return None;
        }
        if self.background_entity == Some(id) {
            return None;
        }
        if self.quick_build_ghost_id == Some(id) {
            return None;
        }
        if self.is_player_entity(id) {
            return None;
        }
        self.world.get::<Transform>(id)?;

        if let Some(m) = self.save_registry.meta.get(&id) {
            return Some(m.clone());
        }

        if self.collider_entities.contains(&id) {
            return Some(EntitySaveMeta {
                kind: "collider".to_string(),
                path: "[Colisionador]".to_string(),
                visual_model_path: None,
                points: self.points_from_transform_xy(id),
            });
        }

        if self.execution_area_entities.contains(&id) {
            return Some(EntitySaveMeta {
                kind: "execution_area".to_string(),
                path: "[ExecutionArea]".to_string(),
                visual_model_path: None,
                points: self.points_from_transform_xy(id),
            });
        }

        if let Some(c) = self.world.get::<CharacterMarker>(id) {
            return Some(EntitySaveMeta {
                kind: "character".to_string(),
                path: c.path.clone(),
                visual_model_path: None,
                points: None,
            });
        }

        if let Some(s) = self.world.get::<ScenarioMarker>(id) {
            return Some(EntitySaveMeta {
                kind: "scenario".to_string(),
                path: s.path.clone(),
                visual_model_path: None,
                points: None,
            });
        }

        if let Some(p) = self.world.get::<ProjectileMarker>(id) {
            return Some(EntitySaveMeta {
                kind: "projectile".to_string(),
                path: p.path.clone(),
                visual_model_path: None,
                points: None,
            });
        }

        None
    }

    pub(crate) fn collider_points_from_transform(&self, id: EntityId) -> Option<[[f32; 2]; 4]> {
        self.points_from_transform_xy(id)
    }

    fn points_from_transform_xy(&self, id: EntityId) -> Option<[[f32; 2]; 4]> {
        let t = self.world.get::<Transform>(id)?;
        let cx = t.position.x;
        let cy = t.position.y;
        let hw = t.scale.x.abs() * 0.5;
        let hh = t.scale.y.abs() * 0.5;
        Some([
            [cx - hw, cy - hh],
            [cx + hw, cy - hh],
            [cx + hw, cy + hh],
            [cx - hw, cy + hh],
        ])
    }

    pub(crate) fn entity_display_name(&self, id: EntityId) -> Option<String> {
        self.world.get::<NameComponent>(id).map(|c| c.name.clone())
    }
}
