//! TLAS v2: instancias estáticas y skinned para RT v2.

use glam::{Mat4, Vec3};

use crate::config_3d::model_animation::GpuSkinnedMeshEntry;
use crate::ecs::{MeshComponent, Transform};
use crate::engine::State;
use crate::mesh::Mesh;
use crate::reflections::bvh::RtTriangle;
use crate::reflections::skinned_rt::skinned_mesh_triangles_world;

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

/// Recolecta instancias RT (estáticas y opcionalmente skinned).
pub fn collect_rt_instances(state: &State, include_skinned: bool) -> Vec<RtInstanceDesc> {
    let mut out = Vec::new();
    for &entity in state.world.entities() {
        if state.play_character_entity == Some(entity) && !include_skinned {
            continue;
        }
        let Some(t) = state.world.get::<Transform>(entity) else {
            continue;
        };
        let transform = t.to_matrix();

        if include_skinned {
            if let Some(binding) = state.model_animation_bindings.get(&entity) {
                if let Some(&gpu_idx) = binding.part_gpu_indices.first() {
                    out.push(RtInstanceDesc::skinned(gpu_idx, transform, entity));
                    if out.len() >= MAX_STATIC_RT_INSTANCES + MAX_SKINNED_RT_INSTANCES {
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
            break;
        }
    }
    out
}

pub fn collect_static_rt_instances(state: &State) -> Vec<RtInstanceDesc> {
    collect_rt_instances(state, false)
}

pub fn mesh_triangles_world(mesh: &Mesh, transform: Mat4) -> Vec<RtTriangle> {
    let mut tris = Vec::new();
    let idx = &mesh.rt_indices;
    let pos = &mesh.rt_positions;
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
        let v0 = transform.transform_point3(Vec3::from_array(pos[i0]));
        let v1 = transform.transform_point3(Vec3::from_array(pos[i1]));
        let v2 = transform.transform_point3(Vec3::from_array(pos[i2]));
        tris.push(RtTriangle {
            v0,
            v1,
            v2,
            instance_slot: 0,
        });
    }
    tris
}

pub fn instance_triangles_world(
    meshes: &[Mesh],
    skinned_meshes: &[GpuSkinnedMeshEntry],
    inst: &RtInstanceDesc,
) -> Vec<RtTriangle> {
    if let Some(mesh_idx) = inst.mesh_idx {
        let Some(mesh) = meshes.get(mesh_idx) else {
            return Vec::new();
        };
        return mesh_triangles_world(mesh, inst.transform);
    }
    if let Some(gpu_idx) = inst.skinned_gpu_idx {
        let Some(entry) = skinned_meshes.get(gpu_idx) else {
            return Vec::new();
        };
        return skinned_mesh_triangles_world(entry, inst.transform);
    }
    Vec::new()
}

pub fn instance_triangles_world_state(state: &State, inst: &RtInstanceDesc) -> Vec<RtTriangle> {
    instance_triangles_world(&state.meshes, &state.skinned_gpu_meshes, inst)
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
