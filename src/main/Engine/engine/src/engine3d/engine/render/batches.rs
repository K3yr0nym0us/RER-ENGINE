//! Batches de instancias de escena y aplicación de materiales PBR.

use std::collections::HashSet;

use super::*;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct TransparentReflUniforms {
    pub(crate) enabled: f32,
    pub(crate) strength: f32,
    pub(crate) refl_mix: f32,
    pub(crate) _pad: f32,
}

pub(crate) fn instance_is_transparent(inst: &crate::engine3d::mesh::InstanceData) -> bool {
    inst.flag_pad[1] < 0.99
}

pub(crate) fn apply_surface_pbr_to_instance(
    inst: &mut crate::engine3d::mesh::InstanceData,
    pbr: &crate::engine3d::ecs::SurfacePbr,
) {
    inst.flag_pad[3] = pbr.roughness;
    inst.tex_layer_pad[1] = pbr.metallic;
    inst.tex_layer_pad[3] = pbr.ior;
    inst.flag_pad[1] = crate::engine3d::config_3d::pbr_presets::instance_visual_alpha(pbr);
}

/// Lote de instancias de una misma malla/capa/probe para el pase forward.
pub(crate) struct SceneInstanceBatch {
    pub(crate) mesh_idx: usize,
    pub(crate) texture_layer: u32,
    pub(crate) probe_layer: i32,
    pub(crate) instances: Vec<crate::engine3d::mesh::InstanceData>,
    pub(crate) entity_ids: Vec<crate::engine3d::ecs::EntityId>,
}

/// Wireframe de esferas para los reflection probes del editor.
pub(crate) fn build_probe_gizmo_verts(
    list: &[(crate::engine3d::ecs::EntityId, GlamVec3, usize)],
) -> Vec<crate::engine3d::gizmo::GizmoVertex> {
    let color = [1.0_f32, 0.85, 0.18, 0.9];
    let radius = crate::engine3d::probe_registry::REFLECTION_PROBE_GIZMO_RADIUS_M;
    let segments = 48u32;
    let mut verts = Vec::new();
    for (_, center, _) in list {
        for axis in 0..3 {
            for i in 0..segments {
                let a0 = std::f32::consts::TAU * i as f32 / segments as f32;
                let a1 = std::f32::consts::TAU * (i + 1) as f32 / segments as f32;
                let p0 = crate::engine3d::gizmo::rotation_ring_point(axis, radius, a0);
                let p1 = crate::engine3d::gizmo::rotation_ring_point(axis, radius, a1);
                for p in [p0, p1] {
                    verts.push(crate::engine3d::gizmo::GizmoVertex {
                        position: [center.x + p[0], center.y + p[1], center.z + p[2]],
                        color,
                    });
                }
            }
        }
    }
    verts
}

pub(crate) fn filter_batches_by_alpha(
    batches: &[SceneInstanceBatch],
    want_transparent: bool,
) -> Vec<SceneInstanceBatch> {
    batches
        .iter()
        .filter_map(|b| {
            let mut instances = Vec::new();
            let mut entity_ids = Vec::new();
            for (inst, eid) in b.instances.iter().zip(b.entity_ids.iter()) {
                if instance_is_transparent(inst) == want_transparent {
                    instances.push(*inst);
                    entity_ids.push(*eid);
                }
            }
            if instances.is_empty() {
                return None;
            }
            Some(SceneInstanceBatch {
                mesh_idx: b.mesh_idx,
                texture_layer: b.texture_layer,
                probe_layer: b.probe_layer,
                instances,
                entity_ids,
            })
        })
        .collect()
}

