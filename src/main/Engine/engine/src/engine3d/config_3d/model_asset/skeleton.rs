//! Esqueleto unificado, huesos, IBM, pose de bind y piezas rígidas/skinned.

use super::*;

/// Global de un joint glTF recorriendo la escena (`globalTransform(joint)`).
pub(crate) fn gltf_scene_global_for_joint(
    joint_node: usize,
    joint_locals: &[Mat4],
    joint_gltf_nodes: &[usize],
    scene_parents: &HashMap<usize, usize>,
    bind_node_local: &HashMap<usize, Mat4>,
) -> Mat4 {
    let node_to_joint: HashMap<usize, usize> = joint_gltf_nodes
        .iter()
        .enumerate()
        .map(|(ji, &ni)| (ni, ji))
        .collect();
    let mut chain = vec![joint_node];
    let mut cur = joint_node;
    let mut guard = 0usize;
    while let Some(&parent) = scene_parents.get(&cur) {
        guard += 1;
        if guard > 512 {
            break;
        }
        chain.push(parent);
        cur = parent;
    }
    chain.reverse();
    let mut world = Mat4::IDENTITY;
    for idx in chain {
        let local = if let Some(&ji) = node_to_joint.get(&idx) {
            joint_locals.get(ji).copied().unwrap_or(Mat4::IDENTITY)
        } else {
            bind_node_local.get(&idx).copied().unwrap_or(Mat4::IDENTITY)
        };
        world *= local;
    }
    world
}

pub(crate) fn compute_gltf_joint_worlds(
    joint_gltf_nodes: &[usize],
    joint_locals: &[Mat4],
    scene_parents: &HashMap<usize, usize>,
    bind_node_local: &HashMap<usize, Mat4>,
) -> Vec<Mat4> {
    joint_gltf_nodes
        .iter()
        .map(|&node_ix| {
            gltf_scene_global_for_joint(
                node_ix,
                joint_locals,
                joint_gltf_nodes,
                scene_parents,
                bind_node_local,
            )
        })
        .collect()
}

pub(crate) fn gltf_skin_joint_node_indices(skin: &gltf::Skin) -> Vec<usize> {
    skin.joints().map(|n| n.index()).collect()
}

/// Origen de un hueso en el esqueleto unificado (útil para diagnosticar anchors rígidos).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GltfJointOrigin {
    SkinJoint,
    SkeletonRoot,
    SceneAncestor,
    AnimNode,
}

/// Esqueleto unificado: varios `skin` en un GLB comparten nodos o usan listas distintas.
pub(crate) struct GltfUnifiedSkeleton {
    pub(crate) node_to_unified: HashMap<usize, usize>,
    pub(crate) joint_gltf_nodes: Vec<usize>,
    pub(crate) joint_names: Vec<String>,
    pub(crate) joint_origins: Vec<GltfJointOrigin>,
}

/// Nodo con malla skinned y el `skin` glTF a usar (propio o heredado del padre).
pub(crate) struct SkinnedMeshNode {
    pub(crate) node_index: usize,
    pub(crate) skin_index: usize,
}

pub(crate) fn build_gltf_unified_skeleton(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    skinned_nodes: &[SkinnedMeshNode],
) -> Option<GltfUnifiedSkeleton> {
    let mut skin_stats: HashMap<usize, (usize, Vec<usize>)> = HashMap::new();
    for entry in skinned_nodes {
        let skin = doc.skins().nth(entry.skin_index)?;
        let joint_nodes = gltf_skin_joint_node_indices(&skin);
        let stat = skin_stats
            .entry(entry.skin_index)
            .or_insert_with(|| (0, joint_nodes));
        stat.0 += gltf_skinned_vertex_count(doc, buffers, entry.node_index);
    }
    if skin_stats.is_empty() {
        return None;
    }

    let mut skins_ordered: Vec<(usize, usize, Vec<usize>)> = skin_stats
        .into_iter()
        .map(|(idx, (verts, joints))| (idx, verts, joints))
        .collect();
    skins_ordered.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.len().cmp(&a.2.len())));

    let mut ordered_unique: Vec<usize> = Vec::new();
    let mut seen: HashSet<usize> = HashSet::new();
    for (_skin_idx, _verts, joint_nodes) in &skins_ordered {
        for &node_ix in joint_nodes {
            if seen.insert(node_ix) {
                ordered_unique.push(node_ix);
            }
        }
    }
    let raw_unique_joints = ordered_unique.len();
    let joints_truncated = raw_unique_joints.saturating_sub(MAX_JOINTS);

    let mut node_to_unified: HashMap<usize, usize> = HashMap::new();
    let mut joint_gltf_nodes: Vec<usize> = Vec::new();
    let mut joint_names: Vec<String> = Vec::new();
    let mut joint_origins: Vec<GltfJointOrigin> = Vec::new();

    for &node_ix in ordered_unique.iter().take(MAX_JOINTS) {
        let ui = joint_gltf_nodes.len();
        node_to_unified.insert(node_ix, ui);
        joint_gltf_nodes.push(node_ix);
        let name = doc
            .nodes()
            .nth(node_ix)
            .and_then(|n| n.name())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("joint_{ui}"));
        joint_names.push(name);
        joint_origins.push(GltfJointOrigin::SkinJoint);
    }

    if joint_gltf_nodes.is_empty() {
        return None;
    }
    if joints_truncated > 0 {
        log::warn!(
            "[model_asset] glTF: esqueleto truncado {raw_unique_joints} → {MAX_JOINTS} huesos ({joints_truncated} omitidos)"
        );
    }

    Some(GltfUnifiedSkeleton {
        node_to_unified,
        joint_gltf_nodes,
        joint_names,
        joint_origins,
    })
}

