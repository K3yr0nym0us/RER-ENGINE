//! Materiales por instancia RT (Hit Lighting lite).

use bytemuck::{Pod, Zeroable};

use crate::config_3d::model_animation::GpuSkinnedMeshEntry;
use crate::ecs::{MeshComponent, SurfacePbr};
use crate::engine::State;
use crate::mesh::Mesh;
use crate::reflections::tlas::{RtInstanceDesc, MAX_SKINNED_RT_INSTANCES, MAX_STATIC_RT_INSTANCES};

pub const MAX_RT_MATERIALS: usize = MAX_STATIC_RT_INSTANCES + MAX_SKINNED_RT_INSTANCES;

pub const RT_MAT_FLAG_DIELECTRIC: u32 = 1;

/// GPU: albedo.xyz + flags en w (bit0 = dieléctrico).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct RtInstanceMaterialGpu {
    pub albedo: [f32; 4],
    /// x=roughness, y=metallic, z=ior, w=tex_layer
    pub pbr: [f32; 4],
    /// x=probe layer (-1 = auto nearest), yzw unused
    pub probe: [f32; 4],
}

impl RtInstanceMaterialGpu {
    pub fn from_entity(
        state: &State,
        entity_id: u32,
        tex_idx: usize,
        probe_layer: i32,
        albedo_rgb: [f32; 3],
    ) -> Self {
        let (roughness, metallic, ior, dielectric) = if let Some(pbr) = state.world.get::<SurfacePbr>(entity_id) {
            let dielectric = pbr.ior > 1.0;
            (pbr.roughness, pbr.metallic, pbr.ior.max(0.0), dielectric)
        } else {
            (0.5, 0.0, 0.0, false)
        };
        let mut flags = 0u32;
        if dielectric {
            flags |= RT_MAT_FLAG_DIELECTRIC;
        }
        Self {
            albedo: [albedo_rgb[0], albedo_rgb[1], albedo_rgb[2], f32::from_bits(flags)],
            pbr: [
                roughness,
                metallic,
                if ior > 1.0 { ior } else { 1.5 },
                tex_idx as f32,
            ],
            probe: [probe_layer as f32, 0.0, 0.0, 0.0],
        }
    }
}

fn albedo_for_tex_idx(state: &State, tex_idx: usize) -> [f32; 3] {
    if tex_idx == 0 {
        return [1.0, 1.0, 1.0];
    }
    state
        .tex_layer_albedo
        .get(tex_idx)
        .copied()
        .unwrap_or([1.0, 1.0, 1.0])
}

pub fn build_rt_materials(
    state: &State,
    instances: &[RtInstanceDesc],
    probe_index_map: &std::collections::HashMap<crate::ecs::EntityId, usize>,
) -> Vec<RtInstanceMaterialGpu> {
    let mut out = Vec::with_capacity(instances.len());
    for inst in instances.iter().take(MAX_RT_MATERIALS) {
        let probe_layer = probe_index_map
            .get(&inst.entity_id)
            .copied()
            .map(|i| i as i32)
            .unwrap_or(-1);
        let mat = if let Some(mesh_idx) = inst.mesh_idx {
            let tex_idx = state
                .world
                .get::<MeshComponent>(inst.entity_id)
                .map(|m| m.tex_idx)
                .unwrap_or(0);
            let _ = mesh_idx;
            let albedo = albedo_for_tex_idx(state, tex_idx);
            RtInstanceMaterialGpu::from_entity(state, inst.entity_id, tex_idx, probe_layer, albedo)
        } else if let Some(gpu_idx) = inst.skinned_gpu_idx {
            let tex_idx = state
                .model_animation_bindings
                .get(&inst.entity_id)
                .and_then(|b| b.part_tex_layers.first().copied())
                .map(|layer| {
                    state
                        .tex_layers
                        .iter()
                        .position(|&l| l == layer)
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            let _ = gpu_idx;
            let albedo = albedo_for_tex_idx(state, tex_idx);
            RtInstanceMaterialGpu::from_entity(state, inst.entity_id, tex_idx, probe_layer, albedo)
        } else {
            RtInstanceMaterialGpu::default()
        };
        out.push(mat);
    }
    out
}

pub fn instance_triangles_tagged(
    meshes: &[Mesh],
    skinned_meshes: &[GpuSkinnedMeshEntry],
    inst: &RtInstanceDesc,
    instance_slot: u32,
) -> Vec<crate::reflections::bvh::RtTriangle> {
    use crate::reflections::skinned_rt::skinned_mesh_triangles_world;
    use crate::reflections::tlas::mesh_triangles_world;

    let mut tris = if let Some(mesh_idx) = inst.mesh_idx {
        let Some(mesh) = meshes.get(mesh_idx) else {
            return Vec::new();
        };
        mesh_triangles_world(mesh, inst.transform)
    } else if let Some(gpu_idx) = inst.skinned_gpu_idx {
        let Some(entry) = skinned_meshes.get(gpu_idx) else {
            return Vec::new();
        };
        skinned_mesh_triangles_world(entry, inst.transform)
    } else {
        Vec::new()
    };
    for t in &mut tris {
        t.instance_slot = instance_slot;
    }
    tris
}