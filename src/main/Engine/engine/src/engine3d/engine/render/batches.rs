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

/// Lote de instancias de una misma malla/capa/probe.
///
/// Tras `partition_scene_batches`, los opacos que proyectan sombra quedan
/// ordenados `[forward | extra_casters]`: el forward usa el prefijo
/// `forward_count` y el shadow map el vec entero, sin copiar instancias.
pub(crate) struct SceneInstanceBatch {
    pub(crate) mesh_idx: usize,
    pub(crate) texture_layer: u32,
    pub(crate) probe_layer: i32,
    pub(crate) instances: Vec<crate::engine3d::mesh::InstanceData>,
    pub(crate) entity_ids: Vec<crate::engine3d::ecs::EntityId>,
    /// Prefijo de `instances` que entra en el pase forward opaco.
    pub(crate) forward_count: usize,
}

/// Resultado de clasificar un conjunto de lotes una sola vez.
pub(crate) struct PartitionedBatches {
    /// Opacos que proyectan sombra: `instances` = `[forward | extra_casters]`.
    pub(crate) opaque: Vec<SceneInstanceBatch>,
    /// Opacos visibles que no proyectan (sol, suelo, fondo).
    pub(crate) opaque_no_shadow: Vec<SceneInstanceBatch>,
    /// Semitransparentes visibles en el forward blend.
    pub(crate) transparent: Vec<SceneInstanceBatch>,
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

/// Clasifica cada instancia una sola vez: forward, casters extra y skip-shadow.
///
/// El lote opaco queda como prefijo + cola para que el upload GPU del shadow
/// map cubra también el forward (mismo puntero, `instance_count` menor).
pub(crate) fn partition_scene_batches(
    batches: Vec<SceneInstanceBatch>,
    visible: &HashSet<crate::engine3d::ecs::EntityId>,
    skip_shadow: impl Fn(crate::engine3d::ecs::EntityId) -> bool,
) -> PartitionedBatches {
    let mut opaque = Vec::new();
    let mut opaque_no_shadow = Vec::new();
    let mut transparent = Vec::new();
    for batch in batches {
        let mut forward_cast = Vec::new();
        let mut forward_cast_ids = Vec::new();
        let mut extra_cast = Vec::new();
        let mut extra_cast_ids = Vec::new();
        let mut forward_skip = Vec::new();
        let mut forward_skip_ids = Vec::new();
        let mut transparent_inst = Vec::new();
        let mut transparent_ids = Vec::new();
        for (inst, entity_id) in batch.instances.into_iter().zip(batch.entity_ids) {
            let in_forward = visible.contains(&entity_id);
            if instance_is_transparent(&inst) {
                if in_forward {
                    transparent_inst.push(inst);
                    transparent_ids.push(entity_id);
                }
                continue;
            }
            if skip_shadow(entity_id) {
                if in_forward {
                    forward_skip.push(inst);
                    forward_skip_ids.push(entity_id);
                }
                continue;
            }
            if in_forward {
                forward_cast.push(inst);
                forward_cast_ids.push(entity_id);
            } else {
                extra_cast.push(inst);
                extra_cast_ids.push(entity_id);
            }
        }
        if !forward_cast.is_empty() || !extra_cast.is_empty() {
            let forward_count = forward_cast.len();
            forward_cast.extend(extra_cast);
            forward_cast_ids.extend(extra_cast_ids);
            opaque.push(SceneInstanceBatch {
                mesh_idx: batch.mesh_idx,
                texture_layer: batch.texture_layer,
                probe_layer: batch.probe_layer,
                instances: forward_cast,
                entity_ids: forward_cast_ids,
                forward_count,
            });
        }
        if !forward_skip.is_empty() {
            let forward_count = forward_skip.len();
            opaque_no_shadow.push(SceneInstanceBatch {
                mesh_idx: batch.mesh_idx,
                texture_layer: batch.texture_layer,
                probe_layer: batch.probe_layer,
                instances: forward_skip,
                entity_ids: forward_skip_ids,
                forward_count,
            });
        }
        if !transparent_inst.is_empty() {
            let forward_count = transparent_inst.len();
            transparent.push(SceneInstanceBatch {
                mesh_idx: batch.mesh_idx,
                texture_layer: batch.texture_layer,
                probe_layer: batch.probe_layer,
                instances: transparent_inst,
                entity_ids: transparent_ids,
                forward_count,
            });
        }
    }
    PartitionedBatches {
        opaque,
        opaque_no_shadow,
        transparent,
    }
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
                forward_count: 0,
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
            forward_count: 0,
        }
    }

    fn transparent_batch(ids: &[u32]) -> SceneInstanceBatch {
        let mut inst = crate::engine3d::mesh::InstanceData::new(Mat4::IDENTITY, 0.0, 0);
        inst.flag_pad[1] = 0.5;
        SceneInstanceBatch {
            mesh_idx: 0,
            texture_layer: 0,
            probe_layer: -1,
            instances: ids.iter().map(|_| inst).collect(),
            entity_ids: ids.to_vec(),
            forward_count: 0,
        }
    }

    #[test]
    fn partition_keeps_forward_as_prefix_of_shadow() {
        let part = partition_scene_batches(vec![batch(&[1, 2, 3])], &HashSet::from([2]), |_| false);
        assert_eq!(part.opaque.len(), 1);
        assert_eq!(part.opaque[0].entity_ids, vec![2, 1, 3]);
        assert_eq!(part.opaque[0].forward_count, 1);
        assert_eq!(
            part.opaque[0].instances[..1].as_ptr(),
            part.opaque[0].instances.as_ptr()
        );
        assert!(part.opaque_no_shadow.is_empty());
        assert!(part.transparent.is_empty());
    }

    #[test]
    fn partition_keeps_casters_without_forward() {
        let part = partition_scene_batches(vec![batch(&[7])], &HashSet::new(), |_| false);
        assert_eq!(part.opaque.len(), 1);
        assert_eq!(part.opaque[0].forward_count, 0);
        assert_eq!(part.opaque[0].entity_ids, vec![7]);
    }

    #[test]
    fn partition_drops_batches_with_no_forward_or_shadow() {
        let part = partition_scene_batches(vec![batch(&[7])], &HashSet::new(), |_| true);
        assert!(part.opaque.is_empty());
        assert!(part.opaque_no_shadow.is_empty());
        assert!(part.transparent.is_empty());
    }

    #[test]
    fn partition_splits_skip_shadow_and_transparent() {
        let part = partition_scene_batches(
            vec![batch(&[1, 2]), transparent_batch(&[3])],
            &HashSet::from([1, 2, 3]),
            |id| id == 1,
        );
        assert_eq!(part.opaque.len(), 1);
        assert_eq!(part.opaque[0].entity_ids, vec![2]);
        assert_eq!(part.opaque[0].forward_count, 1);
        assert_eq!(part.opaque_no_shadow.len(), 1);
        assert_eq!(part.opaque_no_shadow[0].entity_ids, vec![1]);
        assert_eq!(part.transparent.len(), 1);
        assert_eq!(part.transparent[0].entity_ids, vec![3]);
    }
}