pub(crate) fn remap_gltf_vertex_joints_to_unified(
    mesh: &mut SkinnedMeshData,
    skin: &gltf::Skin,
    node_to_unified: &HashMap<usize, usize>,
    drops: &mut skin_diag::RemapDropCollector,
) {
    let skin_joint_nodes: Vec<usize> = gltf_skin_joint_node_indices(skin);
    for (vi, v) in mesh.vertices.iter_mut().enumerate() {
        let mut joints = v.joints;
        let mut weights = v.weights;
        for slot in 0..4 {
            if weights[slot] <= 1e-6 {
                continue;
            }
            let si = v.joints[slot] as usize;
            let Some(&node_ix) = skin_joint_nodes.get(si) else {
                drops.record(
                    vi,
                    slot,
                    v.joints[slot],
                    None,
                    weights[slot],
                    skin_diag::RemapDropReason::SkinJointIndexOob,
                );
                weights[slot] = 0.0;
                joints[slot] = 0;
                continue;
            };
            let Some(ui) = node_to_unified.get(&node_ix).copied() else {
                drops.record(
                    vi,
                    slot,
                    v.joints[slot],
                    Some(node_ix),
                    weights[slot],
                    skin_diag::RemapDropReason::NotInUnifiedSkeleton,
                );
                weights[slot] = 0.0;
                joints[slot] = 0;
                continue;
            };
            joints[slot] = ui as u32;
        }
        let sum: f32 = weights.iter().sum();
        if sum > 1e-6 {
            for w in weights.iter_mut() {
                *w /= sum;
            }
        } else {
            weights = [1.0, 0.0, 0.0, 0.0];
            joints = [0, 0, 0, 0];
        }
        v.joints = joints;
        v.weights = weights;
    }
}

pub(crate) fn part_inverse_bind_for_gltf_skin(
    skin: &gltf::Skin,
    buffers: &[gltf::buffer::Data],
    node_to_unified: &HashMap<usize, usize>,
    joint_count: usize,
    _mesh_bind_world: Mat4,
    doc: &gltf::Document,
    scene_parents: &HashMap<usize, usize>,
) -> (Vec<[[f32; 4]; 4]>, u32) {
    let mut ibm = vec![[[0.0; 4]; 4]; joint_count];
    let skin_joint_nodes = gltf_skin_joint_node_indices(skin);
    let skin_reader = skin.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()));
    let mut ibm_fallback = 0u32;
    if let Some(iter) = skin_reader.read_inverse_bind_matrices() {
        for (si, m) in iter.enumerate() {
            let Some(&node_ix) = skin_joint_nodes.get(si) else {
                continue;
            };
            let Some(&ui) = node_to_unified.get(&node_ix) else {
                continue;
            };
            ibm[ui] = m;
        }
    }
    for &node_ix in &skin_joint_nodes {
        let Some(&ui) = node_to_unified.get(&node_ix) else {
            continue;
        };
        if ibm[ui] == [[0.0; 4]; 4] {
            let joint_world = world_matrix_for_gltf_node_index(doc, scene_parents, node_ix);
            ibm[ui] = joint_world.inverse().to_cols_array_2d();
            ibm_fallback += 1;
        }
    }
    (ibm, ibm_fallback)
}

/// Nodos que anima el GLB pero no están en `skin.joints` (p. ej. raíz Armature).
pub(crate) fn extend_gltf_unified_with_anim_nodes(
    doc: &gltf::Document,
    unified: &mut GltfUnifiedSkeleton,
) {
    for anim in doc.animations() {
        for channel in anim.channels() {
            let node_ix = channel.target().node().index();
            if unified.node_to_unified.contains_key(&node_ix) {
                continue;
            }
            if unified.joint_gltf_nodes.len() >= MAX_JOINTS {
                return;
            }
            let ui = unified.joint_gltf_nodes.len();
            unified.node_to_unified.insert(node_ix, ui);
            unified.joint_gltf_nodes.push(node_ix);
            let name = doc
                .nodes()
                .nth(node_ix)
                .and_then(|n| n.name())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("anim_node_{ui}"));
            unified.joint_names.push(name);
            unified.joint_origins.push(GltfJointOrigin::AnimNode);
        }
    }
}

pub(crate) fn extend_gltf_unified_with_scene_ancestors(
    doc: &gltf::Document,
    node_parents: &HashMap<usize, usize>,
    unified: &mut GltfUnifiedSkeleton,
) -> usize {
    let mut to_add: Vec<usize> = Vec::new();
    for &joint_node in &unified.joint_gltf_nodes {
        let mut cur = joint_node;
        for _ in 0..512 {
            let Some(&parent) = node_parents.get(&cur) else {
                break;
            };
            if !unified.node_to_unified.contains_key(&parent) {
                to_add.push(parent);
            }
            cur = parent;
        }
    }
    to_add.sort_unstable();
    to_add.dedup();
    let mut added = 0usize;
    for node_ix in to_add {
        if unified.joint_gltf_nodes.len() >= MAX_JOINTS {
            break;
        }
        if unified.node_to_unified.contains_key(&node_ix) {
            continue;
        }
        let ui = unified.joint_gltf_nodes.len();
        unified.node_to_unified.insert(node_ix, ui);
        unified.joint_gltf_nodes.push(node_ix);
        let name = doc
            .nodes()
            .nth(node_ix)
            .and_then(|n| n.name())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("ancestor_{ui}"));
        unified.joint_names.push(name);
        unified.joint_origins.push(GltfJointOrigin::SceneAncestor);
        added += 1;
    }
    added
}

