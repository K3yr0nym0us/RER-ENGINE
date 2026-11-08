//! Registro de reflection probes (lógica pura, sin GPU).
//!
//! Sustento de las call sites que tenía `reflections::probes_pipeline::registry` tras borrar
//! el módulo `reflections/` (supersedido por `vulkan::FrameRenderer` en Fase 4). Mantiene las
//! ranuras de cubemap por entidad y la detección de entidades probe.

use std::collections::{HashMap, HashSet};

use glam::Vec3;

use crate::ecs::EntityId;
use crate::entity_save_meta::{EntitySaveMeta, EntitySaveRegistry, entity_path_marker};

/// Marcador de plantilla que identifica la entidad sonda (se guarda en `EntitySaveMeta.path`).
pub const REFLECTION_PROBE_PATH_MARKER: &str = "[ReflectionProbe]";

/// Radio del gizmo de editor para probes (metros).
pub const REFLECTION_PROBE_GIZMO_RADIUS_M: f32 = 0.3;

/// Número máximo de reflection probes simultáneas (= capas del cubemap array / 6).
pub const MAX_PROBES: usize = 8;

/// Lado de cada cara del cubemap, en px.
pub const PROBE_FACE_SIZE: u32 = 64;

/// Capas totales del `texture_cube_array`: 6 caras por ranura.
pub const PROBE_CUBE_ARRAY_LAYERS: u32 = (MAX_PROBES as u32) * 6;

/// Radio de influencia por defecto al insertar una probe (metros).
pub const DEFAULT_REFLECTION_PROBE_INFLUENCE_M: f32 = 20.0;

/// Tipo de ranura de probe (slot fijo 0..MAX_PROBES-1).
pub type ProbeSlot = usize;

/// El spawn guarda el radio de influencia como `scale = radio * 2`.
pub fn reflection_probe_world_radius(scale_x: f32) -> f32 {
    (scale_x * 0.5).max(0.15)
}

/// Una sonda con su centro, radio de influencia y ranura de cubemap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeField {
    pub center: Vec3,
    pub radius_m: f32,
    pub slot: ProbeSlot,
}

/// Veredicto de una meta de guardado: sonda si lleva el marcador de plantilla o la categoría.
///
/// Antes solo se miraba `entity_category == "reflection_probe"`, pero `spawn_reflection_probe_entity`
/// registra la categoría `"object"` (para que el editor la liste como objeto) y el marcador en
/// `path`. Resultado: ninguna entidad se reconocía nunca como sonda, así que no se capturaba
/// ninguna cara y el termino IBL del shader estaba muerto.
fn is_probe_meta(meta: &EntitySaveMeta) -> bool {
    entity_path_marker(&meta.path) == Some(REFLECTION_PROBE_PATH_MARKER)
        || meta
            .entity_category
            .as_deref()
            .is_some_and(|c| c.eq_ignore_ascii_case("reflection_probe"))
}

/// Entidades que son reflection probes.
pub fn reflection_probe_entities(registry: &EntitySaveRegistry) -> Vec<EntityId> {
    let mut ids: Vec<EntityId> = registry
        .meta
        .iter()
        .filter(|(_, meta)| is_probe_meta(meta))
        .map(|(id, _)| *id)
        .collect();
    ids.sort_unstable();
    ids
}

