//! Carga FBX (ufbx): esqueleto, malla skinned y clips.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::mesh::SkinnedVertex;

use super::model_asset::{
    self, AnimChannel, AnimKeyframe, AnimProperty, AnimationClip, MAX_JOINTS, ModelAsset,
    ModelClipInfo, SkinnedMeshData, SkinnedMeshPart,
};

/// Metadatos de clips del FBX sin cargar malla skinned (para listar en UI si falla el asset completo).
pub fn list_fbx_clip_infos(path: &Path) -> Vec<ModelClipInfo> {
    let mut opts = ufbx::LoadOpts::default();
    opts.generate_missing_normals = true;
    opts.load_external_files = true;
    opts.target_axes = ufbx::CoordinateAxes::right_handed_y_up();
    opts.space_conversion = ufbx::SpaceConversion::ModifyGeometry;

    let Ok(scene) = ufbx::load_file(path.to_str().unwrap_or(""), opts) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for stack in scene.anim_stacks.iter() {
        let stack = stack.as_ref();
        let name = if stack.element.name.is_empty() {
            format!("AnimStack_{}", stack.element.element_id)
        } else {
            stack.element.name.to_string()
        };
        let mut duration_s = 1.0f32 / 30.0;
        if let Ok(baked) = ufbx::bake_anim(&scene, &stack.anim, ufbx::BakeOpts::default()) {
            let mut max_t = 0.0f64;
            for nb in baked.nodes.iter() {
                for key in nb.translation_keys.iter() {
                    max_t = max_t.max(key.time);
                }
                for key in nb.rotation_keys.iter() {
                    max_t = max_t.max(key.time);
                }
                for key in nb.scale_keys.iter() {
                    max_t = max_t.max(key.time);
                }
            }
            if max_t > 1e-6 {
                duration_s = max_t as f32;
            }
        }
        out.push(ModelClipInfo {
            name,
            duration_s,
            fps: 30.0,
        });
    }
    out
}

/// Como `mesh_3d::normalize_vertices_height_feet_pivot`: escala por altura Y, pies en Y=0, centro X/Z.
fn feet_pivot_normalize_mat(min: [f32; 3], max: [f32; 3], target_height: f32) -> Mat4 {
    let cx = (min[0] + max[0]) * 0.5;
    let cz = (min[2] + max[2]) * 0.5;
    let min_y = min[1];
    let height = (max[1] - min[1]).max(1e-5);
    let scale = (target_height / height).clamp(0.001, 50.0);
    let pivot = Vec3::new(cx, min_y, cz);
    Mat4::from_scale(Vec3::splat(scale)) * Mat4::from_translation(-pivot)
}

/// AABB de todas las mallas del FBX en espacio mundo (como `mesh_3d::load_fbx`).
fn fbx_scene_world_bounds(scene: &ufbx::Scene) -> Option<([f32; 3], [f32; 3])> {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for node_ref in scene.nodes.iter() {
        let node = node_ref.as_ref();
        let Some(mesh_ref) = node.mesh.as_ref() else {
            continue;
        };
        let mesh = mesh_ref.as_ref();
        let world = ufbx_matrix_to_mat4(&node.geometry_to_world);
        let mut tri_indices = vec![0u32; mesh.max_face_triangles * 3];
        let face_iter: Box<dyn Iterator<Item = u32>> = if !mesh.material_parts.is_empty() {
            Box::new(
                mesh.material_parts
                    .iter()
                    .flat_map(|part| part.face_indices.iter().copied()),
            )
        } else {
            Box::new((0..mesh.faces.len() as u32).collect::<Vec<_>>().into_iter())
        };
        for face_index in face_iter {
            let face = mesh.faces[face_index as usize];
            let num_tris = mesh.triangulate_face(&mut tri_indices, face);
            let corner_count = num_tris as usize * 3;
            for &index in &tri_indices[..corner_count] {
                let ix = index as usize;
                let pos_local = ufbx::get_vertex_vec3(&mesh.vertex_position, ix);
                let wp = world.transform_point3(Vec3::new(
                    pos_local.x as f32,
                    pos_local.y as f32,
                    pos_local.z as f32,
                ));
                let p = wp.to_array();
                for i in 0..3 {
                    min[i] = min[i].min(p[i]);
                    max[i] = max[i].max(p[i]);
                }
                any = true;
            }
        }
    }
    any.then_some((min, max))
}