pub(crate) fn extend_gltf_unified_with_skin_skeleton_roots(
    doc: &gltf::Document,
    unified: &mut GltfUnifiedSkeleton,
) -> usize {
    let mut added = 0usize;
    for skin in doc.skins() {
        let Some(skeleton) = skin.skeleton() else {
            continue;
        };
        let node_ix = skeleton.index();
        if unified.node_to_unified.contains_key(&node_ix) {
            continue;
        }
        if unified.joint_gltf_nodes.len() >= MAX_JOINTS {
            break;
        }
        let ui = unified.joint_gltf_nodes.len();
        unified.node_to_unified.insert(node_ix, ui);
        unified.joint_gltf_nodes.push(node_ix);
        let name = doc
            .nodes()
            .nth(node_ix)
            .and_then(|n| n.name())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("skeleton_root_{ui}"));
        unified.joint_names.push(name);
        unified.joint_origins.push(GltfJointOrigin::SkeletonRoot);
        added += 1;
    }
    added
}

pub(crate) fn collect_gltf_scene_chain_nodes(
    joint_gltf_nodes: &[usize],
    scene_parents: &HashMap<usize, usize>,
) -> HashSet<usize> {
    let mut nodes = HashSet::new();
    for &joint_node in joint_gltf_nodes {
        let mut cur = joint_node;
        let mut guard = 0usize;
        nodes.insert(cur);
        while let Some(&parent) = scene_parents.get(&cur) {
            guard += 1;
            if guard > 512 {
                break;
            }
            nodes.insert(parent);
            cur = parent;
        }
    }
    nodes
}

/// Índice del nodo con mesh más grande (preview / sustituto de malla estática).
pub(crate) fn gltf_primary_mesh_node_index(file: &GltfFile) -> Option<usize> {
    let scene = file
        .doc
        .default_scene()
        .or_else(|| file.doc.scenes().next())?;
    let mut mesh_nodes: Vec<usize> = Vec::new();
    for root in scene.nodes() {
        collect_gltf_mesh_node_indices(root, &mut mesh_nodes);
    }
    mesh_nodes
        .into_iter()
        .max_by_key(|&idx| gltf_mesh_vertex_count(&file.doc, &file.buffers, idx))
        .filter(|&idx| gltf_mesh_vertex_count(&file.doc, &file.buffers, idx) > 0)
}

pub(crate) fn collect_gltf_mesh_node_indices(node: gltf::Node, out: &mut Vec<usize>) {
    if node.mesh().is_some() {
        out.push(node.index());
    }
    for child in node.children() {
        collect_gltf_mesh_node_indices(child, out);
    }
}

pub(crate) fn gltf_mesh_vertex_count(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    node_index: usize,
) -> usize {
    let node = match doc.nodes().nth(node_index) {
        Some(n) => n,
        None => return 0,
    };
    let mesh = match node.mesh() {
        Some(m) => m,
        None => return 0,
    };
    mesh.primitives()
        .filter_map(|prim| {
            prim.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()))
                .read_positions()
                .map(|p| p.count())
        })
        .sum()
}

pub(crate) fn build_gltf_node_parents(scene: &gltf::Scene) -> HashMap<usize, usize> {
    let mut parents = HashMap::new();
    fn walk(node: gltf::Node, parent: Option<usize>, out: &mut HashMap<usize, usize>) {
        let idx = node.index();
        if let Some(p) = parent {
            out.insert(idx, p);
        }
        for child in node.children() {
            walk(child, Some(idx), out);
        }
    }
    for root in scene.nodes() {
        walk(root, None, &mut parents);
    }
    parents
}

/// Padres de **todos** los nodos del GLB (no solo los alcanzables desde `scene`).
/// Los `skin.joints` pueden colgar fuera del árbol de la escena por defecto.
pub(crate) fn build_gltf_all_node_parents(doc: &gltf::Document) -> HashMap<usize, usize> {
    let mut parents = HashMap::new();
    for node in doc.nodes() {
        let idx = node.index();
        for child in node.children() {
            parents.insert(child.index(), idx);
        }
    }
    parents
}

pub(crate) fn world_matrix_for_gltf_node_index(
    doc: &gltf::Document,
    node_parents: &HashMap<usize, usize>,
    target: usize,
) -> Mat4 {
    let mut chain = vec![target];
    let mut cur = target;
    let mut guard = 0usize;
    while let Some(&parent) = node_parents.get(&cur) {
        guard += 1;
        if guard > 512 {
            break;
        }
        chain.push(parent);
        cur = parent;
    }
    chain.reverse();
    let mut world = Mat4::IDENTITY;
    for idx in chain {
        if let Some(n) = doc.nodes().nth(idx) {
            world *= node_local_matrix(&n);
        }
    }
    world
}

