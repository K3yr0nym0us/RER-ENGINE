//! Bounds de play character, forward en XZ y normalización de vértices.

use super::*;

pub(crate) fn model_asset_bind_pose_bounds(asset: &ModelAsset) -> Option<([f32; 3], [f32; 3])> {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for part in &asset.parts {
        let Some((pmin, pmax)) = skinned_mesh_bounds(&part.mesh.vertices) else {
            continue;
        };
        any = true;
        for i in 0..3 {
            min[i] = min[i].min(pmin[i]);
            max[i] = max[i].max(pmax[i]);
        }
    }
    any.then_some((min, max))
}

pub(crate) fn apply_normalize_to_skinned_vertices(vertices: &mut [SkinnedVertex], norm: Mat4) {
    transform_skinned_vertices(vertices, norm);
}

pub(crate) fn transform_skinned_vertices(vertices: &mut [SkinnedVertex], transform: Mat4) {
    let normal_xform = transform.inverse().transpose();
    for v in vertices.iter_mut() {
        v.position = transform
            .transform_point3(Vec3::from_array(v.position))
            .to_array();
        let n = normal_xform
            .transform_vector3(Vec3::from_array(v.normal))
            .normalize_or_zero();
        if n.length_squared() > 1e-8 {
            v.normal = n.to_array();
        }
    }
}

pub(crate) fn skinned_vertex_position(palette: &[Mat4], vertex: &SkinnedVertex) -> Vec3 {
    let pos = Vec4::new(
        vertex.position[0],
        vertex.position[1],
        vertex.position[2],
        1.0,
    );
    let mut out = Vec4::ZERO;
    for slot in 0..4 {
        let w = vertex.weights[slot];
        if w <= 1e-8 {
            continue;
        }
        let ji = vertex.joints[slot] as usize;
        if ji >= palette.len() {
            continue;
        }
        out += palette[ji] * pos * w;
    }
    out.truncate()
}

/// AABB en bind pose deformado (glTF skinned), espacio local de entidad.
pub(crate) fn gltf_play_skinned_bind_pose_aabb(asset: &ModelAsset) -> Option<([f32; 3], [f32; 3])> {
    let joint_count = asset
        .bind_local
        .len()
        .min(MAX_JOINTS)
        .min(asset.joint_gltf_nodes.len());
    if joint_count == 0 {
        return None;
    }
    let locals = &asset.bind_local[..joint_count];
    let inv_norm = asset.mesh_normalize.inverse();
    let mut min_p = Vec3::splat(f32::MAX);
    let mut max_p = Vec3::splat(f32::MIN);
    let mut any = false;
    for part in &asset.parts {
        let global = gltf_skinned_part_globals_for_palette(asset, part, locals);
        let mut palette = vec![Mat4::IDENTITY; MAX_JOINTS];
        for ji in 0..joint_count.min(global.len()) {
            let g2b = Mat4::from_cols_array_2d(&part.inverse_bind[ji]);
            palette[ji] = asset.mesh_normalize * global[ji] * g2b * inv_norm;
        }
        for vertex in &part.mesh.vertices {
            let p = skinned_vertex_position(&palette, vertex);
            min_p = min_p.min(p);
            max_p = max_p.max(p);
            any = true;
        }
    }
    any.then_some(([min_p.x, min_p.y, min_p.z], [max_p.x, max_p.y, max_p.z]))
}

/// Centro del AABB en bind pose (jerarquía glTF, rigs con TRS en nodos).
pub(crate) fn gltf_play_bind_pose_aabb_center(
    parts: &[SkinnedMeshPart],
    joint_gltf_nodes: &[usize],
    bind_local: &[Mat4],
    scene_parents: &HashMap<usize, usize>,
    bind_node_local: &HashMap<usize, Mat4>,
    mesh_normalize: Mat4,
) -> Option<Vec3> {
    let joint_count = bind_local.len().min(MAX_JOINTS).min(joint_gltf_nodes.len());
    if joint_count == 0 {
        return None;
    }
    let global = compute_gltf_joint_worlds(
        &joint_gltf_nodes[..joint_count],
        bind_local,
        scene_parents,
        bind_node_local,
    );
    let inv_norm = mesh_normalize.inverse();
    let mut min_p = Vec3::splat(f32::MAX);
    let mut max_p = Vec3::splat(f32::MIN);
    let mut any = false;
    for part in parts {
        let mut palette = vec![Mat4::IDENTITY; MAX_JOINTS];
        for ji in 0..joint_count {
            let g2b = Mat4::from_cols_array_2d(&part.inverse_bind[ji]);
            palette[ji] = mesh_normalize * global[ji] * g2b * inv_norm;
        }
        for vertex in &part.mesh.vertices {
            let p = skinned_vertex_position(&palette, vertex);
            min_p = min_p.min(p);
            max_p = max_p.max(p);
            any = true;
        }
    }
    any.then_some((min_p + max_p) * 0.5)
}