// ── FBX (ufbx) ───────────────────────────────────────────────────────────────

pub(crate) fn load_fbx_asset(path: &Path, normalize_to_extent: Option<f32>) -> Option<Arc<ModelAsset>> {
    let mut opts = ufbx::LoadOpts::default();
    opts.generate_missing_normals = true;
    opts.load_external_files = true;
    opts.target_axes = ufbx::CoordinateAxes::right_handed_y_up();
    opts.target_unit_meters = 1.0;
    opts.space_conversion = ufbx::SpaceConversion::ModifyGeometry;

    let scene = ufbx::load_file(path.to_str()?, opts).ok()?;
    let has_skinned_mesh = scene.nodes.iter().any(|n| {
        n.mesh
            .as_ref()
            .map(|m| !m.skin_deformers.is_empty())
            .unwrap_or(false)
    });
    if scene.anim_stacks.is_empty() {
        if has_skinned_mesh {
            log::warn!(
                "[model_asset] FBX con skin pero sin animaciones embebidas (anim_stacks=0): {}. \
                 Reexporta el modelo con las curvas de animaci├│n incluidas en el FBX/GLB \
                 (evita referencias solo a .mb u otros archivos externos).",
                path.display()
            );
        }
        return None;
    }

    let skinned_nodes = collect_fbx_skinned_nodes(&scene);
    if skinned_nodes.is_empty() {
        log::warn!(
            "[model_asset] FBX sin malla skinned (skin_deformers vac├¡o): {}",
            path.display()
        );
        return None;
    }

    let mut skeleton = build_fbx_skeleton_from_scene(&scene).or_else(|| {
        log::warn!("[model_asset] sin huesos en skins del FBX: {}", path.display());
        None
    })?;
    extend_fbx_skeleton_with_ancestors(&scene, &mut skeleton, None);
    if skeleton.bone_to_joint.len() > MAX_JOINTS {
        log::warn!(
            "[model_asset] FBX tiene {} huesos (m├íx {MAX_JOINTS}): {}",
            skeleton.bone_to_joint.len(),
            path.display()
        );
        return None;
    }
    rebuild_fbx_skeleton_tables(&scene, &mut skeleton).or_else(|| {
        log::warn!(
            "[model_asset] no se pudo resolver nodos de hueso en: {}",
            path.display()
        );
        None
    })?;
    let joint_count = skeleton.bone_order.len();
    let node_to_joint = &skeleton.bone_to_joint;

    let fbx_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut asset_parts: Vec<SkinnedMeshPart> = Vec::new();
    for (skinned_mesh_node, mesh, skin_def) in &skinned_nodes {
        let world = ufbx_matrix_to_mat4(&skinned_mesh_node.geometry_to_world);
        let Some(mesh_data) =
            read_skinned_fbx_mesh(mesh, skin_def, fbx_dir, world, node_to_joint)
        else {
            continue;
        };
        let part_ibm = inverse_bind_for_skin(joint_count, skin_def, node_to_joint);
        let part_name = if skinned_mesh_node.element.name.is_empty() {
            format!("part_{}", asset_parts.len())
        } else {
            skinned_mesh_node.element.name.to_string()
        };
        asset_parts.push(SkinnedMeshPart {
            name: part_name,
            material_index: 0,
            mesh: mesh_data,
            mesh_bind_world: world,
            inverse_bind: part_ibm,
        });
    }
    if asset_parts.is_empty() {
        log::warn!("[model_asset] FBX sin geometr├¡a skinned legible: {}", path.display());
        return None;
    }

    let target_height = normalize_to_extent.unwrap_or(1.8);
    let mesh_normalize = fbx_scene_world_bounds(&scene)
        .map(|(min, max)| feet_pivot_normalize_mat(min, max, target_height))
        .unwrap_or(Mat4::IDENTITY);
    for part in asset_parts.iter_mut() {
        model_asset::apply_normalize_to_skinned_vertices(&mut part.mesh.vertices, mesh_normalize);
    }

    log::info!(
        "[model_asset] esqueleto FBX: {} huesos, {} pieza(s) skinned para {}",
        skeleton.bone_to_joint.len(),
        asset_parts.len(),
        path.display()
    );

    let mut clips = Vec::new();
    for stack in scene.anim_stacks.iter() {
        let stack: &ufbx::AnimStack = stack.as_ref();
        let name = if stack.element.name.is_empty() {
            format!("AnimStack_{}", stack.element.element_id)
        } else {
            stack.element.name.to_string()
        };
        if let Some(clip) = parse_fbx_anim_stack(stack, &scene, &node_to_joint, &name) {
            clips.push(clip);
        }
    }

    if clips.is_empty() {
        log::warn!(
            "[model_asset] FBX con anim_stacks pero sin canales mapeados a huesos: {}",
            path.display()
        );
        return None;
    }

    let joint_count = skeleton.joint_parents.len();
    Some(Arc::new(ModelAsset {
        path: path.display().to_string(),
        joint_parents: skeleton.joint_parents,
        inverse_bind: skeleton.inverse_bind,
        bind_local: skeleton.bind_local,
        joint_prefix_world: vec![Mat4::IDENTITY; joint_count],
        joint_names: skeleton.joint_names,
        parts: asset_parts,
        clips,
        mesh_normalize,
        facing_forward_xz: crate::config_3d::fbx_facing::forward_xz_from_ufbx_front(
            scene.settings.axes.front,
        ),
        joint_gltf_nodes: Vec::new(),
        gltf_scene_parents: HashMap::new(),
        gltf_bind_node_local: HashMap::new(),
        bind_pose_from_ibm: false,
    }))
}