pub(crate) fn node_local_matrix(node: &gltf::Node) -> Mat4 {
    match node.transform() {
        gltf::scene::Transform::Matrix { matrix } => Mat4::from_cols_array_2d(&matrix),
        gltf::scene::Transform::Decomposed {
            translation,
            rotation,
            scale,
        } => Mat4::from_scale_rotation_translation(
            Vec3::from(scale),
            Quat::from_array(rotation),
            Vec3::from(translation),
        ),
    }
}

/// Inversa segura de IBM (evita NaN con matrices cero; no filtra por determinante).
pub(crate) fn mat4_from_ibm_inverse(ibm: &[[f32; 4]; 4]) -> Option<Mat4> {
    if ibm.iter().flatten().all(|&c| c == 0.0) {
        return None;
    }
    let m = Mat4::from_cols_array_2d(ibm);
    if !m.to_cols_array().iter().all(|v| v.is_finite()) {
        return None;
    }
    let inv = m.inverse();
    if !inv.to_cols_array().iter().all(|v| v.is_finite()) {
        return None;
    }
    Some(inv)
}

pub(crate) fn gltf_ibm_is_populated(ibm: &[[f32; 4]; 4]) -> bool {
    !ibm.iter().flatten().all(|&c| c == 0.0)
}

/// Fusiona IBM por hueso unificado desde todas las piezas skinned (varios `skin` en un GLB).
pub(crate) fn merge_gltf_unified_inverse_bind(
    parts: &[SkinnedMeshPart],
    joint_count: usize,
) -> (Vec<[[f32; 4]; 4]>, usize) {
    let empty = [[0.0f32; 4]; 4];
    let mut merged = vec![empty; joint_count];
    let mut gaps_filled = 0usize;
    for (pi, part) in parts.iter().enumerate() {
        for (ji, merged_ibm) in merged.iter_mut().enumerate().take(part.inverse_bind.len()) {
            let src = &part.inverse_bind[ji];
            if !gltf_ibm_is_populated(src) {
                continue;
            }
            if !gltf_ibm_is_populated(merged_ibm) {
                *merged_ibm = *src;
                if pi > 0 {
                    gaps_filled += 1;
                }
            }
        }
    }
    (merged, gaps_filled)
}

/// Rellena huecos de IBM por pieza (p. ej. Head en skin del cuerpo) sin pisar la IBM propia del skin.
pub(crate) fn fill_gltf_part_inverse_bind_gaps(
    parts: &mut [SkinnedMeshPart],
    unified: &[[[f32; 4]; 4]],
) {
    for part in parts.iter_mut() {
        for (ji, part_ibm) in part.inverse_bind.iter_mut().enumerate().take(unified.len()) {
            if !gltf_ibm_is_populated(part_ibm) && gltf_ibm_is_populated(&unified[ji]) {
                *part_ibm = unified[ji];
            }
        }
    }
}

/// Endereza rigs IBM-only cuyo AABB no tiene Y como eje principal (p. ej. GLB 4K acostado).
pub(crate) fn apply_editor_upright_ibm_skinned(
    parts: &mut [SkinnedMeshPart],
    default_ibm: &mut [[[f32; 4]; 4]],
    bind_local: &[Mat4],
    joint_names: &[String],
) -> bool {
    let Some((min, max)) = gltf_skinned_parts_bounds(parts) else {
        return false;
    };
    let span = Vec3::from_array(max) - Vec3::from_array(min);
    let max_axis = span.x.max(span.y).max(span.z);
    if max_axis < 1e-4 || span.y >= max_axis * 0.82 {
        return false;
    }
    let ibm_globals = gltf_bind_globals_from_ibm(default_ibm, bind_local, bind_local);
    let joint_positions: Vec<Vec3> = ibm_globals
        .iter()
        .map(|m| m.transform_point3(Vec3::ZERO))
        .collect();
    let points = gltf_skinned_sample_points(parts);
    let upright =
        upright_quat_from_bind_joints(joint_names, &joint_positions).unwrap_or_else(|| {
            crate::engine3d::config_3d::mesh_3d::upright_quat_from_vertices_bounds(
                min, max, &points,
            )
        });
    if upright.w.abs() > 0.9999 && upright.x.abs() < 1e-4 && upright.y.abs() < 1e-4 {
        return false;
    }
    let norm = Mat4::from_quat(upright);
    let inv_norm = norm.inverse();
    for part in parts.iter_mut() {
        apply_normalize_to_skinned_vertices(&mut part.mesh.vertices, norm);
        for ibm in part.inverse_bind.iter_mut() {
            let m = Mat4::from_cols_array_2d(ibm);
            *ibm = (m * inv_norm).to_cols_array_2d();
        }
    }
    for ibm in default_ibm.iter_mut() {
        if gltf_ibm_is_populated(ibm) {
            let m = Mat4::from_cols_array_2d(ibm);
            *ibm = (m * inv_norm).to_cols_array_2d();
        }
    }
    true
}

