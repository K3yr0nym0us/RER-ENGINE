//! Deformación CPU de mallas skinned para RT v2 (BVH mundo).

use glam::{Mat4, Vec3};

use super::bvh::RtTriangle;
use crate::config_3d::model_animation::GpuSkinnedMeshEntry;
use crate::config_3d::model_asset::MAX_JOINTS;
use crate::mesh::SkinnedVertex;

fn skin_matrix(joints: [u32; 4], weights: [f32; 4], palette: &[[[f32; 4]; 4]]) -> Mat4 {
    let j0 = joints[0].min((MAX_JOINTS - 1) as u32) as usize;
    let j1 = joints[1].min((MAX_JOINTS - 1) as u32) as usize;
    let j2 = joints[2].min((MAX_JOINTS - 1) as u32) as usize;
    let j3 = joints[3].min((MAX_JOINTS - 1) as u32) as usize;
    Mat4::from_cols_array_2d(&palette[j0]) * weights[0]
        + Mat4::from_cols_array_2d(&palette[j1]) * weights[1]
        + Mat4::from_cols_array_2d(&palette[j2]) * weights[2]
        + Mat4::from_cols_array_2d(&palette[j3]) * weights[3]
}

fn deform_vertex(v: &SkinnedVertex, palette: &[[[f32; 4]; 4]]) -> Vec3 {
    let skin = skin_matrix(v.joints, v.weights, palette);
    let p = skin * Vec3::from_array(v.position).extend(1.0);
    p.truncate()
}

/// Posiciones locales deformadas (sin transform de entidad) para BLAS skinned.
pub fn skinned_local_positions(entry: &GpuSkinnedMeshEntry) -> Vec<[f32; 3]> {
    let palette = entry.joint_palette();
    if palette.is_empty() {
        return Vec::new();
    }
    entry
        .rt_mesh
        .vertices
        .iter()
        .map(|v| deform_vertex(v, palette).to_array())
        .collect()
}

/// Triángulos mundo de una pieza skinned con la pose actual.
pub fn skinned_mesh_triangles_world(
    entry: &GpuSkinnedMeshEntry,
    transform: Mat4,
) -> Vec<RtTriangle> {
    let palette = entry.joint_palette();
    if palette.is_empty() {
        return Vec::new();
    }
    let mesh = &entry.rt_mesh;
    let mut tris = Vec::new();
    for chunk in mesh.indices.chunks(3) {
        if chunk.len() < 3 {
            break;
        }
        let i0 = chunk[0] as usize;
        let i1 = chunk[1] as usize;
        let i2 = chunk[2] as usize;
        let Some(v0) = mesh.vertices.get(i0) else {
            continue;
        };
        let Some(v1) = mesh.vertices.get(i1) else {
            continue;
        };
        let Some(v2) = mesh.vertices.get(i2) else {
            continue;
        };
        let w0 = transform.transform_point3(deform_vertex(v0, palette));
        let w1 = transform.transform_point3(deform_vertex(v1, palette));
        let w2 = transform.transform_point3(deform_vertex(v2, palette));
        tris.push(RtTriangle {
            v0: w0,
            v1: w1,
            v2: w2,
            uv0: v0.uv,
            uv1: v1.uv,
            uv2: v2.uv,
            instance_slot: 0,
        });
    }
    tris
}