type FbxSkinnedNode<'a> = (&'a ufbx::Node, &'a ufbx::Mesh, &'a ufbx::SkinDeformer);

fn collect_fbx_skinned_nodes(scene: &ufbx::Scene) -> Vec<FbxSkinnedNode<'_>> {
    let mut out: Vec<FbxSkinnedNode<'_>> = Vec::new();
    for node_ref in scene.nodes.iter() {
        let node = node_ref.as_ref();
        let Some(mesh) = node.mesh.as_ref() else {
            continue;
        };
        let mesh = mesh.as_ref();
        let Some(skin) = mesh.skin_deformers.first() else {
            continue;
        };
        out.push((node, mesh, skin.as_ref()));
    }
    out.sort_by(|a, b| {
        let wa = a.2.vertices.len();
        let wb = b.2.vertices.len();
        wb.cmp(&wa)
    });
    out
}

fn build_fbx_skeleton_from_scene(scene: &ufbx::Scene) -> Option<FbxSkeletonData> {
    let mut bone_to_joint: HashMap<u32, usize> = HashMap::new();
    let mut bone_order: Vec<u32> = Vec::new();
    for node_ref in scene.nodes.iter() {
        let Some(mesh) = node_ref.mesh.as_ref() else {
            continue;
        };
        for skin_ref in mesh.skin_deformers.iter() {
            for cluster in skin_ref.clusters.iter() {
                if let Some(bone) = cluster.bone_node.as_ref() {
                    let tid = bone.element.typed_id;
                    if !bone_to_joint.contains_key(&tid) {
                        bone_to_joint.insert(tid, bone_order.len());
                        bone_order.push(tid);
                    }
                }
            }
        }
    }
    if bone_order.is_empty() {
        return None;
    }
    Some(FbxSkeletonData {
        bone_order,
        bone_to_joint,
        joint_parents: Vec::new(),
        inverse_bind: Vec::new(),
        bind_local: Vec::new(),
        joint_names: Vec::new(),
    })
}

