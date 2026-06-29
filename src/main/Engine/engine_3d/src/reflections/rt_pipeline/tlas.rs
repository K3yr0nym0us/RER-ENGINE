//! TLAS v2: instancias estáticas y skinned para RT v2.

use glam::{Mat4, Vec3};

use crate::ecs::{MeshComponent, Transform};
use crate::engine::State;
use crate::mesh::Mesh;
use super::bvh::RtTriangle;

pub const MAX_STATIC_RT_INSTANCES: usize = 512;
pub const MAX_SKINNED_RT_INSTANCES: usize = 32;

/// Descriptor de instancia para BLAS/TLAS/BVH.
#[derive(Clone, Copy, Debug)]
pub struct RtInstanceDesc {
    pub mesh_idx: Option<usize>,
    pub skinned_gpu_idx: Option<usize>,
    pub transform: Mat4,
    pub entity_id: u32,
}

impl RtInstanceDesc {
    pub fn static_mesh(mesh_idx: usize, transform: Mat4, entity_id: u32) -> Self {
        Self {
            mesh_idx: Some(mesh_idx),
            skinned_gpu_idx: None,
            transform,
            entity_id,
        }
    }

    pub fn skinned(skinned_gpu_idx: usize, transform: Mat4, entity_id: u32) -> Self {
        Self {
            mesh_idx: None,
            skinned_gpu_idx: Some(skinned_gpu_idx),
            transform,
            entity_id,
        }
    }
}

fn rt_entity_visible(
    state: &State,
    entity: crate::ecs::EntityId,
    transform: &Transform,
    _frustum_vp: &Mat4,
    is_player: bool,
) -> bool {
    let is_ground = state
        .world
        .get::<crate::ecs::NameComponent>(entity)
        .is_some_and(|n| n.name.eq_ignore_ascii_case("ground"));
    if is_ground {
        return true;
    }
    if is_player {
        return true;
    }
    let (mesh_center, mesh_half) = state.entity_world_pick_aabb(entity, transform);
    if !state
        .world_bounds_3d
        .intersects_world_aabb(mesh_center, mesh_half)
    {
        return false;
    }
    true
}

/// Recolecta instancias RT (estáticas y opcionalmente skinned), con culling de frustum/mundo.
pub fn collect_rt_instances(
    state: &State,
    include_skinned: bool,
    frustum_vp: &Mat4,
) -> Vec<RtInstanceDesc> {
    let mut out = Vec::new();
    let mut static_truncated = false;
    let mut skinned_truncated = false;
    for &entity in state.world.entities() {
        if state.play_character_entity == Some(entity) && !include_skinned {
            continue;
        }
        let Some(t) = state.world.get::<Transform>(entity) else {
            continue;
        };
        let is_player = state.play_character_entity == Some(entity);
        if !rt_entity_visible(state, entity, t, frustum_vp, is_player) {
            continue;
        }
        let transform = t.to_matrix();

        if include_skinned {
            if let Some(binding) = state.model_animation_bindings.get(&entity) {
                if let Some(&gpu_idx) = binding.part_gpu_indices.first() {
                    out.push(RtInstanceDesc::skinned(gpu_idx, transform, entity));
                    if out.len() >= MAX_STATIC_RT_INSTANCES + MAX_SKINNED_RT_INSTANCES {
                        skinned_truncated = true;
                        break;
                    }
                    continue;
                }
            }
        }

        if state.model_animation_bindings.contains_key(&entity) {
            continue;
        }
        let Some(mesh_comp) = state.world.get::<MeshComponent>(entity) else {
            continue;
        };
        if mesh_comp.mesh_idx >= state.meshes.len() {
            continue;
        }
        out.push(RtInstanceDesc::static_mesh(
            mesh_comp.mesh_idx,
            transform,
            entity,
        ));
        if out.len() >= MAX_STATIC_RT_INSTANCES {
            static_truncated = true;
            break;
        }
    }
    if static_truncated {
        log::warn!(
            "[RT] Límite de instancias estáticas alcanzado ({MAX_STATIC_RT_INSTANCES})"
        );
    }
    if skinned_truncated {
        log::warn!(
            "[RT] Límite de instancias skinned alcanzado ({MAX_SKINNED_RT_INSTANCES})"
        );
    }
    out
}

pub fn mesh_triangles_world(mesh: &Mesh, transform: Mat4) -> Vec<RtTriangle> {
    let mut tris = Vec::new();
    let idx = &mesh.rt_indices;
    let pos = &mesh.rt_positions;
    let uvs = &mesh.rt_uvs;
    if idx.len() < 3 || pos.is_empty() {
        return tris;
    }
    for chunk in idx.chunks(3) {
        if chunk.len() < 3 {
            break;
        }
        let i0 = chunk[0] as usize;
        let i1 = chunk[1] as usize;
        let i2 = chunk[2] as usize;
        if i0 >= pos.len() || i1 >= pos.len() || i2 >= pos.len() {
            continue;
        }
        let uv0 = uvs.get(i0).copied().unwrap_or([0.0, 0.0]);
        let uv1 = uvs.get(i1).copied().unwrap_or([0.0, 0.0]);
        let uv2 = uvs.get(i2).copied().unwrap_or([0.0, 0.0]);
        let v0 = transform.transform_point3(Vec3::from_array(pos[i0]));
        let v1 = transform.transform_point3(Vec3::from_array(pos[i1]));
        let v2 = transform.transform_point3(Vec3::from_array(pos[i2]));
        tris.push(RtTriangle {
            v0,
            v1,
            v2,
            uv0,
            uv1,
            uv2,
            instance_slot: 0,
        });
    }
    tris
}

/// Mat4 (glam column-major) → transform 3×4 row-major para `TlasInstance`.
pub fn mat4_to_tlas_transform(m: Mat4) -> [f32; 12] {
    let c = m.to_cols_array();
    [
        c[0], c[4], c[8], c[12],
        c[1], c[5], c[9], c[13],
        c[2], c[6], c[10], c[14],
    ]
}
