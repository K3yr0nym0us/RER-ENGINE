//! Diagnóstico skinning: transform glTF (Matrix vs TRS) vs IBM vs jerarquía.

use std::collections::HashMap;

use glam::{Mat4, Vec3};

use crate::config_3d::model_asset::{
    gltf_node_local_ignore_matrix, gltf_node_transform_raw, mat4_from_ibm_inverse,
    node_local_matrix, skin_bind_hierarchy_ibm_stats, skeleton_extent_mesh_space,
};

const PROBE_NAME_NEEDLES: &[&str] = &["pelvis_05", "spine_01_06", "DEF-brow"];
const PROBE_MISMATCH_MAX: usize = 2;

/// Huesos estándar para comparar jerarquía vs runtime (Mixamo / convenciones comunes).
const DUAL_SPACE_BONE_NEEDLES: &[&str] = &[
    "hips",
    "spine",
    "head",
    "lefthand",
    "righthand",
    "leftfoot",
    "rightfoot",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemapDropReason {
    SkinJointIndexOob,
    NotInUnifiedSkeleton,
}

impl RemapDropReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::SkinJointIndexOob => "skin_joint_oob",
            Self::NotInUnifiedSkeleton => "not_in_unified",
        }
    }
}

#[derive(Clone, Debug)]
struct RemapDropSample {
    vert_index:        usize,
    slot:              usize,
    local_joint_index: u32,
    skin_node_index:   Option<usize>,
    weight:            f32,
    reason:            RemapDropReason,
}

#[derive(Default)]
pub struct RemapDropCollector {
    pub total:   u32,
    samples:     Vec<RemapDropSample>,
}

impl RemapDropCollector {
    pub fn record(
        &mut self,
        vert_index: usize,
        slot: usize,
        local_joint_index: u32,
        skin_node_index: Option<usize>,
        weight: f32,
        reason: RemapDropReason,
    ) {
        self.total += 1;
        if self.samples.len() < 10 {
            self.samples.push(RemapDropSample {
                vert_index,
                slot,
                local_joint_index,
                skin_node_index,
                weight,
                reason,
            });
        }
    }

    pub fn log_if_any(&self, label: &str) {
        if self.total == 0 {
            return;
        }
        let samples: Vec<String> = self
            .samples
            .iter()
            .map(|s| {
                format!(
                    "v{}s{} lj={} node={:?} w={:.3} {}",
                    s.vert_index,
                    s.slot,
                    s.local_joint_index,
                    s.skin_node_index,
                    s.weight,
                    s.reason.label()
                )
            })
            .collect();
        log::warn!(
            "[SKIN_XFORM] {label} remap_drop={} samples=[{}]",
            self.total,
            samples.join(" ")
        );
    }
}

pub fn log_skinned_unavailable(label: &str, reason: &str) {
    log::warn!("[SKIN_XFORM] {label} unavailable reason={reason}");
}

fn mat4_translation(m: Mat4) -> [f32; 3] {
    m.w_axis.truncate().to_array()
}

fn t_len(t: [f32; 3]) -> f32 {
    Vec3::from_array(t).length()
}

fn at_origin(t: [f32; 3]) -> bool {
    t_len(t) < 1e-3
}

fn chain_to_root(node: usize, parents: &HashMap<usize, usize>) -> Vec<usize> {
    let mut chain = vec![node];
    let mut cur = node;
    for _ in 0..512 {
        let Some(&parent) = parents.get(&cur) else {
            break;
        };
        chain.push(parent);
        cur = parent;
    }
    chain.reverse();
    chain
}