/// Omite props/armas rígidas emparentadas al personaje (KSVR, kits sin skin, etc.).
pub(crate) fn should_skip_gltf_rigid_mesh(
    mesh_name: &str,
    skinned_part_count: usize,
    rigid_candidate_count: usize,
) -> bool {
    let n = mesh_name.to_ascii_lowercase();
    const KEYWORDS: &[&str] = &[
        "ksvr",
        "gun",
        "weapon",
        "rifle",
        "pistol",
        "firearm",
        "barrel",
        "trigger",
        "stockframe",
        "stockback",
        "stockthing",
        "stockbutton",
        "mag_low",
        "_mag",
        "muzzle",
        "silencer",
        "topgun",
        "bottomgun",
        "frontgrip",
        "frontsight",
        "backsight",
        "siderail",
        "toprail",
    ];
    if KEYWORDS.iter().any(|k| n.contains(k)) {
        return true;
    }
    // Kit de muchas piezas sin nombre (Object_N) + pocos skins de personaje.
    if skinned_part_count <= 5
        && rigid_candidate_count >= 12
        && (n.starts_with("object_") || n.is_empty())
    {
        return true;
    }
    false
}

/// global[j] = inverse(IBM[j]) × (local[j] × inverse(bind_local[j]))
pub(crate) fn gltf_bind_globals_from_ibm(
    inverse_bind: &[[[f32; 4]; 4]],
    bind_local: &[Mat4],
    locals: &[Mat4],
) -> Vec<Mat4> {
    let n = locals.len().min(bind_local.len()).min(inverse_bind.len());
    (0..n)
        .map(|ji| {
            let rel = locals[ji] * bind_local[ji].inverse();
            mat4_from_ibm_inverse(&inverse_bind[ji])
                .map(|bind_global| bind_global * rel)
                .unwrap_or(Mat4::IDENTITY)
        })
        .collect()
}

/// Globals de bind para paleta GPU (IBM por pieza si `bind_pose_from_ibm`).
pub(crate) fn gltf_skinned_part_globals_for_palette(
    asset: &ModelAsset,
    part: &SkinnedMeshPart,
    locals: &[Mat4],
) -> Vec<Mat4> {
    let joint_count = asset
        .joint_parents
        .len()
        .min(MAX_JOINTS)
        .min(asset.bind_local.len())
        .min(locals.len());
    if asset.bind_pose_from_ibm && !asset.joint_gltf_nodes.is_empty() {
        return gltf_bind_globals_from_ibm(
            &part.inverse_bind,
            &asset.bind_local[..joint_count],
            &locals[..joint_count],
        );
    }
    compute_gltf_joint_worlds(
        &asset.joint_gltf_nodes[..joint_count.min(asset.joint_gltf_nodes.len())],
        &locals[..joint_count],
        &asset.gltf_scene_parents,
        &asset.gltf_bind_node_local,
    )
}

pub(crate) fn skinned_mesh_extent(vertices: &[SkinnedVertex]) -> f32 {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for v in vertices {
        if !v.position.iter().all(|c| c.is_finite()) {
            continue;
        }
        any = true;
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
        }
    }
    if any {
        (Vec3::from_array(max) - Vec3::from_array(min)).length()
    } else {
        0.0
    }
}

pub(crate) fn skeleton_extent_from_globals(globals: &[Mat4]) -> f32 {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for g in globals {
        let p = g.transform_point3(Vec3::ZERO);
        if !p.is_finite() {
            continue;
        }
        any = true;
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    if any {
        (Vec3::from_array(max) - Vec3::from_array(min)).length()
    } else {
        0.0
    }
}

/// Estadísticas de jerarquía vs IBM para detectar rigs con bind pose en IBM.
pub(crate) fn skin_bind_hierarchy_ibm_stats(
    hierarchy_globals: &[Mat4],
    inverse_bind: &[[[f32; 4]; 4]],
) -> (usize, usize, f32) {
    let joint_count = hierarchy_globals.len().min(inverse_bind.len());
    let mut mismatch = 0usize;
    let mut ibm_pos = 0usize;
    for ji in 0..joint_count {
        let engine_t = hierarchy_globals[ji].transform_point3(Vec3::ZERO);
        let Some(ibm_inv) = mat4_from_ibm_inverse(&inverse_bind[ji]) else {
            continue;
        };
        let ibm_t = ibm_inv.transform_point3(Vec3::ZERO);
        if ibm_t.length() > 1e-2 {
            ibm_pos += 1;
        }
        if engine_t.length() < 1e-3 && ibm_t.length() > 1e-2 {
            mismatch += 1;
        }
    }
    let skel_span = skeleton_extent_from_globals(hierarchy_globals);
    (mismatch, ibm_pos, skel_span)
}

/// Detecta rigs FBX→GLB donde la bind pose vive en IBM y los nodos tienen TRS≈identidad.
pub(crate) fn detect_bind_pose_from_ibm(
    hierarchy_globals: &[Mat4],
    inverse_bind: &[[[f32; 4]; 4]],
    mesh_vertices: &[SkinnedVertex],
) -> bool {
    let joint_count = hierarchy_globals.len().min(inverse_bind.len());
    if joint_count == 0 {
        return false;
    }

    let mesh_span = skinned_mesh_extent(mesh_vertices);
    let (mismatch, ibm_pos, skel_span) =
        skin_bind_hierarchy_ibm_stats(hierarchy_globals, inverse_bind);

    if skel_span < 0.001 && mesh_span > 0.1 {
        return true;
    }

    if ibm_pos == 0 {
        return false;
    }

    if mismatch >= 20 && mismatch * 2 >= ibm_pos {
        return true;
    }

    if mismatch * 2 >= joint_count {
        return true;
    }

    false
}

pub(crate) const EDITOR_IBM_SKINNED_TARGET_EXTENT: f32 = 1.8;

pub(crate) const EDITOR_IBM_SKINNED_MAX_Y: f32 = 2.2;

/// Escala mallas IBM-only demasiado altas (~2.7 m) a ~1.8 m en editor.
/// También ajusta IBM: `IBM' = IBM × inverse(norm)` para que malla y skinning compartan espacio.
pub(crate) fn editor_normalize_oversized_ibm_skinned(
    parts: &mut [SkinnedMeshPart],
    skinned_only: usize,
) -> bool {
    let n = skinned_only.min(parts.len());
    if n == 0 {
        return false;
    }
    let Some((min, max)) = gltf_skinned_parts_bounds(&parts[..n]) else {
        return false;
    };
    let span = Vec3::from_array(max) - Vec3::from_array(min);
    let y_span = span.y;
    if y_span <= EDITOR_IBM_SKINNED_MAX_Y && span.length() <= 5.0 {
        return false;
    }
    let extent = y_span.max(span.length()).max(1e-4);
    let scale = EDITOR_IBM_SKINNED_TARGET_EXTENT / extent;
    let center = (Vec3::from_array(min) + Vec3::from_array(max)) * 0.5;
    let norm = Mat4::from_scale(Vec3::splat(scale)) * Mat4::from_translation(-center);
    let inv_norm = norm.inverse();
    for part in parts.iter_mut().take(n) {
        apply_normalize_to_skinned_vertices(&mut part.mesh.vertices, norm);
        for ibm in part.inverse_bind.iter_mut() {
            let m = Mat4::from_cols_array_2d(ibm);
            *ibm = (m * inv_norm).to_cols_array_2d();
        }
    }
    true
}

pub(crate) fn collect_gltf_skinned_mesh_nodes(
    node: gltf::Node,
    inherited_skin: Option<usize>,
    out: &mut Vec<SkinnedMeshNode>,
) {
    let skin_ix = node.skin().map(|s| s.index()).or(inherited_skin);
    if let Some(skin_index) = skin_ix
        && node.mesh().is_some()
    {
        out.push(SkinnedMeshNode {
            node_index: node.index(),
            skin_index,
        });
    }
    let child_inherit = node.skin().map(|s| s.index()).or(inherited_skin);
    for child in node.children() {
        collect_gltf_skinned_mesh_nodes(child, child_inherit, out);
    }
}

pub(crate) fn collect_gltf_scene_mesh_nodes(node: gltf::Node, out: &mut Vec<(usize, String)>) {
    if node.mesh().is_some() {
        let name = node
            .name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("mesh_node_{}", node.index()));
        out.push((node.index(), name));
    }
    for child in node.children() {
        collect_gltf_scene_mesh_nodes(child, out);
    }
}

