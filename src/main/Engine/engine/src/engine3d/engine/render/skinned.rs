//! Seguimiento de mallas skinned para el trazado de rayos.

use super::*;

/// Cada cuántos frames se re-deforma la geometría skinned para el BVH. A 60 fps son 15 Hz de
/// actualización de la silueta reflejada, de sobra para un reflejo y mucho más barato que un
/// `build_bvh` por frame.
pub(crate) const SKINNED_TRACE_REFRESH_FRAMES: u32 = 4;

/// Techo de triángulos skinned trazables por escena: protege el frame en rigs con mallas pesadas.
pub(crate) const SKINNED_TRACE_MAX_TRIANGLES: usize = 24_000;

/// Una pieza skinned visible, con todo lo necesario para trazarse.
#[derive(Clone)]
pub(crate) struct SkinnedTraceTarget {
    pub(crate) entity_id: crate::engine3d::ecs::EntityId,
    pub(crate) gpu_mesh_idx: usize,
    pub(crate) part_index: usize,
    pub(crate) model_matrix: Mat4,
    pub(crate) probe_layer: i32,
    /// Clave en `State::model_assets` para llegar a los vértices de bind pose.
    pub(crate) asset_path: String,
}

/// Piezas skinned visibles, **ordenadas** por (entidad, pieza).
///
/// El orden no es cosmético: `build_skinned_trace` numera los materiales siguiendo esta lista y
/// `refresh_skinned_rt_geometry` crea las instancias del TLAS en el mismo orden. Iterar
/// `model_animation_bindings` (un `HashMap`) dos veces puede dar órdenes distintos y desalinear
/// material con geometría, porque el hit shader indexa `mats[slot]` con el índice de instancia.
pub(crate) fn collect_skinned_trace_targets(
    state: &State,
    frustum_vp: &Mat4,
    probe_fields: &[crate::engine3d::probe_registry::ProbeField],
) -> Vec<SkinnedTraceTarget> {
    let mut out: Vec<SkinnedTraceTarget> = Vec::new();
    for (&id, binding) in &state.model_animation_bindings {
        let Some(transform) = state.world.get::<crate::engine3d::ecs::Transform>(id) else {
            continue;
        };
        if state.sun_entity == Some(id) {
            continue;
        }
        let model_matrix = transform.to_matrix();
        let (center, half) = state.entity_world_pick_aabb(id, transform);
        if !is_aabb_visible_3d(frustum_vp, center, half) {
            continue;
        }
        if !state.model_assets.contains_key(&binding.asset_path) {
            continue;
        }
        let probe_layer = crate::engine3d::probe_registry::select_probe_slot(probe_fields, center)
            .map(|slot| slot as i32)
            .unwrap_or(-1);
        for (part_index, &gpu_mesh_idx) in binding.part_gpu_indices.iter().enumerate() {
            if state
                .skinned_gpu_meshes
                .get(gpu_mesh_idx)
                .is_some_and(|entry| !entry.joint_palette.is_empty())
            {
                out.push(SkinnedTraceTarget {
                    entity_id: id,
                    gpu_mesh_idx,
                    part_index,
                    model_matrix,
                    probe_layer,
                    asset_path: binding.asset_path.clone(),
                });
            }
        }
    }
    out.sort_by_key(|target| (target.entity_id, target.part_index));
    out
}

/// Deforma en CPU las mallas skinned visibles y devuelve sus triángulos en espacio mundo con sus
/// materiales.
///
/// Usa la misma fórmula que `shaders/raster/skinned.vert`
/// (`model * (Σ pesos·palette[joint] · posición)`), así la silueta que trazan el BVH y el RT
/// coincide con la rasterizada. Los materiales se numeran desde 0: `build_rt_scene` les suma el
/// desplazamiento de los lotes estáticos.
pub(crate) fn build_skinned_trace(
    state: &State,
    targets: &[SkinnedTraceTarget],
) -> (Vec<crate::engine3d::bvh::RtTriangle>, Vec<RtMaterialGpu>) {
    let mut triangles: Vec<crate::engine3d::bvh::RtTriangle> = Vec::new();
    let mut materials: Vec<RtMaterialGpu> = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        // Un material por objetivo, siempre y en este orden: `refresh_skinned_rt_geometry` numera
        // sus instancias con el índice del objetivo, y un hueco aquí desalinearía materiales.
        let record = RtInstanceRecord {
            mesh_idx: 0,
            entity_id: target.entity_id,
            transform: target.model_matrix,
            probe_layer: target.probe_layer,
        };
        materials.push(rt_material(state, &record));
        let Some(asset) = state.model_assets.get(&target.asset_path) else {
            continue;
        };
        let (Some(entry), Some(part)) = (
            state.skinned_gpu_meshes.get(target.gpu_mesh_idx),
            asset.parts.get(target.part_index),
        ) else {
            continue;
        };
        if triangles.len() >= SKINNED_TRACE_MAX_TRIANGLES {
            return (triangles, materials);
        }
        let palette: Vec<Mat4> = entry
            .joint_palette
            .iter()
            .map(Mat4::from_cols_array_2d)
            .collect();
        if palette.is_empty() {
            continue;
        }
        let vertices = &part.mesh.vertices;
        for chunk in part.mesh.indices.chunks_exact(3) {
            let mut tri = crate::engine3d::bvh::RtTriangle {
                v0: GlamVec3::ZERO,
                v1: GlamVec3::ZERO,
                v2: GlamVec3::ZERO,
                uv0: [0.0; 2],
                uv1: [0.0; 2],
                uv2: [0.0; 2],
                material_slot: index as u32,
            };
            let mut ok = true;
            for (out_pos, out_uv, src_uv) in [
                (&mut tri.v0, &mut tri.uv0, 0usize),
                (&mut tri.v1, &mut tri.uv1, 1usize),
                (&mut tri.v2, &mut tri.uv2, 2usize),
            ] {
                let index = chunk[src_uv] as usize;
                let Some(vertex) = vertices.get(index) else {
                    ok = false;
                    break;
                };
                let local = crate::engine3d::config_3d::model_asset::skinned_vertex_position(
                    &palette, vertex,
                );
                *out_pos = target.model_matrix.transform_point3(local);
                *out_uv = vertex.uv;
            }
            if ok {
                triangles.push(tri);
            }
        }
    }
    (triangles, materials)
}