fn inverse_bind_for_skin(
    joint_count: usize,
    skin_def: &ufbx::SkinDeformer,
    bone_to_joint: &HashMap<u32, usize>,
) -> Vec<[[f32; 4]; 4]> {
    let mut ibm = vec![Mat4::IDENTITY.to_cols_array_2d(); joint_count];
    for cluster in skin_def.clusters.iter() {
        if let Some(bone) = cluster.bone_node.as_ref() {
            if let Some(&ji) = bone_to_joint.get(&bone.element.typed_id) {
                if ji < joint_count {
                    ibm[ji] = ufbx_matrix_to_mat4(&cluster.geometry_to_bone).to_cols_array_2d();
                }
            }
        }
    }
    ibm
}

/// Incluye nodos padre (p. ej. Armature de Mixamo) que no tienen cluster pero afectan la jerarqu├¡a.
fn extend_fbx_skeleton_with_ancestors(
    scene: &ufbx::Scene,
    skeleton: &mut FbxSkeletonData,
    stop_at_mesh_tid: Option<u32>,
) {
    let initial: Vec<u32> = skeleton.bone_order.clone();
    for &bone_tid in &initial {
        let Some(mut node) = scene.nodes.iter().find_map(|n| {
            (n.element.typed_id == bone_tid).then_some(n.as_ref())
        }) else {
            continue;
        };
        loop {
            let Some(parent) = node.parent.as_ref() else {
                break;
            };
            let parent = parent.as_ref();
            let ptid = parent.element.typed_id;
            if stop_at_mesh_tid.is_some_and(|tid| ptid == tid) {
                break;
            }
            if !skeleton.bone_to_joint.contains_key(&ptid) {
                if skeleton.bone_order.len() >= MAX_JOINTS {
                    log::warn!(
                        "[model_asset] ancestros FBX omitidos: l├¡mite {MAX_JOINTS} huesos"
                    );
                    break;
                }
                skeleton.bone_to_joint.insert(ptid, skeleton.bone_order.len());
                skeleton.bone_order.push(ptid);
            }
            node = parent;
        }
    }
}

struct FbxSkeletonData {
    bone_order: Vec<u32>,
    bone_to_joint: HashMap<u32, usize>,
    joint_parents: Vec<Option<usize>>,
    inverse_bind: Vec<[[f32; 4]; 4]>,
    bind_local: Vec<Mat4>,
    joint_names: Vec<String>,
}

fn rebuild_fbx_skeleton_tables(scene: &ufbx::Scene, skeleton: &mut FbxSkeletonData) -> Option<()> {
    let joint_count = skeleton.bone_order.len();
    let mut joint_parents = vec![None; joint_count];
    let mut bind_local = vec![Mat4::IDENTITY; joint_count];
    let mut inverse_bind = vec![Mat4::IDENTITY.to_cols_array_2d(); joint_count];
    let mut joint_names = vec![String::new(); joint_count];

    for (ji, &bone_tid) in skeleton.bone_order.iter().enumerate() {
        let bone_node = find_fbx_bone_node(scene, bone_tid)?;
        joint_names[ji] = if bone_node.element.name.is_empty() {
            format!("bone_{}", bone_node.element.element_id)
        } else {
            bone_node.element.name.to_string()
        };
        bind_local[ji] = ufbx_transform_to_mat4(&bone_node.local_transform);
        if let Some(cluster) = find_fbx_cluster_for_bone(scene, bone_tid) {
            inverse_bind[ji] = ufbx_matrix_to_mat4(&cluster.geometry_to_bone).to_cols_array_2d();
        }
        if let Some(parent) = bone_node.parent.as_ref() {
            if let Some(&pji) = skeleton.bone_to_joint.get(&parent.element.typed_id) {
                joint_parents[ji] = Some(pji);
            }
        }
    }

    skeleton.joint_parents = joint_parents;
    skeleton.bind_local = bind_local;
    skeleton.inverse_bind = inverse_bind;
    skeleton.joint_names = joint_names;
    Some(())
}