pub(crate) fn is_valid_rigid_anchor_origin(origin: GltfJointOrigin) -> bool {
    matches!(
        origin,
        GltfJointOrigin::SkinJoint | GltfJointOrigin::AnimNode
    )
}

pub(crate) fn mat4_world_translation(m: Mat4) -> Vec3 {
    m.transform_point3(Vec3::ZERO)
}

/// Centro del AABB de todas las primitivas POSITION del nodo mesh (espacio local del nodo).
pub(crate) fn gltf_mesh_node_local_aabb_center(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    node_index: usize,
) -> Option<Vec3> {
    let node = doc.nodes().nth(node_index)?;
    let mesh = node.mesh()?;
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    let mut any = false;
    for prim in mesh.primitives() {
        let reader = prim.reader(|buf| Some(&buffers[buf.index()]));
        let Some(iter) = reader.read_positions() else {
            continue;
        };
        for p in iter {
            any = true;
            let v = Vec3::from(p);
            min = min.min(v);
            max = max.max(v);
        }
    }
    any.then_some((min + max) * 0.5)
}

/// Matriz bind del nodo + punto de referencia espacial (centro geométrico en espacio escena).
pub(crate) fn gltf_rigid_mesh_spatial_reference(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    node_parents: &HashMap<usize, usize>,
    node_index: usize,
) -> (Mat4, Vec3) {
    let mesh_bind_world = world_matrix_for_gltf_node_index(doc, node_parents, node_index);
    let node_origin = mat4_world_translation(mesh_bind_world);
    let mesh_world_ref = gltf_mesh_node_local_aabb_center(doc, buffers, node_index)
        .map(|local| mesh_bind_world.transform_point3(local))
        .unwrap_or(node_origin);
    (mesh_bind_world, mesh_world_ref)
}

pub(crate) fn nearest_skin_joint_spatial(
    mesh_world_ref: Vec3,
    skin_joint_set: &HashSet<usize>,
    node_to_joint: &HashMap<usize, usize>,
    runtime_globals: &[Mat4],
) -> Option<usize> {
    if skin_joint_set.is_empty() {
        return None;
    }
    let mut best: Option<(usize, f32)> = None;
    for &gltf_node in skin_joint_set {
        let Some(&ui) = node_to_joint.get(&gltf_node) else {
            continue;
        };
        let joint_m = runtime_globals.get(ui).copied().unwrap_or(Mat4::IDENTITY);
        let joint_pos = mat4_world_translation(joint_m);
        let dist = (mesh_world_ref - joint_pos).length();
        if best.as_ref().is_none_or(|(_, d)| dist < *d) {
            best = Some((ui, dist));
        }
    }
    best.map(|(ui, _)| ui)
}