/// Escribe en GPU las posiciones locales ya poseadas de cada pieza skinned y prepara las
/// instancias para el TLAS (camino de hardware).
///
/// Devuelve `true` si este frame reescribió alguna pose: solo entonces hay que reconstruir esos
/// BLAS, porque el resto de frames se reutilizan los cacheados en `FrameRenderer`. Sin esto, el
/// personaje animado no aparecía en los reflejos trazados por hardware (solo en el BVH).
///
/// El buffer guarda solo posiciones (`stride` 12 bytes) y conserva el orden de vértices del VBO
/// skinned, así se reutiliza su propio índice. `material_slot` sigue el orden de `targets`, el
/// mismo que usó `build_skinned_trace`.
pub(crate) fn refresh_skinned_rt_geometry(
    state: &mut State,
    targets: &[SkinnedTraceTarget],
) -> bool {
    let material_count = state.rt_skinned_materials.len();
    let mut pending: Vec<(usize, Vec<u8>, u32, usize)> = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        // Sin material asignado no hay instancia posible: `build_skinned_trace` pudo truncar la
        // lista por el tope de triángulos.
        if index >= material_count {
            break;
        }
        let Some(asset) = state.model_assets.get(&target.asset_path) else {
            continue;
        };
        let (Some(entry), Some(part)) = (
            state.skinned_gpu_meshes.get(target.gpu_mesh_idx),
            asset.parts.get(target.part_index),
        ) else {
            continue;
        };
        let palette: Vec<Mat4> = entry
            .joint_palette
            .iter()
            .map(Mat4::from_cols_array_2d)
            .collect();
        if palette.is_empty() || part.mesh.indices.len() < 3 {
            continue;
        }
        let mut bytes = Vec::with_capacity(part.mesh.vertices.len() * 12);
        for vertex in &part.mesh.vertices {
            let posed =
                crate::engine3d::config_3d::model_asset::skinned_vertex_position(&palette, vertex);
            bytes.extend_from_slice(bytemuck::bytes_of(&posed.to_array()));
        }
        if bytes.is_empty() {
            continue;
        }
        let vertex_count = (bytes.len() / 12) as u32;
        pending.push((target.gpu_mesh_idx, bytes, vertex_count, index));
    }

    let mut instances: Vec<crate::engine3d::bvh::SkinnedRtInstance> =
        Vec::with_capacity(pending.len());
    let mut refreshed = false;
    for (gpu_mesh_idx, bytes, vertex_count, order) in pending {
        let needed = bytes.len() as u64;
        let must_create = match state.rt_skinned_vertices.get(&gpu_mesh_idx) {
            Some(buffer) => buffer.size < needed,
            None => true,
        };
        if must_create {
            match state
                .vulkan
                .create_trace_vertex_buffer(needed, "rt-skinned-vertices")
            {
                Ok(buffer) => {
                    state.rt_skinned_vertices.insert(gpu_mesh_idx, buffer);
                }
                Err(error) => {
                    // Sin buffer no hay instancia: el frame sigue dibujando, solo pierde ese
                    // reflejo. Se avisa una vez por pieza.
                    log::warn!(
                        "[reflexiones] sin buffer de pose para la pieza skinned {gpu_mesh_idx}: {error}"
                    );
                    continue;
                }
            }
        }
        let Some(buffer) = state.rt_skinned_vertices.get_mut(&gpu_mesh_idx) else {
            continue;
        };
        if let Err(error) = buffer.write(0, &bytes) {
            log::warn!(
                "[reflexiones] no se pudo subir la pose de la pieza {gpu_mesh_idx}: {error}"
            );
            continue;
        }
        let Some(target) = targets.get(order) else {
            continue;
        };
        instances.push(crate::engine3d::bvh::SkinnedRtInstance {
            gpu_mesh_idx,
            transform: mat4_to_tlas_transform(target.model_matrix),
            vertex_count,
            // El índice del objetivo = su posición en `rt_skinned_materials`.
            material_slot: order as u32,
        });
        refreshed = true;
    }
    state.rt_skinned_instances = instances;
    refreshed
}