fn find_fbx_bone_node<'a>(scene: &'a ufbx::Scene, bone_tid: u32) -> Option<&'a ufbx::Node> {
    for node_ref in scene.nodes.iter() {
        let node = node_ref.as_ref();
        if node.element.typed_id == bone_tid {
            return Some(node);
        }
    }
    None
}

fn find_fbx_cluster_for_bone<'a>(
    scene: &'a ufbx::Scene,
    bone_tid: u32,
) -> Option<&'a ufbx::SkinCluster> {
    for node_ref in scene.nodes.iter() {
        let Some(mesh) = node_ref.mesh.as_ref() else {
            continue;
        };
        let mesh = mesh.as_ref();
        for skin_ref in mesh.skin_deformers.iter() {
            for cluster in skin_ref.clusters.iter() {
                let c = cluster.as_ref();
                if c.bone_node
                    .as_ref()
                    .map(|b| b.element.typed_id)
                    .is_some_and(|tid| tid == bone_tid)
                {
                    return Some(c);
                }
            }
        }
    }
    None
}

fn fbx_cluster_to_joint_index(
    skin_def: &ufbx::SkinDeformer,
    cluster_index: u32,
    bone_to_joint: &HashMap<u32, usize>,
) -> u32 {
    let ci = cluster_index as usize;
    if ci >= skin_def.clusters.len() {
        return 0;
    }
    if let Some(bone) = skin_def.clusters[ci].bone_node.as_ref() {
        if let Some(&ji) = bone_to_joint.get(&bone.element.typed_id) {
            return ji.min(MAX_JOINTS - 1) as u32;
        }
    }
    0
}

fn ufbx_transform_to_mat4(t: &ufbx::Transform) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        Vec3::new(t.scale.x as f32, t.scale.y as f32, t.scale.z as f32),
        Quat::from_xyzw(
            t.rotation.x as f32,
            t.rotation.y as f32,
            t.rotation.z as f32,
            t.rotation.w as f32,
        ),
        Vec3::new(
            t.translation.x as f32,
            t.translation.y as f32,
            t.translation.z as f32,
        ),
    )
}

fn ufbx_matrix_to_mat4(m: &ufbx::Matrix) -> Mat4 {
    Mat4::from_cols(
        glam::Vec4::new(m.m00 as f32, m.m10 as f32, m.m20 as f32, 0.0),
        glam::Vec4::new(m.m01 as f32, m.m11 as f32, m.m21 as f32, 0.0),
        glam::Vec4::new(m.m02 as f32, m.m12 as f32, m.m22 as f32, 0.0),
        glam::Vec4::new(m.m03 as f32, m.m13 as f32, m.m23 as f32, 1.0),
    )
}