/// AABB del jugador: bind pose skinned o vértices en reposo.
pub(crate) fn model_asset_play_character_visual_bounds(
    asset: &ModelAsset,
) -> Option<([f32; 3], [f32; 3])> {
    if !asset.joint_gltf_nodes.is_empty()
        && !asset.bind_local.is_empty()
        && let Some(bounds) = gltf_play_skinned_bind_pose_aabb(asset)
    {
        return Some(bounds);
    }
    model_asset_bind_pose_bounds(asset)
}

pub(crate) fn skinned_mesh_bounds(vertices: &[SkinnedVertex]) -> Option<([f32; 3], [f32; 3])> {
    if vertices.is_empty() {
        return None;
    }
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in vertices.iter() {
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
        }
    }
    Some((min, max))
}

// ── Orientación jugador FP (glTF/GLB) ────────────────────────────────────────

pub(crate) fn torso_vertex_indices(vertices: &[crate::engine3d::mesh::Vertex]) -> Vec<usize> {
    if vertices.is_empty() {
        return Vec::new();
    }
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in vertices {
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
        }
    }
    let h = (max[1] - min[1]).max(1e-5);
    let w = (max[0] - min[0]).max(1e-5);
    let cx = (min[0] + max[0]) * 0.5;
    let y_lo = min[1] + 0.18 * h;
    let y_hi = min[1] + 0.72 * h;
    let x_half = 0.42 * w;
    vertices
        .iter()
        .enumerate()
        .filter(|(_, v)| {
            let p = &v.position;
            p[1] >= y_lo && p[1] <= y_hi && (p[0] - cx).abs() <= x_half
        })
        .map(|(i, _)| i)
        .collect()
}

pub(crate) fn estimate_forward_from_normals(
    vertices: &[crate::engine3d::mesh::Vertex],
) -> glam::Vec2 {
    let mut sum = glam::Vec2::ZERO;
    for v in vertices {
        let n = glam::Vec2::new(v.normal[0], v.normal[2]);
        let len = n.length();
        if len > 1e-5 {
            sum += n / len;
        }
    }
    if sum.length_squared() < 1e-6 {
        glam::Vec2::new(0.0, 1.0)
    } else {
        sum.normalize()
    }
}

pub(crate) fn estimate_forward_torso(vertices: &[crate::engine3d::mesh::Vertex]) -> glam::Vec2 {
    if vertices.is_empty() {
        return glam::Vec2::new(0.0, 1.0);
    }
    let torso_ix = torso_vertex_indices(vertices);
    let positions: Vec<[f32; 3]> = if torso_ix.len() >= 64 {
        torso_ix.iter().map(|&i| vertices[i].position).collect()
    } else {
        vertices.iter().map(|v| v.position).collect()
    };
    crate::engine3d::config_3d::mesh_3d::estimate_forward_xz_from_positions(&positions)
}

pub(crate) fn find_hips_joint_index(asset: &ModelAsset) -> Option<usize> {
    asset
        .joint_names
        .iter()
        .position(|n| n.to_ascii_lowercase().contains("hips"))
}

pub(crate) fn skinned_forward_core_vertices(
    asset: &ModelAsset,
) -> Vec<crate::engine3d::mesh::Vertex> {
    use crate::engine3d::mesh::Vertex;
    let mut verts: Vec<Vertex> = asset
        .parts
        .iter()
        .filter(|p| part_counts_for_play_forward(&p.name))
        .flat_map(|p| &p.mesh.vertices)
        .map(|v| Vertex {
            position: v.position,
            normal: v.normal,
            uv: v.uv,
        })
        .collect();
    if verts.len() < 64 {
        verts = asset
            .parts
            .iter()
            .flat_map(|p| &p.mesh.vertices)
            .map(|v| Vertex {
                position: v.position,
                normal: v.normal,
                uv: v.uv,
            })
            .collect();
    }
    verts
}