/// Recorre padres desde la malla; el anchor válido es el primer nodo en `node_to_joint`
/// con origen `SkinJoint` o `AnimNode`. Si no hay anchor en cadena, fallback espacial.
pub(crate) fn resolve_rigid_joint_ui(
    mesh_node: usize,
    mesh_world_ref: Vec3,
    scene_parents: &HashMap<usize, usize>,
    node_to_joint: &HashMap<usize, usize>,
    skin_joint_set: &HashSet<usize>,
    joint_origins: &[GltfJointOrigin],
    runtime_globals: &[Mat4],
) -> Option<usize> {
    let mut cur = mesh_node;
    for _ in 0..512 {
        if let Some(&anchor_ui) = node_to_joint.get(&cur) {
            let anchor_origin = joint_origins
                .get(anchor_ui)
                .copied()
                .unwrap_or(GltfJointOrigin::SkinJoint);
            if is_valid_rigid_anchor_origin(anchor_origin) {
                return Some(anchor_ui);
            }
        }
        let Some(&parent) = scene_parents.get(&cur) else {
            break;
        };
        cur = parent;
    }

    nearest_skin_joint_spatial(
        mesh_world_ref,
        skin_joint_set,
        node_to_joint,
        runtime_globals,
    )
    .or_else(|| {
        log::warn!(
            "[model_asset] malla rígida sin joint cercano (skin_joint_set={})",
            skin_joint_set.len(),
        );
        None
    })
}

pub(crate) fn gltf_skinned_vertex_count(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    node_index: usize,
) -> usize {
    let node = match doc.nodes().nth(node_index) {
        Some(n) => n,
        None => return 0,
    };
    let mesh = match node.mesh() {
        Some(m) => m,
        None => return 0,
    };
    mesh.primitives()
        .filter_map(|prim| {
            prim.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()))
                .read_positions()
                .map(|p| p.count())
        })
        .sum()
}

pub(crate) fn read_skinned_gltf_primitive(
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
    material_albedos: &HashMap<usize, gltf::image::Data>,
) -> Option<SkinnedMeshData> {
    let reader = primitive.reader(|buf| Some(&buffers[buf.index()]));
    let positions: Vec<[f32; 3]> = reader.read_positions()?.collect();
    let normals: Vec<[f32; 3]> = reader
        .read_normals()
        .map(|n| n.collect())
        .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);
    let uvs: Vec<[f32; 2]> = reader
        .read_tex_coords(0)
        .map(|tc| tc.into_f32().collect())
        .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);
    let joints_reader = reader.read_joints(0)?;
    let joint_data: Vec<[u16; 4]> = joints_reader.into_u16().collect();
    let weights_reader = reader.read_weights(0)?;
    let weight_data: Vec<[f32; 4]> = weights_reader.into_f32().collect();
    if joint_data.len() != positions.len() || weight_data.len() != positions.len() {
        return None;
    }
    let indices: Vec<u32> = reader
        .read_indices()
        .map(|i| i.into_u32().collect())
        .unwrap_or_else(|| (0..positions.len() as u32).collect());

    let mut vertices = Vec::with_capacity(positions.len());
    for i in 0..positions.len() {
        let j = joint_data.get(i).copied().unwrap_or([0, 0, 0, 0]);
        let (j0, j1, j2, j3) = (j[0] as u32, j[1] as u32, j[2] as u32, j[3] as u32);
        let w = weight_data.get(i).copied().unwrap_or([1.0, 0.0, 0.0, 0.0]);
        let (w0, w1, w2, w3) = (w[0], w[1], w[2], w[3]);
        let sum = w0 + w1 + w2 + w3;
        let (w0, w1, w2, w3) = if sum > 1e-6 {
            (w0 / sum, w1 / sum, w2 / sum, w3 / sum)
        } else {
            (1.0, 0.0, 0.0, 0.0)
        };
        vertices.push(SkinnedVertex {
            position: positions[i],
            normal: normals[i],
            uv: uvs[i],
            joints: [j0, j1, j2, j3],
            weights: [w0, w1, w2, w3],
        });
    }

    let (rgba, width, height) = gltf_texture_rgba(primitive, material_albedos);

    Some(SkinnedMeshData {
        vertices,
        indices,
        rgba,
        width,
        height,
    })
}

/// Malla sin JOINTS_0 (ojos, pestañas, dientes…) anclada 100 % al joint ancestro más cercano.
pub(crate) fn read_gltf_rigid_primitive(
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
    material_albedos: &HashMap<usize, gltf::image::Data>,
    joint_ui: u32,
) -> Option<SkinnedMeshData> {
    let reader = primitive.reader(|buf| Some(&buffers[buf.index()]));
    let positions: Vec<[f32; 3]> = reader.read_positions()?.collect();
    if positions.is_empty() {
        return None;
    }
    let normals: Vec<[f32; 3]> = reader
        .read_normals()
        .map(|n| n.collect())
        .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);
    let uvs: Vec<[f32; 2]> = reader
        .read_tex_coords(0)
        .map(|tc| tc.into_f32().collect())
        .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);
    let indices: Vec<u32> = reader
        .read_indices()
        .map(|i| i.into_u32().collect())
        .unwrap_or_else(|| (0..positions.len() as u32).collect());
    let vertices: Vec<SkinnedVertex> = positions
        .into_iter()
        .enumerate()
        .map(|(i, position)| SkinnedVertex {
            position,
            normal: normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]),
            uv: uvs.get(i).copied().unwrap_or([0.0, 0.0]),
            joints: [joint_ui, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        })
        .collect();
    let (rgba, width, height) = gltf_texture_rgba(primitive, material_albedos);
    Some(SkinnedMeshData {
        vertices,
        indices,
        rgba,
        width,
        height,
    })
}

