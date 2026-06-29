//! Registro de reflection probes: identidad, ranuras cubemap y metadatos GPU.

use std::collections::{HashMap, HashSet};

use glam::Vec3;

use crate::ecs::EntityId;
use crate::entity_save_meta::{entity_path_marker, EntitySaveRegistry};
use crate::reflections::policy::ProbeSlot;
use crate::reflections::probe_env::{ProbeMetaUniform, MAX_PROBES};

pub const REFLECTION_PROBE_PATH_MARKER: &str = "[ReflectionProbe]";

/// Radio de influencia por defecto al insertar una probe (metros). Mismo orden que la pelota física de plantilla.
pub const DEFAULT_REFLECTION_PROBE_INFLUENCE_M: f32 = 0.3;

/// Radio del gizmo de editor (metros); marcador fijo, no el volumen de influencia IBL.
pub const REFLECTION_PROBE_GIZMO_RADIUS_M: f32 = 0.3;
pub fn reflection_probe_entities(registry: &EntitySaveRegistry) -> Vec<EntityId> {
    let mut ids: Vec<_> = registry
        .meta
        .iter()
        .filter_map(|(id, m)| {
            if entity_path_marker(&m.path) == Some(REFLECTION_PROBE_PATH_MARKER) {
                Some(*id)
            } else {
                None
            }
        })
        .collect();
    ids.sort_unstable();
    ids
}

pub fn is_reflection_probe_entity(registry: &EntitySaveRegistry, id: EntityId) -> bool {
    registry.meta.get(&id).is_some_and(|m| {
        entity_path_marker(&m.path) == Some(REFLECTION_PROBE_PATH_MARKER)
    })
}

/// Asigna ranura fija 0..MAX_PROBES-1; reutiliza la existente si ya estaba registrada.
pub fn allocate_probe_slot(slots: &mut HashMap<EntityId, ProbeSlot>, id: EntityId) -> Option<ProbeSlot> {
    if let Some(&slot) = slots.get(&id) {
        return Some(slot);
    }
    let used: HashSet<ProbeSlot> = slots.values().copied().collect();
    for slot in 0..MAX_PROBES {
        if !used.contains(&slot) {
            slots.insert(id, slot);
            return Some(slot);
        }
    }
    log::warn!("[reflexiones] sin ranuras libres en cubemap para probe {id}");
    None
}

pub fn release_probe_slot(slots: &mut HashMap<EntityId, ProbeSlot>, id: EntityId) -> bool {
    slots.remove(&id).is_some()
}

pub fn ensure_probe_slots_allocated(
    registry: &EntitySaveRegistry,
    slots: &mut HashMap<EntityId, ProbeSlot>,
) {
    for id in reflection_probe_entities(registry) {
        allocate_probe_slot(slots, id);
    }
}

/// Probes activos: (entidad, centro, ranura cubemap). La ranura es estable por `EntityId`.
pub fn reflection_probe_render_list(
    registry: &EntitySaveRegistry,
    slots: &HashMap<EntityId, ProbeSlot>,
    world_center: impl Fn(EntityId) -> Option<Vec3>,
) -> Vec<(EntityId, Vec3, ProbeSlot)> {
    reflection_probe_entities(registry)
        .into_iter()
        .filter_map(|id| {
            let slot = slots.get(&id).copied()?;
            let center = world_center(id)?;
            Some((id, center, slot))
        })
        .take(MAX_PROBES)
        .collect()
}

/// Si el conjunto de probes activos cambió, actualiza `last_ids` y devuelve true.
pub fn sync_capture_burst_for_entity_set(
    last_ids: &mut Option<Vec<EntityId>>,
    probe_ids: &[EntityId],
) -> bool {
    let changed = last_ids
        .as_ref()
        .map_or(true, |prev| prev.as_slice() != probe_ids);
    if changed {
        *last_ids = Some(probe_ids.to_vec());
    }
    changed
}

/// Radio de influencia (metros) al buscar la probe más cercana. Coincide con la escala de la entidad.
pub fn probe_world_radius(scale_x_abs_half: f32) -> f32 {
    scale_x_abs_half.max(0.1)
}

/// Rellena `probe_meta.entries[slot]` con centro xyz + radio w.
pub fn build_probe_meta(
    probe_list: &[(EntityId, Vec3, ProbeSlot)],
    radius_for: impl Fn(EntityId) -> f32,
) -> ProbeMetaUniform {
    let mut probe_meta = ProbeMetaUniform::default();
    for &(id, center, slot) in probe_list {
        let radius = radius_for(id);
        probe_meta.entries[slot] = [center.x, center.y, center.z, radius];
    }
    probe_meta
}

/// Mapa entidad → ranura para instancing y RT materials.
pub fn probe_index_map_from_list(
    probe_list: &[(EntityId, Vec3, ProbeSlot)],
) -> HashMap<EntityId, ProbeSlot> {
    probe_list.iter().map(|(id, _, slot)| (*id, *slot)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_save_meta::EntitySaveMeta;
    use std::collections::HashMap;

    fn probe_meta(_id: EntityId) -> EntitySaveMeta {
        EntitySaveMeta {
            kind: "model".to_string(),
            path: REFLECTION_PROBE_PATH_MARKER.to_string(),
            visual_model_path: None,
            entity_category: Some("object".to_string()),
        }
    }

    #[test]
    fn five_probes_get_distinct_slots() {
        let mut registry = EntitySaveRegistry::new();
        let mut slots = HashMap::new();
        for id in 1u32..=5 {
            registry.register_meta(id, probe_meta(id));
            let slot = allocate_probe_slot(&mut slots, id).expect("slot");
            assert_eq!(slot, (id - 1) as usize);
        }
        assert_eq!(slots.len(), 5);
    }

    #[test]
    fn release_frees_slot_for_reuse() {
        let mut slots = HashMap::new();
        assert_eq!(allocate_probe_slot(&mut slots, 10), Some(0));
        assert_eq!(allocate_probe_slot(&mut slots, 20), Some(1));
        assert!(release_probe_slot(&mut slots, 10));
        assert_eq!(allocate_probe_slot(&mut slots, 30), Some(0));
        assert_eq!(slots.get(&20), Some(&1));
        assert_eq!(slots.get(&30), Some(&0));
    }

    #[test]
    fn build_probe_meta_fills_entries() {
        let list = vec![
            (1u32, Vec3::new(-4.0, 0.8, 1.5), 0usize),
            (2u32, Vec3::new(0.0, 0.8, 1.5), 1usize),
        ];
        let meta = build_probe_meta(&list, |_| 0.8);
        assert_eq!(meta.entries[0], [-4.0, 0.8, 1.5, 0.8]);
        assert_eq!(meta.entries[1], [0.0, 0.8, 1.5, 0.8]);
        assert_eq!(meta.entries[2], [0.0; 4]);
    }
}