fn read_skinned_fbx_mesh(
    mesh: &ufbx::Mesh,
    skin_def: &ufbx::SkinDeformer,
    fbx_dir: &Path,
    world: Mat4,
    bone_to_joint: &HashMap<u32, usize>,
) -> Option<SkinnedMeshData> {
    let normal_mat = world.inverse().transpose();
    let mut vertices: Vec<SkinnedVertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let mut tri_indices = vec![0u32; mesh.max_face_triangles * 3];
    let face_iter: Box<dyn Iterator<Item = u32>> = if !mesh.material_parts.is_empty() {
        Box::new(
            mesh.material_parts
                .iter()
                .flat_map(|part| part.face_indices.iter().copied()),
        )
    } else {
        Box::new((0..mesh.faces.len() as u32).collect::<Vec<_>>().into_iter())
    };

    for face_index in face_iter {
        let face = mesh.faces[face_index as usize];
        let num_tris = mesh.triangulate_face(&mut tri_indices, face);
        let corner_count = num_tris as usize * 3;
        for &index in &tri_indices[..corner_count] {
            let ix = index as usize;
            let pos_local = ufbx::get_vertex_vec3(&mesh.vertex_position, ix);
            let norm_local = ufbx::get_vertex_vec3(&mesh.vertex_normal, ix);
            let pos4 = world * Vec4::new(pos_local.x as f32, pos_local.y as f32, pos_local.z as f32, 1.0);
            let norm4 = normal_mat * Vec4::new(norm_local.x as f32, norm_local.y as f32, norm_local.z as f32, 0.0);
            let pos = pos4.truncate();
            let norm = norm4.truncate().normalize_or_zero();
            let uv = ufbx::get_vertex_vec2(&mesh.vertex_uv, ix);

            let vert_index = mesh.vertex_indices[ix] as usize;
            let mut joints = [0u32, 0, 0, 0];
            let mut weights = [0.0f32; 4];
            if vert_index < skin_def.vertices.len() {
                let sv = &skin_def.vertices[vert_index];
                let begin = sv.weight_begin as usize;
                let n = sv.num_weights as usize;
                for i in 0..n.min(4) {
                    if begin + i < skin_def.weights.len() {
                        let w = &skin_def.weights[begin + i];
                        joints[i] = fbx_cluster_to_joint_index(skin_def, w.cluster_index, bone_to_joint);
                        weights[i] = w.weight as f32;
                    }
                }
            }
            let sum: f32 = weights.iter().sum();
            if sum > 1e-6 {
                for w in &mut weights {
                    *w /= sum;
                }
            } else {
                weights = [1.0, 0.0, 0.0, 0.0];
            }

            vertices.push(SkinnedVertex {
                position: [pos.x, pos.y, pos.z],
                normal: [norm.x, norm.y, norm.z],
                uv: [uv.x as f32, uv.y as f32],
                joints,
                weights,
            });
            indices.push(vertices.len() as u32 - 1);
        }
    }

    if vertices.is_empty() {
        return None;
    }

    let (rgba, width, height) = fbx_texture_rgba(mesh, fbx_dir);

    Some(SkinnedMeshData {
        vertices,
        indices,
        rgba,
        width,
        height,
    })
}

fn fbx_texture_rgba(mesh: &ufbx::Mesh, fbx_dir: &Path) -> (Vec<u8>, u32, u32) {
    if let Some(mat) = mesh.materials.first().map(|m| m.as_ref()) {
        if let Some(tex) = fbx_material_texture(mat) {
            if !tex.content.is_empty() {
                if let Ok(img) = image::load_from_memory(&tex.content) {
                    let rgba = img.to_rgba8();
                    let w = rgba.width();
                    let h = rgba.height();
                    return (rgba.into_raw(), w, h);
                }
            }
            for name in [
                tex.absolute_filename.as_ref(),
                tex.filename.as_ref(),
                tex.relative_filename.as_ref(),
            ] {
                if name.is_empty() {
                    continue;
                }
                let path = Path::new(name);
                let resolved = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    fbx_dir.join(path)
                };
                if resolved.is_file() {
                    if let Ok(img) = image::open(&resolved) {
                        let rgba = img.to_rgba8();
                        let w = rgba.width();
                    let h = rgba.height();
                    return (rgba.into_raw(), w, h);
                    }
                }
            }
        }
    }
    (vec![255, 255, 255, 255], 1, 1)
}

fn fbx_material_texture<'a>(material: &'a ufbx::Material) -> Option<&'a ufbx::Texture> {
    if material.pbr.base_color.texture_enabled {
        if let Some(tex) = material.pbr.base_color.texture.as_ref() {
            return Some(tex.as_ref());
        }
    }
    if material.fbx.diffuse_color.texture_enabled {
        if let Some(tex) = material.fbx.diffuse_color.texture.as_ref() {
            return Some(tex.as_ref());
        }
    }
    material
        .textures
        .first()
        .map(|entry| entry.texture.as_ref())
}

