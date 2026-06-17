//! Overlay de esqueleto (líneas padre→hijo) — editor sockets / selección de huesos.

use glam::{Mat4, Vec3};

use crate::config_3d::model_asset::{ModelAsset, MAX_JOINTS};
use crate::engine::State;
use crate::gizmo::{self, GizmoVertex};

const COLOR_BONE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const COLOR_BONE_HOVER: [f32; 4] = [1.0, 0.85, 0.15, 1.0];
const COLOR_ROOT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const BONE_CROSS_SIZE: f32 = 0.075;
const BONE_HOVER_CROSS_SIZE: f32 = 0.12;

fn push_line(verts: &mut Vec<GizmoVertex>, a: Vec3, b: Vec3, color: [f32; 4]) {
    let dir = b - a;
    let len = dir.length();
    if len < 1e-6 {
        return;
    }
    let n = dir / len;
    let perp = if n.y.abs() < 0.9 {
        n.cross(Vec3::Y).normalize()
    } else {
        n.cross(Vec3::X).normalize()
    };
    let thick = 0.008;
    for offset in [-thick, 0.0, thick] {
        let o = perp * offset;
        verts.push(GizmoVertex {
            position: (a + o).to_array(),
            color,
        });
        verts.push(GizmoVertex {
            position: (b + o).to_array(),
            color,
        });
    }
}

fn push_joint_cross(verts: &mut Vec<GizmoVertex>, p: Vec3, color: [f32; 4], size: f32) {
    push_line(verts, p + Vec3::X * size, p - Vec3::X * size, color);
    push_line(verts, p + Vec3::Y * size, p - Vec3::Y * size, color);
    push_line(verts, p + Vec3::Z * size, p - Vec3::Z * size, color);
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
    let id = state
        .socket_bone_pick_entity
        .or(state.selected_entity)
        .unwrap_or(0);
    if id == 0 {
        return gizmo::build_from_vertices(device, &[]);
    }
    let hovered = if state.socket_bone_pick_entity == Some(id) {
        state.socket_bone_pick_hovered_joint
    } else {
        None
    };
    build_entity_skeleton_overlay(device, state, id, hovered)
}

fn push_skeleton_lines(
    verts: &mut Vec<GizmoVertex>,
    asset: &ModelAsset,
    globals: &[Mat4],
    entity_model: Mat4,
    hovered_joint: Option<usize>,
) {
    let joint_positions = joint_positions_mesh_space(asset, globals);
    let joint_count = asset
        .joint_names
        .len()
        .min(joint_positions.len())
        .min(MAX_JOINTS);

    if joint_count == 0 {
        return;
    }

    if !asset.joint_gltf_nodes.is_empty() {
        let gltf_count = asset.joint_gltf_nodes.len().min(joint_count);
        let mut has_parent = vec![false; gltf_count];
        for ji in 0..gltf_count {
            let is_hover = hovered_joint == Some(ji);
            let line_color = if is_hover {
                COLOR_BONE_HOVER
            } else {
                COLOR_BONE
            };
            let Some(pi) = skeleton_parent_index(
                &asset.joint_gltf_nodes[..gltf_count],
                &asset.gltf_scene_parents,
                ji,
            ) else {
                let wp = entity_model.transform_point3(joint_positions[ji]);
                let size = if is_hover {
                    BONE_HOVER_CROSS_SIZE
                } else {
                    BONE_CROSS_SIZE
                };
                push_joint_cross(verts, wp, line_color, size);
                continue;
            };
            has_parent[ji] = true;
            let a = entity_model.transform_point3(joint_positions[pi]);
            let b = entity_model.transform_point3(joint_positions[ji]);
            push_line(verts, a, b, line_color);
            if is_hover {
                push_joint_cross(verts, b, COLOR_BONE_HOVER, BONE_HOVER_CROSS_SIZE);
            }
        }
        for ji in 0..gltf_count {
            if !has_parent[ji] {
                let is_hover = hovered_joint == Some(ji);
                let wp = entity_model.transform_point3(joint_positions[ji]);
                let size = if is_hover {
                    BONE_HOVER_CROSS_SIZE
                } else {
                    BONE_CROSS_SIZE
                };
                let color = if is_hover {
                    COLOR_BONE_HOVER
                } else {
                    COLOR_ROOT
                };
                push_joint_cross(verts, wp, color, size);
            }
        }
        return;
    }

    for ji in 0..joint_count {
        let is_hover = hovered_joint == Some(ji);
        let line_color = if is_hover {
            COLOR_BONE_HOVER
        } else {
            COLOR_BONE
        };
        let wp = entity_model.transform_point3(joint_positions[ji]);
        let size = if is_hover {
            BONE_HOVER_CROSS_SIZE
        } else {
            BONE_CROSS_SIZE
        };
        push_joint_cross(verts, wp, line_color, size);
        if let Some(pi) = asset.joint_parents.get(ji).and_then(|p| *p) {
            if pi < joint_count {
                let a = entity_model.transform_point3(joint_positions[pi]);
                push_line(verts, a, wp, line_color);
            }
        }
    }
}

pub(crate) fn build_entity_skeleton_overlay(
    device: &wgpu::Device,
    state: &State,
    entity_id: u32,
    hovered_joint: Option<usize>,
) -> gizmo::GizmoBuffer {
    let Some((asset, globals, entity_model)) = state.entity_skeleton_globals(entity_id) else {
        return gizmo::build_from_vertices(device, &[]);
    };

    let mut verts = Vec::new();
    push_skeleton_lines(
        &mut verts,
        &asset,
        &globals,
        entity_model,
        hovered_joint,
    );

    gizmo::build_from_vertices(device, &verts)
}