/// Devuelve true si la entidad está registrada como probe.
pub fn is_reflection_probe_entity(registry: &EntitySaveRegistry, id: EntityId) -> bool {
    registry.meta.get(&id).is_some_and(is_probe_meta)
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

/// Sondas con su volumen de influencia real, para elegir qué ranura ilumina cada entidad.
/// `pose_of` devuelve (posición, `scale.x`), ya que el spawn guarda el radio como `scale = r*2`.
pub fn reflection_probe_fields<F>(
    registry: &EntitySaveRegistry,
    slots: &HashMap<EntityId, ProbeSlot>,
    pose_of: F,
) -> Vec<ProbeField>
where
    F: Fn(EntityId) -> Option<(Vec3, f32)>,
{
    let mut out: Vec<ProbeField> = Vec::new();
    for id in reflection_probe_entities(registry) {
        let Some(&slot) = slots.get(&id) else {
            continue;
        };
        let Some((center, scale_x)) = pose_of(id) else {
            continue;
        };
        out.push(ProbeField {
            center,
            radius_m: reflection_probe_world_radius(scale_x),
            slot,
        });
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

/// Ranura de la sonda cuyo volumen de influencia contiene el punto; si varias lo contienen, la de
/// centro más cercano. `None` = fuera de todo volumen → el shader usa el entorno procedural.
pub fn select_probe_slot(fields: &[ProbeField], point: Vec3) -> Option<ProbeSlot> {
    let mut best: Option<(f32, ProbeSlot)> = None;
    for field in fields {
        let radius = field.radius_m.max(0.15);
        let distance = (point - field.center).length();
        if distance > radius {
            continue;
        }
        // Normalizado: una sonda grande gana a una pequeña si el punto está igual de lejos de su
        // centro relativo, evitando que una esfera diminuta tape zonas de una grande.
        let relative = distance / radius;
        if best.is_none_or(|(best_relative, _)| relative < best_relative) {
            best = Some((relative, field.slot));
        }
    }
    best.map(|(_, slot)| slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(x: f32, radius: f32, slot: ProbeSlot) -> ProbeField {
        ProbeField {
            center: Vec3::new(x, 0.0, 0.0),
            radius_m: radius,
            slot,
        }
    }

    #[test]
    fn radius_is_half_of_the_stored_scale() {
        // `spawn_reflection_probe_entity` guarda `scale = radio * 2`.
        assert!((reflection_probe_world_radius(40.0) - 20.0).abs() < 1e-5);
        assert!((reflection_probe_world_radius(0.0) - 0.15).abs() < 1e-5);
    }

    #[test]
    fn fuera_de_todo_volumen_no_elige_ranura() {
        let fields = [field(0.0, 2.0, 3)];
        assert_eq!(select_probe_slot(&fields, Vec3::new(9.0, 0.0, 0.0)), None);
        assert_eq!(select_probe_slot(&[], Vec3::ZERO), None);
    }

    #[test]
    fn elige_la_sonda_mas_cercana_cuando_se_solapan() {
        let fields = [field(0.0, 10.0, 1), field(6.0, 10.0, 2)];
        assert_eq!(
            select_probe_slot(&fields, Vec3::new(5.5, 0.0, 0.0)),
            Some(2)
        );
        assert_eq!(
            select_probe_slot(&fields, Vec3::new(1.0, 0.0, 0.0)),
            Some(1)
        );
    }

    #[test]
    fn gana_el_centro_mas_cerca_relativo_a_su_volumen() {
        // Mismo radio en ambas: el punto está más cerca del centro de la ranura 5.
        let fields = [field(-2.0, 5.0, 4), field(2.0, 5.0, 5)];
        assert_eq!(
            select_probe_slot(&fields, Vec3::new(0.5, 0.0, 0.0)),
            Some(5)
        );
        // Y al revés, del otro lado.
        assert_eq!(
            select_probe_slot(&fields, Vec3::new(-0.5, 0.0, 0.0)),
            Some(4)
        );
    }

    #[test]
    fn una_sonda_pequena_no_tapa_a_grande_cerca_de_su_centro() {
        let big = field(0.0, 20.0, 7);
        let small = field(8.0, 0.5, 0);
        let fields = [big, small];
        // Dentro del volumen pequeño manda el pequeño; fuera de él, el grande.
        assert_eq!(
            select_probe_slot(&fields, Vec3::new(8.1, 0.0, 0.0)),
            Some(0)
        );
        assert_eq!(
            select_probe_slot(&fields, Vec3::new(12.0, 0.0, 0.0)),
            Some(7)
        );
    }

    #[test]
    fn capa_del_cubemap_array_para_una_ranura() {
        // `copy_color` escribe en `base_array_layer`, así que la cara `f` de la ranura `s`
        // ocupa la capa `s * 6 + f`.
        let slot: ProbeSlot = 3;
        for face in 0..6u32 {
            let layer = (slot as u32) * 6 + face;
            assert!((layer as usize) < PROBE_CUBE_ARRAY_LAYERS as usize);
        }
        assert_eq!(PROBE_CUBE_ARRAY_LAYERS, (MAX_PROBES as u32) * 6);
    }
}