fn skeleton_span(globals: &[Mat4]) -> f32 {
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

fn mesh_span(vertices: &[crate::mesh::SkinnedVertex]) -> f32 {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for v in vertices {
        let p = v.position;
        if !p.iter().all(|c| c.is_finite()) {
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

fn format_node_probe(
    doc: &gltf::Document,
    node_ix: usize,
    name: &str,
    parent: Option<usize>,
    part_ibm: &[[[f32; 4]; 4]],
    ji: usize,
    _bind_local: &[Mat4],
    engine_globals: &[Mat4],
) -> String {
    let Some(node) = doc.nodes().nth(node_ix) else {
        return format!("node={node_ix} {name} MISSING");
    };
    let raw = gltf_node_transform_raw(&node);
    let loader = node_local_matrix(&node);
    let ignore_matrix = gltf_node_local_ignore_matrix(&node);
    let loader_t = mat4_translation(loader);
    let ignore_matrix_t = mat4_translation(ignore_matrix);
    let matrix_t = mat4_translation(Mat4::from_cols_array_2d(&raw.matrix_cols));
    let ibm_t = mat4_from_ibm_inverse(&part_ibm.get(ji).copied().unwrap_or([[0.0; 4]; 4]))
        .map(|m| mat4_translation(m))
        .unwrap_or([0.0; 3]);
    let engine_t = mat4_translation(
        engine_globals.get(ji).copied().unwrap_or(Mat4::IDENTITY),
    );

    let matrix_ignored_would_break = raw.storage == "Matrix"
        && !raw.matrix_is_identity
        && at_origin(ignore_matrix_t)
        && t_len(matrix_t) > 1e-3;

    let suspect = if matrix_ignored_would_break {
        "BUG_IGNORE_MATRIX"
    } else if raw.storage == "Decomposed" && raw.trs_is_identity && !at_origin(ibm_t) && at_origin(engine_t) {
        "BIND_IN_IBM_NOT_NODE"
    } else if at_origin(engine_t) && !at_origin(ibm_t) {
        "HIER_COLLAPSED_IBM_OK"
    } else {
        "OK"
    };

    format!(
        "node={node_ix} {name} parent={parent:?} | storage={} trs_id={} matrix_id={} \
         T={:?} R={:?} S={:?} | matrix_T={matrix_t:?} ignore_matrix_T={ignore_matrix_t:?} loader_T={loader_t:?} \
         ibm_inv_T={ibm_t:?} engine_T={engine_t:?} | {suspect}",
        raw.storage,
        raw.trs_is_identity,
        raw.matrix_is_identity,
        raw.translation,
        raw.rotation,
        raw.scale,
    )
}

/// Import: Matrix vs TRS vs IBM vs jerarquía reconstruida.
pub fn log_skin_transform_probe(
    label: &str,
    doc: &gltf::Document,
    joint_gltf_nodes: &[usize],
    joint_names: &[String],
    bind_local: &[Mat4],
    hierarchy_globals: &[Mat4],
    runtime_globals: &[Mat4],
    part_ibm: &[[[f32; 4]; 4]],
    all_node_parents: &HashMap<usize, usize>,
    mesh_vertices: &[crate::mesh::SkinnedVertex],
    bones_skin: usize,
    bones_runtime: usize,
    remap_drop: u32,
    bind_pose_from_ibm: bool,
    mesh_normalize: Mat4,
) {
    let joint_count = joint_gltf_nodes.len();
    let mut storage_matrix = 0usize;
    let mut storage_decomposed = 0usize;
    let mut trs_identity = 0usize;
    let mut matrix_non_identity = 0usize;
    let mut matrix_ignored_would_break = 0usize;

    let (hierarchy_mismatch, ibm_pos_count, skel_span_hier) =
        skin_bind_hierarchy_ibm_stats(hierarchy_globals, part_ibm);

    for (ji, &node_ix) in joint_gltf_nodes.iter().enumerate() {
        let Some(node) = doc.nodes().nth(node_ix) else {
            continue;
        };
        let raw = gltf_node_transform_raw(&node);
        match raw.storage {
            "Matrix" => storage_matrix += 1,
            _ => storage_decomposed += 1,
        }
        if raw.trs_is_identity {
            trs_identity += 1;
        }
        if !raw.matrix_is_identity {
            matrix_non_identity += 1;
        }
        if raw.storage == "Matrix" && !raw.matrix_is_identity {
            let ignore_t = mat4_translation(gltf_node_local_ignore_matrix(&node));
            let matrix_t = mat4_translation(Mat4::from_cols_array_2d(&raw.matrix_cols));
            if at_origin(ignore_t) && t_len(matrix_t) > 1e-3 {
                matrix_ignored_would_break += 1;
            }
        }
        let _ = ji;
        let _ = node_ix;
    }

    let skel_span_runtime = skeleton_span(runtime_globals);
    let skel_span_mesh = skeleton_extent_mesh_space(runtime_globals, mesh_normalize);
    let mesh_sp = mesh_span(mesh_vertices);
    let degenerate = skel_span_mesh < 0.001 && mesh_sp > 0.1;
    let runtime_extra = bones_runtime.saturating_sub(bones_skin);

    let summary = format!(
        "[SKIN_XFORM] {label} summary | bones_skin={bones_skin} bones_runtime={bones_runtime} \
         runtime_extra={runtime_extra} remap_drop={remap_drop} bind_pose_from_ibm={bind_pose_from_ibm} \
         storage Matrix={storage_matrix} Decomposed={storage_decomposed} \
         trs_identity={trs_identity} matrix_non_id={matrix_non_identity} \
         ignore_matrix_would_break={matrix_ignored_would_break} \
         hierarchy_ibm_mismatch={hierarchy_mismatch} ibm_has_pos={ibm_pos_count} \
         skel_span_hier={skel_span_hier:.3}m skel_span_runtime={skel_span_runtime:.3}m \
         skel_span_mesh={skel_span_mesh:.3}m mesh_span={mesh_sp:.3}m degenerate={degenerate}"
    );

    if degenerate || (!bind_pose_from_ibm && hierarchy_mismatch > joint_count / 2) {
        log::warn!("{summary}");
        if bind_pose_from_ibm && degenerate {
            log::warn!(
                "[SKIN_XFORM] {label} hint | bind_pose_from_ibm activo pero skel_span_runtime≈0 \
                 → revisar IBM singulares o inversas fallidas"
            );
        } else if matrix_ignored_would_break > 0 {
            log::warn!(
                "[SKIN_XFORM] {label} hint | ignore_matrix_would_break>0 → nodos con node.matrix; loader actual SÍ usa matrix"
            );
        } else if hierarchy_mismatch > 0 && storage_matrix == 0 && !bind_pose_from_ibm {
            log::warn!(
                "[SKIN_XFORM] {label} hint | nodos Decomposed TRS≈id + IBM con posición → bind en IBM; \
                 ignore_matrix_would_break solo aplica a storage=Matrix (aquí={storage_matrix})"
            );
        }
    } else {
        log::info!("{summary}");
    }

    let mut probe_indices: Vec<usize> = Vec::new();
    for needle in PROBE_NAME_NEEDLES {
        if let Some(ji) = joint_names
            .iter()
            .position(|n| n.contains(needle))
        {
            if !probe_indices.contains(&ji) {
                probe_indices.push(ji);
            }
        }
    }
    for (ji, &node_ix) in joint_gltf_nodes.iter().enumerate() {
        if probe_indices.len() >= PROBE_NAME_NEEDLES.len() + PROBE_MISMATCH_MAX {
            break;
        }
        if probe_indices.contains(&ji) {
            continue;
        }
        let engine_t = mat4_translation(
            runtime_globals.get(ji).copied().unwrap_or(Mat4::IDENTITY),
        );
        let ibm_t = mat4_from_ibm_inverse(&part_ibm.get(ji).copied().unwrap_or([[0.0; 4]; 4]))
            .map(|m| mat4_translation(m))
            .unwrap_or([0.0; 3]);
        if at_origin(engine_t) && !at_origin(ibm_t) {
            probe_indices.push(ji);
        }
        let _ = node_ix;
    }

    for &ji in &probe_indices {
        let node_ix = joint_gltf_nodes[ji];
        let name = joint_names.get(ji).map(|s| s.as_str()).unwrap_or("?");
        let parent = all_node_parents.get(&node_ix).copied();
        log::info!(
            "[SKIN_XFORM] {label} bone | {}",
            format_node_probe(
                doc,
                node_ix,
                name,
                parent,
                part_ibm,
                ji,
                bind_local,
                runtime_globals,
            )
        );
    }

    if let Some(&ji) = probe_indices.first() {
        let node_ix = joint_gltf_nodes[ji];
        let name = joint_names.get(ji).map(|s| s.as_str()).unwrap_or("?");
        let chain = chain_to_root(node_ix, all_node_parents);
        let mut chain_parts = Vec::new();
        for (depth, &idx) in chain.iter().enumerate() {
            let Some(node) = doc.nodes().nth(idx) else {
                continue;
            };
            let nname = node.name().unwrap_or("?");
            let raw = gltf_node_transform_raw(&node);
            let loader_t = mat4_translation(node_local_matrix(&node));
            let parent = all_node_parents.get(&idx).copied();
            chain_parts.push(format!(
                "[{depth}]node{idx}:{nname} parent={parent:?} storage={} trs_id={} matrix_id={} \
                 T={:?} loader_T={loader_t:?}",
                raw.storage,
                raw.trs_is_identity,
                raw.matrix_is_identity,
                raw.translation,
            ));
        }
        log::info!(
            "[SKIN_XFORM] {label} chain_to_root {name} | {}",
            chain_parts.join(" → ")
        );
    }
}

fn joint_name_matches_needle(name: &str, needle: &str) -> bool {
    let n = name.to_ascii_lowercase().replace(':', "").replace('_', "");
    let needle = needle.to_ascii_lowercase();
    n.contains(&needle)
}

fn find_joint_by_needles(joint_names: &[String], needles: &[&str]) -> Option<usize> {
    for needle in needles {
        if let Some(ji) = joint_names
            .iter()
            .position(|n| joint_name_matches_needle(n, needle))
        {
            return Some(ji);
        }
    }
    None
}

fn bind_palette_matrix(
    runtime_global: Mat4,
    inverse_bind: &[[[f32; 4]; 4]],
    ji: usize,
    mesh_normalize: Mat4,
) -> Mat4 {
    let default_ibm = [[0.0; 4]; 4];
    let ibm = inverse_bind.get(ji).copied().unwrap_or(default_ibm);
    let g2b = Mat4::from_cols_array_2d(&ibm);
    let inv_norm = mesh_normalize.inverse();
    mesh_normalize * runtime_global * g2b * inv_norm
}

fn format_xyz(v: Vec3) -> String {
    format!("({:.4},{:.4},{:.4})", v.x, v.y, v.z)
}

fn format_mat4_translation(m: Mat4) -> String {
    format_xyz(m.transform_point3(Vec3::ZERO))
}

/// Compara espacio jerárquico (`hierarchy_globals`) vs runtime IBM (`runtime_globals`).
/// Imprime huesos clave y, en debug, todos los joints con local/hier/runtime/IBM/palette bind.
pub fn log_skin_dual_space_report(
    label: &str,
    joint_names: &[String],
    bind_local: &[Mat4],
    hierarchy_globals: &[Mat4],
    runtime_globals: &[Mat4],
    inverse_bind: &[[[f32; 4]; 4]],
    mesh_normalize: Mat4,
) {
    let joint_count = joint_names
        .len()
        .min(bind_local.len())
        .min(hierarchy_globals.len())
        .min(runtime_globals.len())
        .min(inverse_bind.len());

    let hier_collapsed = hierarchy_globals
        .iter()
        .take(joint_count)
        .filter(|g| {
            let t = mat4_translation(**g);
            Vec3::from(t).length() < 1e-3
        })
        .count();
    let runtime_span = skeleton_span(runtime_globals);
    let hier_span = skeleton_span(hierarchy_globals);

    log::info!(
        "[SKIN_DUAL] {label} resumen | joints={joint_count} hier_collapsed={hier_collapsed}/{joint_count} \
         skel_span_hier={hier_span:.4}m skel_span_runtime={runtime_span:.4}m \
         bind_pose_espacios_distintos={}",
        hier_span < 0.001 && runtime_span > 0.1,
    );

    for needle in DUAL_SPACE_BONE_NEEDLES {
        let Some(ji) = find_joint_by_needles(joint_names, &[*needle]) else {
            log::debug!("[SKIN_DUAL] {label} bone={needle} no encontrado en joint_names");
            continue;
        };
        let name = joint_names.get(ji).map(String::as_str).unwrap_or("?");
        let local_t = format_mat4_translation(bind_local[ji]);
        let hier_t = format_mat4_translation(
            hierarchy_globals.get(ji).copied().unwrap_or(Mat4::IDENTITY),
        );
        let runtime_t = format_mat4_translation(
            runtime_globals.get(ji).copied().unwrap_or(Mat4::IDENTITY),
        );
        let ibm_t = mat4_from_ibm_inverse(
            &inverse_bind.get(ji).copied().unwrap_or([[0.0; 4]; 4]),
        )
        .map(format_mat4_translation)
        .unwrap_or_else(|| "(ibm singular)".to_string());
        let palette_t = format_mat4_translation(bind_palette_matrix(
            runtime_globals.get(ji).copied().unwrap_or(Mat4::IDENTITY),
            inverse_bind,
            ji,
            mesh_normalize,
        ));
        log::info!(
            "[SKIN_DUAL] {label} bone={name} ui={ji} | local_T={local_t} hier_T={hier_t} \
             runtime_T={runtime_t} ibm_inv_T={ibm_t} palette_bind_T={palette_t}",
        );
    }

    for ji in 0..joint_count {
        let name = joint_names.get(ji).map(String::as_str).unwrap_or("?");
        let local_t = format_mat4_translation(bind_local[ji]);
        let hier_t = format_mat4_translation(
            hierarchy_globals.get(ji).copied().unwrap_or(Mat4::IDENTITY),
        );
        let runtime_t = format_mat4_translation(
            runtime_globals.get(ji).copied().unwrap_or(Mat4::IDENTITY),
        );
        let ibm_t = mat4_from_ibm_inverse(
            &inverse_bind.get(ji).copied().unwrap_or([[0.0; 4]; 4]),
        )
        .map(format_mat4_translation)
        .unwrap_or_else(|| "(ibm singular)".to_string());
        let palette_t = format_mat4_translation(bind_palette_matrix(
            runtime_globals.get(ji).copied().unwrap_or(Mat4::IDENTITY),
            inverse_bind,
            ji,
            mesh_normalize,
        ));
        log::debug!(
            "[SKIN_JOINT] {label} ui={ji} name={name} | local_T={local_t} hier_T={hier_t} \
             runtime_T={runtime_t} ibm_inv_T={ibm_t} palette_bind_T={palette_t}",
        );
    }
}
