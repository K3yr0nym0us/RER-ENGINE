//! Registro de reflection probes (lógica pura, sin GPU).
//!
//! Sustento de las call sites que tenía `reflections::probes_pipeline::registry` tras borrar
//! el módulo `reflections/` (supersedido por `vulkan::FrameRenderer` en Fase 4). Mantiene las
//! ranuras de cubemap por entidad y la detección de entidades probe.

use std::collections::{HashMap, HashSet};

use crate::ecs::EntityId;
use crate::entity_save_meta::EntitySaveRegistry;

/// Radio del gizmo de editor para probes (metros).
pub const REFLECTION_PROBE_GIZMO_RADIUS_M: f32 = 0.3;

/// Número máximo de reflection probes simultáneas.
pub const MAX_PROBES: usize = 8;

/// Radio de influencia por defecto al insertar una probe (metros).
pub const DEFAULT_REFLECTION_PROBE_INFLUENCE_M: f32 = 20.0;

/// Tipo de ranura de probe (slot fijo 0..MAX_PROBES-1).
pub type ProbeSlot = usize;

/// Entidades que son reflection probes (tienen marcador de ruta `[ReflectionProbe]`).
pub fn reflection_probe_entities(registry: &EntitySaveRegistry) -> Vec<EntityId> {
    let mut ids: Vec<EntityId> = Vec::new();
    for (id, meta) in registry.meta.iter() {
        if meta
            .entity_category
            .as_deref()
            .is_some_and(|c| c.eq_ignore_ascii_case("reflection_probe"))
        {
            ids.push(*id);
        }
    }
    ids
}

/// Devuelve true si la entidad está registrada como probe.
pub fn is_reflection_probe_entity(registry: &EntitySaveRegistry, id: EntityId) -> bool {
    registry.meta.get(&id).is_some_and(|m| {
        m.entity_category
            .as_deref()
            .is_some_and(|c| c.eq_ignore_ascii_case("reflection_probe"))
    })
}

/// Asigna ranura fija 0..MAX_PROBES-1; reutiliza la existente si ya estaba registrada.
pub fn allocate_probe_slot(
    slots: &mut HashMap<EntityId, ProbeSlot>,
    id: EntityId,
) -> Option<ProbeSlot> {
    if let Some(&slot) = slots.get(&id) {
        return Some(slot);
    }
    let used: HashSet<ProbeSlot> = slots.values().copied().collect();
    let slot = (0..MAX_PROBES).find(|s| !used.contains(s))?;
    slots.insert(id, slot);
    Some(slot)
}

/// Libera la ranura de una probe. Devuelve true si existía.
pub fn release_probe_slot(slots: &mut HashMap<EntityId, ProbeSlot>, id: EntityId) -> bool {
    slots.remove(&id).is_some()
}

/// Asegura ranura para cada probe registrado (cargas .save, plantilla FP, etc.).
pub fn ensure_probe_slots_allocated(
    registry: &EntitySaveRegistry,
    slots: &mut HashMap<EntityId, ProbeSlot>,
) {
    for id in reflection_probe_entities(registry) {
        let _ = allocate_probe_slot(slots, id);
    }
}

/// Probes activos: (entidad, centro, ranura cubemap). La ranura es estable por `EntityId`.
pub fn reflection_probe_render_list<F>(
    registry: &EntitySaveRegistry,
    slots: &HashMap<EntityId, ProbeSlot>,
    position_of: F,
) -> Vec<(EntityId, glam::Vec3, ProbeSlot)>
where
    F: Fn(EntityId) -> Option<glam::Vec3>,
{
    let mut out: Vec<(EntityId, glam::Vec3, ProbeSlot)> = Vec::new();
    for id in reflection_probe_entities(registry) {
        let Some(&slot) = slots.get(&id) else {
            continue;
        };
        let Some(pos) = position_of(id) else { continue };
        out.push((id, pos, slot));
    }
    out
}

/// Si el conjunto de probes activos cambió, solicita burst de captura.
pub fn sync_capture_burst_for_entity_set(
    last_ids: &mut Option<Vec<EntityId>>,
    probe_ids: &[EntityId],
) -> bool {
    let changed = match last_ids {
        Some(prev) => {
            if prev.len() != probe_ids.len() {
                true
            } else {
                let mut prev_sorted = prev.clone();
                let mut cur_sorted = probe_ids.to_vec();
                prev_sorted.sort_unstable();
                cur_sorted.sort_unstable();
                prev_sorted != cur_sorted
            }
        }
        None => !probe_ids.is_empty(),
    };
    if changed {
        *last_ids = Some(probe_ids.to_vec());
    }
    changed
}