/// Deja solo las entidades visibles en el pase forward.
///
/// Los lotes se construyen con el conjunto ampliado de casters (frustum de cámara
/// más margen hacia el sol); el forward necesita el subconjunto estricto. El
/// shadow map, en cambio, consume los lotes completos.
pub(crate) fn filter_batches_by_entity_set(
    batches: &[SceneInstanceBatch],
    visible: &HashSet<crate::engine3d::ecs::EntityId>,
) -> Vec<SceneInstanceBatch> {
    batches
        .iter()
        .filter_map(|batch| {
            let mut instances = Vec::new();
            let mut entity_ids = Vec::new();
            for (inst, eid) in batch.instances.iter().zip(batch.entity_ids.iter()) {
                if visible.contains(eid) {
                    instances.push(*inst);
                    entity_ids.push(*eid);
                }
            }
            if instances.is_empty() {
                return None;
            }
            Some(SceneInstanceBatch {
                mesh_idx: batch.mesh_idx,
                texture_layer: batch.texture_layer,
                probe_layer: batch.probe_layer,
                instances,
                entity_ids,
            })
        })
        .collect()
}

/// Instancias estáticas para escena o captura de probe.
pub(crate) fn build_scene_instance_batches(
    state: &State,
    entities: &[(crate::engine3d::ecs::EntityId, usize, usize, Mat4, i32)],
    probe_fields: &[crate::engine3d::probe_registry::ProbeField],
) -> Vec<SceneInstanceBatch> {
    let mut batches: Vec<SceneInstanceBatch> = Vec::new();
    for (entity_id, mesh_idx, tex_idx, model_matrix, _layer) in entities {
        if state.quick_build_ghost_id == Some(*entity_id)
            || state.plane_tool_ghost_id == Some(*entity_id)
        {
            continue;
        }
        if state.preview_playing
            && !state.debug_mode
            && (state.collider_entities.contains(entity_id)
                || state.execution_area_entities.contains(entity_id))
        {
            continue;
        }
        let is_selected = state.selected_entity == Some(*entity_id)
            || state.selected_entities.contains(entity_id);
        let is_hovered = state.hovered_entity == Some(*entity_id);
        let flag = state.editor_selection_flag(*entity_id, is_selected, is_hovered);
        let layer = state.texture_layer_for(*tex_idx);
        let mut inst = crate::engine3d::mesh::InstanceData::new(*model_matrix, flag, layer);
        let is_terrain = state
            .world
            .get::<crate::engine3d::ecs::NameComponent>(*entity_id)
            .is_some_and(|name| name.name == "TerrainChunk");
        if let Some(pbr) = state
            .world
            .get::<crate::engine3d::ecs::SurfacePbr>(*entity_id)
        {
            // `tex_layer_pad.w` es IOR en modelos y la capa Grass en terreno. El IOR
            // del terreno es 0 y coincidiría con el checker (capa 0).
            if is_terrain {
                inst.flag_pad[3] = pbr.roughness;
                inst.tex_layer_pad[1] = pbr.metallic;
                inst.flag_pad[1] =
                    crate::engine3d::config_3d::pbr_presets::instance_visual_alpha(pbr);
            } else {
                apply_surface_pbr_to_instance(&mut inst, pbr);
            }
        }
        // Ranura de sonda de la entidad: la más cercana cuyo volumen la contiene (`-1` = ninguna →
        // el shader cae al cielo procedural). Se resuelve por entidad y entra en la clave de lote,
        // así dos entidades con la misma malla pero distinta sonda no comparten instancing.
        let probe_layer = crate::engine3d::probe_registry::select_probe_slot(
            probe_fields,
            model_matrix.transform_point3(GlamVec3::ZERO),
        )
        .map(|slot| slot as i32)
        .unwrap_or(-1);
        inst.tex_layer_pad[2] = probe_layer as f32;
        if state.is_plane_wall_entity(*entity_id) {
            inst.flag_pad[1] = crate::engine3d::config_3d::plane_tools::PLANE_WALL_VISUAL_ALPHA;
            inst.flag_pad[2] = crate::engine3d::config_3d::plane_tools::PLANE_WALL_RENDER_KIND;
        }
        if is_terrain && let Some(base) = state.worldgen.material_base {
            inst.flag_pad[2] = crate::engine3d::mesh::RENDER_KIND_TERRAIN;
            inst.tex_layer_pad[3] = base as f32;
            if let Some(&tile) = state.worldgen.entity_splat_tile.get(entity_id) {
                inst.tex_layer_pad[1] = tile as f32;
            }
        }
        let can_extend = batches.last().is_some_and(|b| {
            b.mesh_idx == *mesh_idx && b.texture_layer == layer && b.probe_layer == probe_layer
        });
        if can_extend {
            batches.last_mut().unwrap().instances.push(inst);
            batches.last_mut().unwrap().entity_ids.push(*entity_id);
        } else {
            batches.push(SceneInstanceBatch {
                mesh_idx: *mesh_idx,
                texture_layer: layer,
                probe_layer,
                instances: vec![inst],
                entity_ids: vec![*entity_id],
            });
        }
    }
    batches
}