fn parse_fbx_anim_stack(
    stack: &ufbx::AnimStack,
    scene: &ufbx::Scene,
    node_to_joint: &HashMap<u32, usize>,
    name: &str,
) -> Option<AnimationClip> {
    let anim = stack.anim.as_ref();
    let baked_root = ufbx::bake_anim(scene, anim, ufbx::BakeOpts::default()).ok()?;
    let baked = &*baked_root;

    let mut channels: Vec<AnimChannel> = Vec::new();
    let mut max_time = 0.0f32;

    for node_bake in baked.nodes.iter() {
        let node_index = node_bake.typed_id;
        let joint_index = match node_to_joint.get(&node_index) {
            Some(&ji) => ji,
            None => continue,
        };

        if !node_bake.translation_keys.is_empty() {
            let mut keyframes = Vec::new();
            for key in node_bake.translation_keys.iter() {
                let t = key.time as f32;
                max_time = max_time.max(t);
                let v = key.value;
                keyframes.push(AnimKeyframe {
                    time: t,
                    translation: Some([v.x as f32, v.y as f32, v.z as f32]),
                    rotation: None,
                    scale: None,
                });
            }
            channels.push(AnimChannel {
                joint_index,
                property: AnimProperty::Translation,
                keyframes,
            });
        }

        if !node_bake.rotation_keys.is_empty() {
            let mut keyframes = Vec::new();
            for key in node_bake.rotation_keys.iter() {
                let t = key.time as f32;
                max_time = max_time.max(t);
                let q = key.value;
                keyframes.push(AnimKeyframe {
                    time: t,
                    translation: None,
                    rotation: Some([q.x as f32, q.y as f32, q.z as f32, q.w as f32]),
                    scale: None,
                });
            }
            channels.push(AnimChannel {
                joint_index,
                property: AnimProperty::Rotation,
                keyframes,
            });
        }

        if !node_bake.scale_keys.is_empty() {
            let mut keyframes = Vec::new();
            for key in node_bake.scale_keys.iter() {
                let t = key.time as f32;
                max_time = max_time.max(t);
                let v = key.value;
                keyframes.push(AnimKeyframe {
                    time: t,
                    translation: None,
                    rotation: None,
                    scale: Some([v.x as f32, v.y as f32, v.z as f32]),
                });
            }
            channels.push(AnimChannel {
                joint_index,
                property: AnimProperty::Scale,
                keyframes,
            });
        }
    }

    if channels.is_empty() {
        return None;
    }

    let duration_s = max_time.max(1.0 / 30.0);
    Some(AnimationClip {
        name: name.to_string(),
        duration_s,
        fps: 30.0,
        channels,
    })
}


fn estimate_forward_from_normals_local(vertices: &[crate::mesh::Vertex]) -> glam::Vec2 {
    let mut sum = glam::Vec2::ZERO;
    for v in vertices {
        let n = glam::Vec2::new(v.normal[0], v.normal[2]);
        let len = n.length();
        if len > 1e-5 { sum += n / len; }
    }
    if sum.length_squared() < 1e-6 { glam::Vec2::new(0.0, 1.0) } else { sum.normalize() }
}

fn estimate_forward_torso_local(vertices: &[crate::mesh::Vertex]) -> glam::Vec2 {
    if vertices.is_empty() { return glam::Vec2::new(0.0, 1.0); }
    crate::config_3d::mesh_3d::estimate_mesh_forward_xz(vertices)
}

fn part_counts_for_play_forward(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n.is_empty() { return true; }
    const SKIP: &[&str] = &["hair","gun","weapon","boost","cape","wing","shield","backpack","acc","accessory","prop"];
    !SKIP.iter().any(|s| n.contains(s))
}