pub(crate) fn estimate_skinned_forward_from_vertices(asset: &ModelAsset) -> glam::Vec2 {
    let verts = skinned_forward_core_vertices(asset);
    if verts.is_empty() {
        return asset.facing_forward_xz;
    }
    let est_pos = crate::engine3d::config_3d::mesh_3d::estimate_mesh_forward_xz(&verts);
    let est_norm = estimate_forward_from_normals(&verts);
    if asset.facing_forward_xz.dot(est_norm).abs() >= asset.facing_forward_xz.dot(est_pos).abs() {
        est_norm
    } else {
        est_pos
    }
}

pub(crate) fn estimate_skinned_forward_torso(asset: &ModelAsset) -> glam::Vec2 {
    let verts = skinned_forward_core_vertices(asset);
    if verts.is_empty() {
        return asset.facing_forward_xz;
    }
    estimate_forward_torso(&verts)
}

pub(crate) fn needs_skinned_forward_correction(asset: &ModelAsset) -> bool {
    let Some(hi) = find_hips_joint_index(asset) else {
        return false;
    };
    let (_, rot, _) = asset.bind_local[hi].to_scale_rotation_translation();
    let (bone_yaw, _, _) = rot.to_euler(glam::EulerRot::YXZ);
    bone_yaw.abs() >= 0.35
}

/// Compensa yaw del hueso raíz (Mixamo suele traer ~±90° en bind).
pub(crate) fn apply_hips_bind_yaw_to_forward(fwd: glam::Vec2, asset: &ModelAsset) -> glam::Vec2 {
    let Some(hi) = find_hips_joint_index(asset) else {
        return fwd;
    };
    let (_, rot, _) = asset.bind_local[hi].to_scale_rotation_translation();
    let (bone_yaw, _, _) = rot.to_euler(glam::EulerRot::YXZ);
    if bone_yaw.abs() < 0.35 {
        return fwd;
    }
    let (s, c) = (-bone_yaw).sin_cos();
    let fx = fwd.x;
    let fz = fwd.y;
    glam::Vec2::new(fx * c - fz * s, fx * s + fz * c).normalize_or_zero()
}

pub(crate) fn part_counts_for_play_forward(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n.is_empty() {
        return true;
    }
    const SKIP: &[&str] = &[
        "hair",
        "gun",
        "weapon",
        "boost",
        "cape",
        "wing",
        "shield",
        "backpack",
        "acc",
        "accessory",
        "prop",
    ];
    !SKIP.iter().any(|s| n.contains(s))
}

pub(crate) fn display_forward_xz(asset: &ModelAsset) -> glam::Vec2 {
    let fwd = estimate_skinned_forward_torso(asset);
    apply_hips_bind_yaw_to_forward(fwd, asset)
}

/// Forward del jugador FP para asset skinned **glTF/GLB** (tras `try_bind`).
pub fn resolve_gltf_play_character_forward_xz(asset: &ModelAsset) -> glam::Vec2 {
    if needs_skinned_forward_correction(asset) {
        return display_forward_xz(asset);
    }
    let meta = asset.facing_forward_xz;
    let est_pos = {
        let verts = skinned_forward_core_vertices(asset);
        if verts.is_empty() {
            meta
        } else {
            crate::engine3d::config_3d::mesh_3d::estimate_mesh_forward_xz(&verts)
        }
    };
    let est_torso = estimate_skinned_forward_torso(asset);
    if est_pos.dot(est_torso) < -0.5 {
        return crate::engine3d::config_3d::mesh_3d::resolve_mesh_forward_xz(meta, est_torso);
    }
    let est = estimate_skinned_forward_from_vertices(asset);
    crate::engine3d::config_3d::mesh_3d::resolve_mesh_forward_xz(meta, est)
}

/// Forward del jugador FP para asset skinned **FBX** (tras `try_bind`).
pub fn resolve_fbx_play_character_forward_xz(asset: &ModelAsset) -> glam::Vec2 {
    if needs_skinned_forward_correction(asset) {
        return display_forward_xz(asset);
    }
    let meta = asset.facing_forward_xz;
    let est_pos = {
        let verts = skinned_forward_core_vertices(asset);
        if verts.is_empty() {
            meta
        } else {
            crate::engine3d::config_3d::mesh_3d::estimate_mesh_forward_xz(&verts)
        }
    };
    let est_torso = estimate_skinned_forward_torso(asset);
    if est_pos.dot(est_torso) < -0.5 {
        return crate::engine3d::config_3d::fbx_facing::resolve_fbx_forward_xz(meta, est_torso);
    }
    let est = estimate_skinned_forward_from_vertices(asset);
    crate::engine3d::config_3d::fbx_facing::resolve_fbx_forward_xz(meta, est)
}
