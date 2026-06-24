use std::collections::HashMap;

use crate::ecs::{EntityId, MeshComponent, NameComponent, NonSelectable, Transform};
use crate::engine::State;

const ENTITY_MARKERS: &[&str] = &[
    "[EditorBox]",
    "[Ground]",
    "[Player]",
    "[EditorCamera]",
    "[Sun]",
    "[Ball]",
    "[ReflectionProbe]",
    "[MatVal]",
    "[MatValLabel]",
    "[Colisionador]",
    "[ExecutionArea]",
];

/// `.glb` / `.gltf` / `.fbx` en disco (no marcadores `[Player]`).
pub(crate) fn is_model_3d_asset_path(path: &str) -> bool {
    if entity_path_marker(path).is_some() {
        return false;
    }
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".glb") || lower.ends_with(".gltf") || lower.ends_with(".fbx")
}

/// Jugador / NPC animado: cápsula de movimiento (no collider Rapier de malla).
pub(crate) fn entity_category_uses_character_capsule(category: Option<&str>) -> bool {
    matches!(category, Some("player") | Some("character"))
}

/// Jugador con mesh importado (`model_id` en biblioteca o archivo `.fbx`/`.glb`).
pub(crate) fn is_play_character_visual_model_path(path: &str) -> bool {
    if is_model_3d_asset_path(path) {
        return true;
    }
    path.starts_with("model_")
}

/// Entorno y objetos estáticos: colisión por AABB de malla.
pub(crate) fn entity_category_uses_mesh_collision(category: Option<&str>) -> bool {
    matches!(category, Some("environment") | Some("object") | Some("weapon") | Some("projectile"))
}

/// Ruta simbólica de plantilla (`[Player]`, `[EditorBox]`, …), no un archivo en disco.
pub(crate) fn entity_path_marker(p: &str) -> Option<&'static str> {
    let marker = p.split(['/', '\\']).next_back().unwrap_or(p);
    ENTITY_MARKERS
        .iter()
        .copied()
        .find(|m| *m == marker)
}

/// Tras `replace_entity_model`: conserva `path` marcador y guarda el mesh real en `visual_model_path`.
pub(crate) fn set_visual_model_on_meta(meta: &mut EntitySaveMeta, visual_path: &str) {
    meta.visual_model_path = Some(visual_path.to_string());
    if entity_path_marker(&meta.path).is_none() {
        meta.path = visual_path.to_string();
    }
}

/// Metadatos de persistencia por entidad (ruta, tipo). El transform vive en ECS.
#[derive(Clone, Debug)]
pub struct EntitySaveMeta {
    pub kind: String,
    pub path: String,
    pub visual_model_path: Option<String>,
    /// `environment` | `object` | etc. (solo entidades `model` con mesh 3D).
    pub entity_category: Option<String>,
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
    /// Ruta del archivo 3D para caché / AABB de física (respeta `visual_model_path` en marcadores).
    pub(crate) fn entity_asset_path_for_bounds(&self, id: EntityId) -> Option<String> {
        let m = self.save_registry.meta.get(&id)?;
        if let Some(visual) = m.visual_model_path.as_ref().filter(|p| !p.is_empty()) {
            return Some(visual.clone());
        }
        if entity_path_marker(&m.path).is_none() {
            return Some(m.path.clone());
        }
        None
    }

    pub(crate) fn register_or_update_visual_model_meta(
        &mut self,
        id: EntityId,
        visual_path: &str,
        is_play_character: bool,
    ) {
        if is_play_character {
            if let Some(m) = self.save_registry.meta.get_mut(&id) {
                set_visual_model_on_meta(m, visual_path);
            } else {
                self.save_registry.register_meta(
                    id,
                    EntitySaveMeta {
                        kind: "character".to_string(),
                        path: "[Player]".to_string(),
                        visual_model_path: Some(visual_path.to_string()),
                        entity_category: None,
                    },
                );
            }
            return;
        }

        if let Some(m) = self.save_registry.meta.get_mut(&id) {
            set_visual_model_on_meta(m, visual_path);
            return;
        }

        let kind = if self.character_entities.contains(&id) {
            "character"
        } else {
            "model"
        };
        let path = if self.scenario_entities.contains(&id) {
            "[EditorBox]".to_string()
        } else {
            visual_path.to_string()
        };
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: kind.to_string(),
                path,
                visual_model_path: Some(visual_path.to_string()),
                entity_category: None,
            },
        );
    }

    /// Metadatos de persistencia: registro explícito o inferencia desde listas del runtime.
    pub(crate) fn resolve_entity_save_meta(&self, id: EntityId) -> Option<EntitySaveMeta> {
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

        if self.world.get::<NonSelectable>(id).is_some() {
            return None;
        }

        if Some(id) == self.play_character_entity {
            let visual = self
                .save_registry
                .meta
                .get(&id)
                .and_then(|m| m.visual_model_path.clone());
            return Some(EntitySaveMeta {
                kind: "character".to_string(),
                path: "[Player]".to_string(),
                visual_model_path: visual,
                entity_category: None,
            });
        }

        if Some(id) == self.sun_entity {
            return Some(EntitySaveMeta {
                kind: "directional_light".to_string(),
                path: "[Sun]".to_string(),
                visual_model_path: None,
                entity_category: None,
            });
        }

        if self.collider_entities.contains(&id) {
            return Some(EntitySaveMeta {
                kind: "collider".to_string(),
                path: "[Colisionador]".to_string(),
                visual_model_path: None,
                entity_category: None,
            });
        }

        if self.execution_area_entities.contains(&id) {
            return Some(EntitySaveMeta {
                kind: "execution_area".to_string(),
                path: "[ExecutionArea]".to_string(),
                visual_model_path: None,
                entity_category: None,
            });
        }

        if self.scenario_entities.contains(&id) {
            return Some(EntitySaveMeta {
                kind: "model".to_string(),
                path: "[EditorBox]".to_string(),
                visual_model_path: None,
                entity_category: None,
            });
        }

        if self.world.get::<MeshComponent>(id).is_some() {
            return Some(EntitySaveMeta {
                kind: "model".to_string(),
                path: "[EditorBox]".to_string(),
                visual_model_path: None,
                entity_category: None,
            });
        }

        None
    }

    pub(crate) fn entity_display_name(&self, id: EntityId) -> Option<String> {
        self.world
            .get::<NameComponent>(id)
            .map(|c| c.name.clone())
    }
}