/// Forward desde piezas skinned del FBX (sin clips embebidos).
pub fn fbx_skinned_play_forward_xz(path: &Path, normalize_to_extent: f32) -> Option<glam::Vec2> {
    let mut opts = ufbx::LoadOpts::default();
    opts.generate_missing_normals = true;
    opts.load_external_files = true;
    opts.target_axes = ufbx::CoordinateAxes::right_handed_y_up();
    opts.target_unit_meters = 1.0;
    opts.space_conversion = ufbx::SpaceConversion::ModifyGeometry;
    let scene = ufbx::load_file(path.to_str()?, opts).ok()?;
    let skinned_nodes = collect_fbx_skinned_nodes(&scene);
    if skinned_nodes.is_empty() {
        return None;
    }
    let mut skeleton = build_fbx_skeleton_from_scene(&scene)?;
    extend_fbx_skeleton_with_ancestors(&scene, &mut skeleton, None);
    if skeleton.bone_to_joint.len() > MAX_JOINTS {
        return None;
    }
    rebuild_fbx_skeleton_tables(&scene, &mut skeleton)?;
    let node_to_joint = &skeleton.bone_to_joint;
    let fbx_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut parts: Vec<SkinnedMeshPart> = Vec::new();
    for (skinned_mesh_node, mesh, skin_def) in &skinned_nodes {
        let world = ufbx_matrix_to_mat4(&skinned_mesh_node.geometry_to_world);
        let Some(mesh_data) = read_skinned_fbx_mesh(mesh, skin_def, fbx_dir, world, node_to_joint)
        else {
            continue;
        };
        let part_name = if skinned_mesh_node.element.name.is_empty() {
            format!("part_{}", parts.len())
        } else {
            skinned_mesh_node.element.name.to_string()
        };
        parts.push(SkinnedMeshPart {
            name: part_name,
            material_index: 0,
            mesh: mesh_data,
            mesh_bind_world: world,
            inverse_bind: Vec::new(),
        });
    }
    if parts.is_empty() {
        return None;
    }
    let mesh_normalize = fbx_scene_world_bounds(&scene)
        .map(|(min, max)| feet_pivot_normalize_mat(min, max, normalize_to_extent))
        .unwrap_or(Mat4::IDENTITY);
    for part in parts.iter_mut() {
        model_asset::apply_normalize_to_skinned_vertices(&mut part.mesh.vertices, mesh_normalize);
    }

    let meta = crate::config_3d::fbx_facing::forward_xz_from_ufbx_front(scene.settings.axes.front);
    use crate::mesh::Vertex;
    let verts: Vec<Vertex> = parts
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
        return None;
    }
    let est_pos = crate::config_3d::mesh_3d::estimate_mesh_forward_xz(&verts);
    let est_torso = estimate_forward_torso_local(&verts);
    let est_norm = estimate_forward_from_normals_local(&verts);
    let est = if est_pos.dot(est_torso) < -0.5 {
        est_torso
    } else if meta.dot(est_norm).abs() >= meta.dot(est_pos).abs() {
        est_norm
    } else {
        est_pos
    };
    let mut resolved = crate::config_3d::fbx_facing::resolve_fbx_forward_xz(meta, est);

    if let Some(hi) = skeleton
        .joint_names
        .iter()
        .position(|n| n.to_ascii_lowercase().contains("hips"))
    {
        let (_, rot, _) = skeleton.bind_local[hi].to_scale_rotation_translation();
        let (bone_yaw, _, _) = rot.to_euler(glam::EulerRot::YXZ);
        if bone_yaw.abs() >= 0.35 {
            let (s, c) = (-bone_yaw).sin_cos();
            let fx = resolved.x;
            let fz = resolved.y;
            resolved = glam::Vec2::new(fx * c - fz * s, fx * s + fz * c).normalize_or_zero();
        }
    }
    Some(resolved)
}