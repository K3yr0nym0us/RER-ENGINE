//! Construcción del `ModelAsset` glTF: mallas, piel, animaciones y normalización.

use super::*;

pub(crate) fn load_gltf_asset_from_file(
    file: &GltfFile,
    normalize_to_extent: Option<f32>,
) -> Option<Arc<ModelAsset>> {
    let doc = &file.doc;
    let buffers = &file.buffers;
    let scene = doc.default_scene().or_else(|| doc.scenes().next())?;
    let scene_parents = build_gltf_node_parents(&scene);
    let all_node_parents = build_gltf_all_node_parents(doc);

    let mut skinned_nodes: Vec<SkinnedMeshNode> = Vec::new();
    for root in scene.nodes() {
        collect_gltf_skinned_mesh_nodes(root, None, &mut skinned_nodes);
    }
    if skinned_nodes.is_empty() {
        if doc.skins().next().is_some() {
            let label = std::path::Path::new(&file.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&file.path);
            skin_diag::log_skinned_unavailable(
                label,
                "glTF tiene skins pero ningún nodo mesh+skin reconocido",
            );
        }
        return None;
    }

    let model_label = file.path.clone();
    let label = std::path::Path::new(&model_label)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&model_label)
        .to_string();

    let mut unified = build_gltf_unified_skeleton(doc, buffers, &skinned_nodes)?;
    let skin_joint_set: HashSet<usize> = unified.joint_gltf_nodes.iter().copied().collect();
    extend_gltf_unified_with_skin_skeleton_roots(doc, &mut unified);
    extend_gltf_unified_with_scene_ancestors(doc, &all_node_parents, &mut unified);
    extend_gltf_unified_with_anim_nodes(doc, &mut unified);
    let joint_count = unified.joint_gltf_nodes.len();
    let node_to_joint = unified.node_to_unified.clone();
    let joint_gltf_nodes = unified.joint_gltf_nodes.clone();
    let joint_names = unified.joint_names.clone();
    let joint_origins = unified.joint_origins.clone();

    let chain_nodes = collect_gltf_scene_chain_nodes(&joint_gltf_nodes, &all_node_parents);
    let mut gltf_bind_node_local = HashMap::new();
    for &node_ix in &chain_nodes {
        if let Some(n) = doc.nodes().nth(node_ix) {
            gltf_bind_node_local.insert(node_ix, node_local_matrix(&n));
        }
    }

    let mut asset_parts: Vec<SkinnedMeshPart> = Vec::new();
    let mut remap_drops = skin_diag::RemapDropCollector::with_max_samples(8);
    let mut v0_pre_remap: Option<SkinnedVertex> = None;
    for entry in &skinned_nodes {
        let node_index = entry.node_index;
        let node = match doc.nodes().nth(node_index) {
            Some(n) => n,
            None => continue,
        };
        let node_skin = node.skin().or_else(|| doc.skins().nth(entry.skin_index))?;
        let mesh = match node.mesh() {
            Some(m) => m,
            None => continue,
        };
        let mesh_bind_world = world_matrix_for_gltf_node_index(doc, &scene_parents, node_index);
        let (part_ibm, _ibm_fb) = part_inverse_bind_for_gltf_skin(
            &node_skin,
            buffers,
            &node_to_joint,
            joint_count,
            mesh_bind_world,
            doc,
            &all_node_parents,
        );
        let part_name = node
            .name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("part_{}", asset_parts.len()));
        let primitives: Vec<_> = mesh.primitives().collect();
        for (prim_i, primitive) in primitives.iter().enumerate() {
            let Some(mut data) =
                read_skinned_gltf_primitive(primitive, buffers, &file.material_smallest_albedos)
            else {
                log::warn!(
                    "[SKIN_ORDER] {label} primitiva {part_name} p{prim_i} omitida (sin POSITION/JOINTS_0/WEIGHTS_0)"
                );
                continue;
            };
            if v0_pre_remap.is_none() && !data.vertices.is_empty() {
                v0_pre_remap = Some(data.vertices[0]);
            }
            remap_gltf_vertex_joints_to_unified(
                &mut data,
                &node_skin,
                &node_to_joint,
                &mut remap_drops,
            );
            let material_index = primitive.material().index().unwrap_or(0) as u32;
            let name = if primitives.len() > 1 {
                format!("{part_name}_m{material_index}_p{prim_i}")
            } else {
                part_name.clone()
            };
            let mut part = SkinnedMeshPart {
                name,
                material_index,
                mesh: data,
                mesh_bind_world,
                inverse_bind: part_ibm.clone(),
            };
            bake_gltf_mesh_bind_world(&mut part);
            asset_parts.push(part);
        }
    }

    if asset_parts.is_empty() {
        return None;
    }

    let bind_local: Vec<Mat4> = joint_gltf_nodes
        .iter()
        .map(|&node_ix| {
            doc.nodes()
                .nth(node_ix)
                .map(|n| node_local_matrix(&n))
                .unwrap_or(Mat4::IDENTITY)
        })
        .collect();

    let globals = compute_gltf_joint_worlds(
        &joint_gltf_nodes,
        &bind_local,
        &all_node_parents,
        &gltf_bind_node_local,
    );

    let (mut default_ibm, _ibm_gaps_filled) =
        merge_gltf_unified_inverse_bind(&asset_parts, joint_count);
    fill_gltf_part_inverse_bind_gaps(&mut asset_parts, &default_ibm);

    let bind_pose_from_ibm = asset_parts.first().is_some_and(|first| {
        detect_bind_pose_from_ibm(&globals, &default_ibm, &first.mesh.vertices)
    });

    let mut runtime_globals = if bind_pose_from_ibm {
        gltf_bind_globals_from_ibm(&default_ibm, &bind_local, &bind_local)
    } else {
        globals.clone()
    };

    if bind_pose_from_ibm
        && normalize_to_extent.is_none()
        && apply_editor_upright_ibm_skinned(
            &mut asset_parts,
            &mut default_ibm,
            &bind_local,
            &joint_names,
        )
    {
        fill_gltf_part_inverse_bind_gaps(&mut asset_parts, &default_ibm);
        runtime_globals = gltf_bind_globals_from_ibm(&default_ibm, &bind_local, &bind_local);
    }

    let rigid_ibm = Some(default_ibm.clone());
    let skinned_part_count = asset_parts.len();
    let loaded_skinned_nodes: HashSet<usize> = skinned_nodes.iter().map(|n| n.node_index).collect();
    let mut scene_mesh_nodes: Vec<(usize, String)> = Vec::new();
    for root in scene.nodes() {
        collect_gltf_scene_mesh_nodes(root, &mut scene_mesh_nodes);
    }
    let mut rigid_candidate_count = 0usize;
    for (node_index, _node_name) in &scene_mesh_nodes {
        if !loaded_skinned_nodes.contains(node_index) {
            rigid_candidate_count += 1;
        }
    }
    if let Some(ibm) = rigid_ibm {
        for (node_index, node_name) in &scene_mesh_nodes {
            if loaded_skinned_nodes.contains(node_index) {
                continue;
            }
            if should_skip_gltf_rigid_mesh(node_name, skinned_part_count, rigid_candidate_count) {
                continue;
            }
            let (mesh_bind_world, mesh_world_ref) =
                gltf_rigid_mesh_spatial_reference(doc, buffers, &scene_parents, *node_index);
            let Some(joint_ui) = resolve_rigid_joint_ui(
                *node_index,
                mesh_world_ref,
                &all_node_parents,
                &node_to_joint,
                &skin_joint_set,
                &joint_origins,
                &runtime_globals,
            ) else {
                log::warn!(
                    "[model_asset] {label} malla «{node_name}» sin skin ni joint ancestro — omitida"
                );
                continue;
            };
            let node = match doc.nodes().nth(*node_index) {
                Some(n) => n,
                None => continue,
            };
            let mesh = match node.mesh() {
                Some(m) => m,
                None => continue,
            };
            let primitives: Vec<_> = mesh.primitives().collect();
            for (prim_i, primitive) in primitives.iter().enumerate() {
                let Some(data) = read_gltf_rigid_primitive(
                    primitive,
                    buffers,
                    &file.material_smallest_albedos,
                    joint_ui as u32,
                ) else {
                    log::warn!(
                        "[model_asset] {label} primitiva rígida {node_name} p{prim_i} omitida (sin POSITION)"
                    );
                    continue;
                };
                let material_index = primitive.material().index().unwrap_or(0) as u32;
                let name = if primitives.len() > 1 {
                    format!("{node_name}_rigid_m{material_index}_p{prim_i}")
                } else {
                    format!("{node_name}_rigid")
                };
                let mut part = SkinnedMeshPart {
                    name,
                    material_index,
                    mesh: data,
                    mesh_bind_world,
                    inverse_bind: ibm.clone(),
                };
                bake_gltf_mesh_bind_world(&mut part);
                asset_parts.push(part);
            }
        }
    }

    remap_drops.log_if_any(&label);

    let mut mesh_normalize = if let Some(target_height) = normalize_to_extent {
        let upright = if bind_pose_from_ibm {
            let ibm_globals = gltf_bind_globals_from_ibm(&default_ibm, &bind_local, &bind_local);
            let joint_positions: Vec<Vec3> = ibm_globals
                .iter()
                .map(|m| m.transform_point3(Vec3::ZERO))
                .collect();
            upright_quat_from_bind_joints(&joint_names, &joint_positions).or_else(|| {
                let (min, max) = gltf_skinned_parts_bounds(&asset_parts)?;
                let span = Vec3::from_array(max) - Vec3::from_array(min);
                let max_axis = span.x.max(span.y).max(span.z);
                if span.y < max_axis * 0.82 {
                    let points = gltf_skinned_sample_points(&asset_parts);
                    Some(
                        crate::engine3d::config_3d::mesh_3d::upright_quat_from_vertices_bounds(
                            min, max, &points,
                        ),
                    )
                } else {
                    Some(Quat::IDENTITY)
                }
            })?
        } else {
            upright_quat_for_gltf_play_character(
                &asset_parts,
                &joint_names,
                &joint_gltf_nodes,
                &bind_local,
                &all_node_parents,
                &gltf_bind_node_local,
            )?
        };
        let joint_positions = if bind_pose_from_ibm {
            let ibm_globals = gltf_bind_globals_from_ibm(&default_ibm, &bind_local, &bind_local);
            ibm_globals
                .iter()
                .map(|m| m.transform_point3(Vec3::ZERO))
                .collect()
        } else {
            bind_joint_world_positions(
                &joint_gltf_nodes,
                &bind_local,
                &all_node_parents,
                &gltf_bind_node_local,
            )
        };
        let (height, center) =
            gltf_play_bind_height_and_center(&asset_parts, &joint_positions, upright)?;
        let scale = (target_height / height).clamp(0.001, 50.0);
        let mut norm = Mat4::from_scale(Vec3::splat(scale))
            * Mat4::from_translation(-center)
            * Mat4::from_quat(upright);
        let inv_norm = norm.inverse();
        for part in asset_parts.iter_mut() {
            apply_normalize_to_skinned_vertices(&mut part.mesh.vertices, norm);
            if bind_pose_from_ibm {
                for ibm in part.inverse_bind.iter_mut() {
                    let m = Mat4::from_cols_array_2d(ibm);
                    *ibm = (m * inv_norm).to_cols_array_2d();
                }
            }
        }
        if bind_pose_from_ibm {
            for ibm in default_ibm.iter_mut() {
                if gltf_ibm_is_populated(ibm) {
                    let m = Mat4::from_cols_array_2d(ibm);
                    *ibm = (m * inv_norm).to_cols_array_2d();
                }
            }
            fill_gltf_part_inverse_bind_gaps(&mut asset_parts, &default_ibm);
        }
        let bind_center = if bind_pose_from_ibm {
            gltf_skinned_parts_bounds(&asset_parts)
                .map(|(min, max)| (Vec3::from_array(min) + Vec3::from_array(max)) * 0.5)
        } else {
            gltf_play_bind_pose_aabb_center(
                &asset_parts,
                &joint_gltf_nodes,
                &bind_local,
                &scene_parents,
                &gltf_bind_node_local,
                norm,
            )
        };
        if let Some(bind_center) = bind_center
            && bind_center.length_squared() > 1e-10
        {
            let fix = Mat4::from_translation(-bind_center);
            let fix_inv = fix.inverse();
            for part in asset_parts.iter_mut() {
                apply_normalize_to_skinned_vertices(&mut part.mesh.vertices, fix);
                if bind_pose_from_ibm {
                    for ibm in part.inverse_bind.iter_mut() {
                        let m = Mat4::from_cols_array_2d(ibm);
                        *ibm = (m * fix_inv).to_cols_array_2d();
                    }
                }
            }
            if bind_pose_from_ibm {
                for ibm in default_ibm.iter_mut() {
                    if gltf_ibm_is_populated(ibm) {
                        let m = Mat4::from_cols_array_2d(ibm);
                        *ibm = (m * fix_inv).to_cols_array_2d();
                    }
                }
            }
            norm = fix * norm;
        }
        if bind_pose_from_ibm {
            Mat4::IDENTITY
        } else {
            norm
        }
    } else {
        Mat4::IDENTITY
    };

    if bind_pose_from_ibm
        && normalize_to_extent.is_none()
        && editor_normalize_oversized_ibm_skinned(&mut asset_parts, skinned_part_count)
    {
        mesh_normalize = Mat4::IDENTITY;
        default_ibm = merge_gltf_unified_inverse_bind(&asset_parts, joint_count).0;
        fill_gltf_part_inverse_bind_gaps(&mut asset_parts, &default_ibm);
    }

    let mut clips = Vec::new();
    for anim in doc.animations() {
        if let Some(clip) = parse_gltf_animation(&anim, buffers, &node_to_joint)
            && !clip.channels.is_empty()
        {
            clips.push(clip);
        }
    }

    Some(Arc::new(ModelAsset {
        path: file.path.clone(),
        joint_parents: vec![None; joint_count],
        inverse_bind: default_ibm,
        bind_local,
        joint_prefix_world: vec![Mat4::IDENTITY; joint_count],
        joint_names,
        parts: asset_parts,
        clips,
        mesh_normalize,
        facing_forward_xz: {
            let node_ix = gltf_primary_mesh_node_index(file);
            node_ix
                .map(|ix| {
                    crate::engine3d::config_3d::mesh_3d::forward_xz_from_node_world(
                        world_matrix_for_gltf_node_index(doc, &scene_parents, ix),
                    )
                })
                .unwrap_or(glam::Vec2::new(0.0, 1.0))
        },
        joint_gltf_nodes,
        gltf_scene_parents: all_node_parents,
        gltf_bind_node_local,
        bind_pose_from_ibm,
    }))
}