pub(crate) fn entity_skips_shadow_cast(
    state: &super::State,
    entity_id: crate::engine3d::ecs::EntityId,
) -> bool {
    // El suelo no se auto-sombrea; el icono del sol es la luz (no un objeto que
    // proyecte sombra) y el fondo es solo decoración.
    state.sun_entity == Some(entity_id)
        || state.background_entity == Some(entity_id)
        || state
            .world
            .get::<crate::engine3d::ecs::NameComponent>(entity_id)
            .is_some_and(|name| {
                let n = name.name.as_str();
                n.eq_ignore_ascii_case("sun")
                    || n.eq_ignore_ascii_case("ground")
                    || (n.len() >= 4 && n[..4].eq_ignore_ascii_case("sun_"))
            })
}

/// Lotes que escriben el mapa de sombras. El checker no entra (se auto-sombrea);
/// el terreno sí, para que las montañas proyecten sobre el suelo.
pub(crate) fn filter_shadow_cast_batches(
    state: &super::State,
    batches: &[SceneInstanceBatch],
) -> Vec<SceneInstanceBatch> {
    batches
        .iter()
        .filter_map(|batch| {
            let mut instances = Vec::new();
            let mut entity_ids = Vec::new();
            for (instance, entity_id) in batch.instances.iter().zip(&batch.entity_ids) {
                if entity_skips_shadow_cast(state, *entity_id) {
                    continue;
                }
                instances.push(*instance);
                entity_ids.push(*entity_id);
            }
            if instances.is_empty() {
                return None;
            }
            Some(SceneInstanceBatch {
                mesh_idx: batch.mesh_idx,
                texture_layer: batch.texture_layer,
                probe_layer: batch.probe_layer,
                instances,
                entity_ids,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Mat4;

    fn batch(ids: &[u32]) -> SceneInstanceBatch {
        let inst = crate::engine3d::mesh::InstanceData::new(Mat4::IDENTITY, 0.0, 0);
        SceneInstanceBatch {
            mesh_idx: 0,
            texture_layer: 0,
            probe_layer: -1,
            instances: ids.iter().map(|_| inst).collect(),
            entity_ids: ids.to_vec(),
        }
    }

    #[test]
    fn forward_batches_keep_only_strictly_visible_entities() {
        let batches = vec![batch(&[1, 2, 3])];
        let forward = filter_batches_by_entity_set(&batches, &HashSet::from([2]));
        assert_eq!(forward.len(), 1);
        assert_eq!(forward[0].entity_ids, vec![2]);
        assert_eq!(forward[0].instances.len(), 1);
        // El lote completo sigue intacto para el shadow map.
        assert_eq!(batches[0].entity_ids, vec![1, 2, 3]);
    }

    #[test]
    fn forward_batches_drop_empty_results() {
        let batches = vec![batch(&[7])];
        assert!(filter_batches_by_entity_set(&batches, &HashSet::new()).is_empty());
    }
}
