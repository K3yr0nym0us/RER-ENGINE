//! Overlay de esqueleto (líneas padre→hijo) sin skinning — depuración IBM / espacio.

use glam::{Mat4, Vec3};

use crate::config_3d::model_asset::{ModelAsset, MAX_JOINTS, compute_gltf_joint_worlds};
use crate::config_3d::model_animation::asset_joint_globals_bind;
use crate::ecs::Transform;
use crate::engine::State;
use crate::gizmo::{self, GizmoVertex};

const COLOR_BONE: [f32; 4] = [0.15, 1.0, 0.45, 1.0];
const COLOR_BONE_HIER: [f32; 4] = [1.0, 0.35, 0.15, 0.85];
const COLOR_ROOT: [f32; 4] = [1.0, 0.92, 0.2, 1.0];
const COLOR_ROOT_HIER: [f32; 4] = [1.0, 0.5, 0.1, 0.85];

fn push_line(verts: &mut Vec<GizmoVertex>, a: Vec3, b: Vec3, color: [f32; 4]) {
    verts.push(GizmoVertex {
        position: a.to_array(),
        color,
    });
    verts.push(GizmoVertex {
        position: b.to_array(),
        color,
    });
}

fn push_joint_cross(verts: &mut Vec<GizmoVertex>, p: Vec3, color: [f32; 4]) {
    let s = 0.04_f32;
    push_line(verts, p + Vec3::X * s, p - Vec3::X * s, color);
    push_line(verts, p + Vec3::Y * s, p - Vec3::Y * s, color);
    push_line(verts, p + Vec3::Z * s, p - Vec3::Z * s, color);
}

fn skeleton_parent_index(
    joint_gltf_nodes: &[usize],
    scene_parents: &std::collections::HashMap<usize, usize>,
    ji: usize,
) -> Option<usize> {
    let mut cur = joint_gltf_nodes[ji];
    for _ in 0..512 {
        let parent_node = scene_parents.get(&cur)?;
        if let Some(pi) = joint_gltf_nodes.iter().position(|&n| n == *parent_node) {
            return Some(pi);
        }
        cur = *parent_node;
    }
    None
}

/// Posiciones de joints en el mismo espacio que la malla dibujada (post-`mesh_normalize`).
fn joint_positions_mesh_space(asset: &ModelAsset, globals: &[Mat4]) -> Vec<Vec3> {
    let norm = asset.mesh_normalize;
    globals
        .iter()
        .map(|g| norm.transform_point3(g.transform_point3(Vec3::ZERO)))
        .collect()
}

pub(crate) fn build_selected_skeleton_overlay(
    device: &wgpu::Device,
    state: &State,
) -> gizmo::GizmoBuffer {
    let Some(id) = state.selected_entity else {
        return gizmo::build_from_vertices(device, &[]);
    };
    build_entity_skeleton_overlay(device, state, id)
}

fn push_skeleton_lines(
    verts: &mut Vec<GizmoVertex>,
    asset: &ModelAsset,
    globals: &[Mat4],
    entity_model: Mat4,
    bone_color: [f32; 4],
    root_color: [f32; 4],
) {
    let joint_positions = joint_positions_mesh_space(asset, globals);
    let joint_count = asset
        .joint_gltf_nodes
        .len()
        .min(joint_positions.len())
        .min(MAX_JOINTS);

    let mut has_parent = vec![false; joint_count];

    for ji in 0..joint_count {
        let Some(pi) = skeleton_parent_index(
            &asset.joint_gltf_nodes[..joint_count],
            &asset.gltf_scene_parents,
            ji,
        ) else {
            let wp = entity_model.transform_point3(joint_positions[ji]);
            push_joint_cross(verts, wp, root_color);
            continue;
        };
        has_parent[ji] = true;
        let a = entity_model.transform_point3(joint_positions[pi]);
        let b = entity_model.transform_point3(joint_positions[ji]);
        push_line(verts, a, b, bone_color);
    }

    for ji in 0..joint_count {
        if !has_parent[ji] {
            let wp = entity_model.transform_point3(joint_positions[ji]);
            push_joint_cross(verts, wp, root_color);
        }
    }
}

pub(crate) fn build_entity_skeleton_overlay(
    device: &wgpu::Device,
    state: &State,
    entity_id: u32,
) -> gizmo::GizmoBuffer {
    let Some(binding) = state.model_animation_bindings.get(&entity_id) else {
        return gizmo::build_from_vertices(device, &[]);
    };
    let Some(asset) = state.get_model_asset_for_entity(&binding.asset_path, entity_id) else {
        return gizmo::build_from_vertices(device, &[]);
    };
    let Some(t) = state.world.get::<Transform>(entity_id) else {
        return gizmo::build_from_vertices(device, &[]);
    };

    let entity_model = t.to_matrix();

    let runtime_globals = asset_joint_globals_bind(&asset);
    let mut verts = Vec::new();
    push_skeleton_lines(
        &mut verts,
        &asset,
        &runtime_globals,
        entity_model,
        COLOR_BONE,
        COLOR_ROOT,
    );

    if asset.bind_pose_from_ibm && !asset.joint_gltf_nodes.is_empty() {
        let joint_count = asset
            .joint_gltf_nodes
            .len()
            .min(asset.bind_local.len())
            .min(MAX_JOINTS);
        let hierarchy_globals = compute_gltf_joint_worlds(
            &asset.joint_gltf_nodes[..joint_count],
            &asset.bind_local[..joint_count],
            &asset.gltf_scene_parents,
            &asset.gltf_bind_node_local,
        );
        push_skeleton_lines(
            &mut verts,
            &asset,
            &hierarchy_globals,
            entity_model,
            COLOR_BONE_HIER,
            COLOR_ROOT_HIER,
        );
    }

    gizmo::build_from_vertices(device, &verts)
}
