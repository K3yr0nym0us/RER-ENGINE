use std::collections::HashMap;

use crate::ecs::{EntityId, MeshComponent, NameComponent, NonSelectable, Transform};
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
    /// Metadatos de persistencia: registro explícito o inferencia desde listas del runtime.
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
        if self.world.get::<Transform>(id).is_none() {
            return None;
        }

        if let Some(m) = self.save_registry.meta.get(&id) {
            return Some(m.clone());
        }

        if Some(id) == self.play_character_entity {
            return Some(EntitySaveMeta {
                kind: "character".to_string(),
                path: "[Player]".to_string(),
                visual_model_path: None,
                points: None,
            });
        }

        if self.collider_entities.contains(&id) {
            return Some(EntitySaveMeta {
                kind: "collider".to_string(),
                path: "[Colisionador]".to_string(),
                visual_model_path: None,
                points: self.points_from_transform_xz(id),
            });
        }

        if self.execution_area_entities.contains(&id) {
            return Some(EntitySaveMeta {
                kind: "execution_area".to_string(),
                path: "[ExecutionArea]".to_string(),
                visual_model_path: None,
                points: self.points_from_transform_xz(id),
            });
        }

        if self.character_entities.contains(&id) {
            return Some(EntitySaveMeta {
                kind: "character".to_string(),
                path: "[Character]".to_string(),
                visual_model_path: None,
                points: None,
            });
        }

        if self.scenario_entities.contains(&id) {
            return Some(EntitySaveMeta {
                kind: "model".to_string(),
                path: "[EditorBox]".to_string(),
                visual_model_path: None,
                points: None,
            });
        }

        if self.world.get::<MeshComponent>(id).is_some() {
            return Some(EntitySaveMeta {
                kind: "model".to_string(),
                path: "[EditorBox]".to_string(),
                visual_model_path: None,
                points: None,
            });
        }

        None
    }

    fn points_from_transform_xz(&self, id: EntityId) -> Option<[[f32; 2]; 4]> {
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
        self.world
            .get::<NameComponent>(id)
            .map(|c| c.name.clone())
    }
}