/// Hornea la transform del nodo mesh en vértices, normales e IBM (espacio escena = joints + paleta Khronos).
pub(crate) fn bake_gltf_mesh_bind_world(part: &mut SkinnedMeshPart) {
    let bind = part.mesh_bind_world;
    if bind == Mat4::IDENTITY {
        return;
    }
    let inv_bind = bind.inverse();
    transform_skinned_vertices(&mut part.mesh.vertices, bind);
    for ibm in part.inverse_bind.iter_mut() {
        *ibm = (Mat4::from_cols_array_2d(ibm) * inv_bind).to_cols_array_2d();
    }
    part.mesh_bind_world = Mat4::IDENTITY;
}

pub(crate) fn gltf_skinned_parts_bounds(parts: &[SkinnedMeshPart]) -> Option<([f32; 3], [f32; 3])> {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for part in parts {
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

pub(crate) fn gltf_skinned_sample_points(parts: &[SkinnedMeshPart]) -> Vec<Vec3> {
    parts
        .iter()
        .flat_map(|part| {
            part.mesh
                .vertices
                .iter()
                .map(|v| Vec3::from_array(v.position))
        })
        .collect()
}

pub(crate) fn find_joint_index_by_name(joint_names: &[String], needles: &[&str]) -> Option<usize> {
    for needle in needles {
        let needle = needle.to_ascii_lowercase();
        if let Some(ix) = joint_names
            .iter()
            .position(|name| name.to_ascii_lowercase().contains(&needle))
        {
            return Some(ix);
        }
    }
    None
}

pub(crate) fn bind_joint_world_positions(
    joint_gltf_nodes: &[usize],
    bind_local: &[Mat4],
    scene_parents: &HashMap<usize, usize>,
    bind_node_local: &HashMap<usize, Mat4>,
) -> Vec<Vec3> {
    compute_gltf_joint_worlds(joint_gltf_nodes, bind_local, scene_parents, bind_node_local)
        .into_iter()
        .map(|m| m.transform_point3(Vec3::ZERO))
        .collect()
}

pub(crate) fn upright_quat_from_bind_joints(
    joint_names: &[String],
    positions: &[Vec3],
) -> Option<Quat> {
    if positions.is_empty() {
        return None;
    }
    let mut up = if let (Some(hip), Some(head)) = (
        find_joint_index_by_name(joint_names, &["hips", "pelvis", "root"]),
        find_joint_index_by_name(joint_names, &["head"]),
    ) {
        positions[head] - positions[hip]
    } else {
        let (min_i, max_i) =
            positions
                .iter()
                .enumerate()
                .fold((0usize, 0usize), |(min_i, max_i), (i, p)| {
                    let min_i = if p.y < positions[min_i].y { i } else { min_i };
                    let max_i = if p.y > positions[max_i].y { i } else { max_i };
                    (min_i, max_i)
                });
        positions[max_i] - positions[min_i]
    };
    if up.length_squared() < 1e-8 {
        return None;
    }
    up = up.normalize();
    if up.y > 0.999 {
        return Some(Quat::IDENTITY);
    }
    Some(Quat::from_rotation_arc(up, Vec3::Y))
}

pub(crate) fn upright_quat_for_gltf_play_character(
    parts: &[SkinnedMeshPart],
    joint_names: &[String],
    joint_gltf_nodes: &[usize],
    bind_local: &[Mat4],
    scene_parents: &HashMap<usize, usize>,
    bind_node_local: &HashMap<usize, Mat4>,
) -> Option<Quat> {
    let joint_positions =
        bind_joint_world_positions(joint_gltf_nodes, bind_local, scene_parents, bind_node_local);
    if let Some(upright) = upright_quat_from_bind_joints(joint_names, &joint_positions) {
        return Some(upright);
    }
    let (min, max) = gltf_skinned_parts_bounds(parts)?;
    let points = gltf_skinned_sample_points(parts);
    let upright =
        crate::engine3d::config_3d::mesh_3d::upright_quat_from_vertices_bounds(min, max, &points);
    Some(upright)
}

/// Altura y centro en espacio enderezado (malla + esqueleto) para escalar al jugador FP.
pub(crate) fn gltf_play_bind_height_and_center(
    parts: &[SkinnedMeshPart],
    joint_positions: &[Vec3],
    upright: Quat,
) -> Option<(f32, Vec3)> {
    let up_mat = Mat4::from_quat(upright);
    let mut min_u = Vec3::splat(f32::MAX);
    let mut max_u = Vec3::splat(f32::MIN);
    let mut count = 0usize;
    for p in joint_positions {
        let pu = up_mat.transform_point3(*p);
        min_u = min_u.min(pu);
        max_u = max_u.max(pu);
        count += 1;
    }
    for part in parts {
        for v in &part.mesh.vertices {
            let pu = up_mat.transform_point3(Vec3::from_array(v.position));
            min_u = min_u.min(pu);
            max_u = max_u.max(pu);
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    let height = (max_u.y - min_u.y).max(1e-5);
    let center = (min_u + max_u) * 0.5;
    Some((height, center))
}
