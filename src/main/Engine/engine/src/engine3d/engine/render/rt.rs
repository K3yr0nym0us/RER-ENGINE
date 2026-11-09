//! Recolección de instancias RT, hash de escena y construcción del TLAS.

use super::*;

/// Instancia trazable (una entidad), compartida por el BVH software y el TLAS hardware.
#[derive(Clone, Copy)]
pub(crate) struct RtInstanceRecord {
    pub(crate) mesh_idx: usize,
    pub(crate) entity_id: crate::engine3d::ecs::EntityId,
    pub(crate) transform: Mat4,
    pub(crate) probe_layer: i32,
}

pub(crate) fn collect_rt_instances(
    state: &State,
    batches: &[SceneInstanceBatch],
) -> Vec<RtInstanceRecord> {
    let mut out = Vec::new();
    for batch in batches {
        let Some(mesh) = state.meshes.get(batch.mesh_idx) else {
            continue;
        };
        if mesh.rt_indices.len() < 3 {
            continue;
        }
        for (inst, entity_id) in batch.instances.iter().zip(batch.entity_ids.iter()) {
            out.push(RtInstanceRecord {
                mesh_idx: batch.mesh_idx,
                entity_id: *entity_id,
                transform: Mat4::from_cols_array_2d(&inst.model),
                probe_layer: inst.tex_layer_pad[2] as i32,
            });
        }
    }
    out
}

pub(crate) fn rt_scene_hash(state: &State, records: &[RtInstanceRecord]) -> u64 {
    let mut hasher = DefaultHasher::new();
    records.len().hash(&mut hasher);
    for rec in records {
        rec.entity_id.hash(&mut hasher);
        rec.mesh_idx.hash(&mut hasher);
        rec.probe_layer.hash(&mut hasher);
        for column in rec.transform.to_cols_array() {
            column.to_bits().hash(&mut hasher);
        }
        let tex_idx = state
            .world
            .get::<MeshComponent>(rec.entity_id)
            .map(|mesh| mesh.tex_idx)
            .unwrap_or(0);
        tex_idx.hash(&mut hasher);
        if let Some(pbr) = state.world.get::<SurfacePbr>(rec.entity_id) {
            pbr.roughness.to_bits().hash(&mut hasher);
            pbr.metallic.to_bits().hash(&mut hasher);
            pbr.ior.to_bits().hash(&mut hasher);
            pbr.opacity.to_bits().hash(&mut hasher);
        }
    }
    // La geometría skinned no está en `records` (se deforma en CPU y se cachea), así que su
    // refresco entra en el hash por tick: el BVH se reconstruye cuando cambia la pose cacheada y
    // no en los frames intermedios.
    if !state.rt_skinned_triangles.is_empty() {
        (state.rt_skinned_tick / SKINNED_TRACE_REFRESH_FRAMES.max(1)).hash(&mut hasher);
    }
    hasher.finish()
}

pub(crate) fn rt_material(state: &State, rec: &RtInstanceRecord) -> RtMaterialGpu {
    let tex_idx = state
        .world
        .get::<MeshComponent>(rec.entity_id)
        .map(|mesh| mesh.tex_idx)
        .unwrap_or(0);
    let albedo = state
        .tex_layer_albedo
        .get(tex_idx)
        .copied()
        .unwrap_or([0.75, 0.75, 0.75]);
    let (roughness, metallic, ior, dielectric) =
        if let Some(pbr) = state.world.get::<SurfacePbr>(rec.entity_id) {
            (pbr.roughness, pbr.metallic, pbr.ior.max(0.0), pbr.ior > 1.0)
        } else {
            (0.5, 0.0, 1.5, false)
        };
    RtMaterialGpu {
        albedo: [
            albedo[0],
            albedo[1],
            albedo[2],
            f32::from_bits(u32::from(dielectric)),
        ],
        pbr: [roughness, metallic, ior, tex_idx as f32],
        probe: [rec.probe_layer as f32, 0.0, 0.0, 0.0],
    }
}

/// Triángulos en espacio mundo + materiales por instancia → BVH y buffers GPU.
pub(crate) fn build_rt_scene(state: &State, records: &[RtInstanceRecord]) -> RtSceneBlob {
    let mut triangles: Vec<RtTriangle> = Vec::new();
    let mut materials: Vec<RtMaterialGpu> = Vec::with_capacity(records.len());
    for (slot, rec) in records.iter().enumerate() {
        let slot = slot as u32;
        materials.push(rt_material(state, rec));
        let Some(mesh) = state.meshes.get(rec.mesh_idx) else {
            continue;
        };
        let positions = &mesh.rt_positions;
        let uvs = &mesh.rt_uvs;
        for chunk in mesh.rt_indices.chunks_exact(3) {
            let (i0, i1, i2) = (chunk[0] as usize, chunk[1] as usize, chunk[2] as usize);
            if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
                continue;
            }
            let v0 = rec
                .transform
                .transform_point3(GlamVec3::from_array(positions[i0]));
            let v1 = rec
                .transform
                .transform_point3(GlamVec3::from_array(positions[i1]));
            let v2 = rec
                .transform
                .transform_point3(GlamVec3::from_array(positions[i2]));
            triangles.push(RtTriangle {
                v0,
                v1,
                v2,
                uv0: uvs.get(i0).copied().unwrap_or([0.0, 0.0]),
                uv1: uvs.get(i1).copied().unwrap_or([0.0, 0.0]),
                uv2: uvs.get(i2).copied().unwrap_or([0.0, 0.0]),
                material_slot: slot,
            });
        }
    }
    // Geometría skinned ya deformada (pose del personaje en el último refresco). Sus materiales se
    // añadieron después de los de los lotes estáticos, así que se les corre el índice.
    let skinned_base = materials.len() as u32;
    for triangle in &state.rt_skinned_triangles {
        let mut triangle = *triangle;
        triangle.material_slot += skinned_base;
        triangles.push(triangle);
    }
    materials.extend(state.rt_skinned_materials.iter().copied());
    let (nodes, gpu_triangles) = crate::engine3d::bvh::build_bvh(triangles);
    RtSceneBlob::from_parts(&nodes, &gpu_triangles, &materials)
}

/// Mat4 (column-major) → transform 3×4 row-major que espera el TLAS.
pub(crate) fn mat4_to_tlas_transform(matrix: Mat4) -> [f32; 12] {
    let c = matrix.to_cols_array();
    [
        c[0], c[4], c[8], c[12], c[1], c[5], c[9], c[13], c[2], c[6], c[10], c[14],
    ]
}
