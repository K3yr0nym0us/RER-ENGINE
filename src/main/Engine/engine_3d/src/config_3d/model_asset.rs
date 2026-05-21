//! Parseo de skeleton, malla skinned y clips de animación (paso aparte de `mesh_3d`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::mesh::SkinnedVertex;

pub const MAX_JOINTS: usize = 256;

#[derive(Clone, Debug)]
pub struct ModelClipInfo {
    pub name: String,
    pub duration_s: f32,
    pub fps: f32,
}

#[derive(Clone, Debug)]
pub enum AnimProperty {
    Translation,
    Rotation,
    Scale,
}

#[derive(Clone, Debug)]
pub struct AnimKeyframe {
    pub time: f32,
    pub translation: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>, // xyzw
    pub scale: Option<[f32; 3]>,
}

#[derive(Clone, Debug)]
pub struct AnimChannel {
    pub joint_index: usize,
    pub property: AnimProperty,
    pub keyframes: Vec<AnimKeyframe>,
}

#[derive(Clone, Debug)]
pub struct AnimationClip {
    pub name: String,
    pub duration_s: f32,
    pub fps: f32,
    pub channels: Vec<AnimChannel>,
}

#[derive(Clone, Debug)]
pub struct SkinnedMeshData {
    pub vertices: Vec<SkinnedVertex>,
    pub indices: Vec<u32>,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Una pieza de malla skinned (FBX suele traer varias: cuerpo, ropa, accesorios).
#[derive(Clone, Debug)]
pub struct SkinnedMeshPart {
    /// Nombre del nodo FBX/glTF (p. ej. Body, Hair) para filtrar forward del jugador.
    pub name: String,
    pub mesh: SkinnedMeshData,
    /// `geometry_to_world` del nodo de esta pieza (vértices en espacio mundo previo a `mesh_normalize`).
    pub mesh_bind_world: Mat4,
    /// `geometry_to_bone` por hueso para esta pieza (identidad si el hueso no influye aquí).
    pub inverse_bind: Vec<[[f32; 4]; 4]>,
}

#[derive(Clone, Debug)]
pub struct ModelAsset {
    pub path: String,
    pub joint_parents: Vec<Option<usize>>,
    pub inverse_bind: Vec<[[f32; 4]; 4]>,
    /// Pose local de bind por joint (base antes de mezclar clips).
    pub bind_local: Vec<Mat4>,
    pub joint_names: Vec<String>,
    pub parts: Vec<SkinnedMeshPart>,
    pub clips: Vec<AnimationClip>,
    /// Centrado/escala aplicados a la malla skinned (misma convención que `mesh_3d`).
    pub mesh_normalize: Mat4,
    /// Solo FBX: `settings.axes.front` del archivo. glTF no usa este campo (orientación pendiente).
    pub facing_forward_xz: glam::Vec2,
}

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

pub fn load_model_asset(path: &Path, normalize_to_extent: Option<f32>) -> Option<Arc<ModelAsset>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "glb" | "gltf" => load_gltf_asset(path, normalize_to_extent),
        "fbx" => load_fbx_asset(path, normalize_to_extent),
        _ => None,
    }
}

fn load_gltf_asset(path: &Path, normalize_to_extent: Option<f32>) -> Option<Arc<ModelAsset>> {
    let (doc, buffers, images) = gltf::import(path).ok()?;
    let scene = doc.default_scene().or_else(|| doc.scenes().next())?;

    let mut skinned_node_index: Option<usize> = None;
    for root in scene.nodes() {
        find_gltf_skinned_node_index(root, &mut skinned_node_index);
        if skinned_node_index.is_some() {
            break;
        }
    }
    let node_index = skinned_node_index?;
    let node = doc.nodes().nth(node_index)?;
    let skin = node.skin()?;
    let mesh = node.mesh()?;

    let joint_nodes: Vec<gltf::Node> = skin.joints().collect();
    if joint_nodes.is_empty() || joint_nodes.len() > MAX_JOINTS {
        return None;
    }

    let node_parents = build_gltf_node_parents(scene);

    let node_to_joint: HashMap<usize, usize> = joint_nodes
        .iter()
        .enumerate()
        .map(|(ji, n)| (n.index(), ji))
        .collect();

    let mut joint_parents = vec![None; joint_nodes.len()];
    let mut joint_names = Vec::with_capacity(joint_nodes.len());
    for (ji, jn) in joint_nodes.iter().enumerate() {
        joint_names.push(
            jn.name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("joint_{ji}")),
        );
        if let Some(&parent_node) = node_parents.get(&jn.index()) {
            if let Some(&pji) = node_to_joint.get(&parent_node) {
                joint_parents[ji] = Some(pji);
            }
        }
    }

    let mut inverse_bind = vec![[[0.0; 4]; 4]; joint_nodes.len()];
    let skin_reader = skin.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()));
    if let Some(iter) = skin_reader.read_inverse_bind_matrices() {
        for (ji, m) in iter.enumerate() {
            if ji < inverse_bind.len() {
                inverse_bind[ji] = m;
            }
        }
    }
    for (ji, jn) in joint_nodes.iter().enumerate() {
        if inverse_bind[ji] == [[0.0; 4]; 4] {
            let local = node_local_matrix(jn);
            inverse_bind[ji] = local.inverse().to_cols_array_2d();
        }
    }

    let mut mesh_data: Option<SkinnedMeshData> = None;
    for primitive in mesh.primitives() {
        if let Some(data) = read_skinned_gltf_primitive(&primitive, &buffers, &images) {
            mesh_data = Some(data);
            break;
        }
    }
    let mut mesh_data = mesh_data?;
    let mesh_normalize = if let Some(extent) = normalize_to_extent {
        let (min, max) = skinned_mesh_bounds(&mesh_data.vertices)?;
        let norm = center_scale_normalize_mat(min, max, extent);
        apply_normalize_to_skinned_vertices(&mut mesh_data.vertices, norm);
        norm
    } else {
        Mat4::IDENTITY
    };

    let mut clips = Vec::new();
    for anim in doc.animations() {
        if let Some(clip) = parse_gltf_animation(&anim, &buffers, &node_to_joint) {
            if !clip.channels.is_empty() {
                clips.push(clip);
            }
        }
    }
    if clips.is_empty() {
        return None;
    }

    let mut bind_local = Vec::with_capacity(joint_nodes.len());
    for jn in &joint_nodes {
        bind_local.push(node_local_matrix(jn));
    }

    Some(Arc::new(ModelAsset {
        path: path.display().to_string(),
        joint_parents,
        inverse_bind: inverse_bind.clone(),
        bind_local,
        joint_names,
        parts: vec![SkinnedMeshPart {
            name: node
                .name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "mesh".to_string()),
            mesh: mesh_data,
            mesh_bind_world: Mat4::IDENTITY,
            inverse_bind,
        }],
        clips,
        mesh_normalize,
        facing_forward_xz: glam::Vec2::ZERO,
    }))
}

fn find_gltf_skinned_node_index(node: gltf::Node, out: &mut Option<usize>) {
    if node.skin().is_some() && node.mesh().is_some() {
        *out = Some(node.index());
        return;
    }
    for child in node.children() {
        find_gltf_skinned_node_index(child, out);
        if out.is_some() {
            return;
        }
    }
}

fn build_gltf_node_parents(scene: gltf::Scene) -> HashMap<usize, usize> {
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

fn node_local_matrix(node: &gltf::Node) -> Mat4 {
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

fn read_skinned_gltf_primitive(
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
    images: &[gltf::image::Data],
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
    let joint_data: Vec<[u16; 4]> = reader
        .read_joints(0)?
        .into_u16()
        .collect();
    let weight_data: Vec<[f32; 4]> = reader.read_weights(0)?.into_f32().collect();
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

    let (rgba, width, height) = gltf_texture_rgba(primitive, images);

    Some(SkinnedMeshData {
        vertices,
        indices,
        rgba,
        width,
        height,
    })
}

fn gltf_texture_rgba(
    primitive: &gltf::Primitive,
    images: &[gltf::image::Data],
) -> (Vec<u8>, u32, u32) {
    if let Some(img_idx) = primitive
        .material()
        .pbr_metallic_roughness()
        .base_color_texture()
        .map(|info| info.texture().source().index())
    {
        if let Some(img_data) = images.get(img_idx) {
            use gltf::image::Format;
            let pixels = match img_data.format {
                Format::R8G8B8 => img_data
                    .pixels
                    .chunks_exact(3)
                    .flat_map(|p| [p[0], p[1], p[2], 255u8])
                    .collect(),
                Format::R8G8B8A8 => img_data.pixels.clone(),
                _ => vec![255, 255, 255, 255],
            };
            return (pixels, img_data.width, img_data.height);
        }
    }
    (vec![255, 255, 255, 255], 1, 1)
}

fn parse_gltf_animation(
    anim: &gltf::Animation,
    buffers: &[gltf::buffer::Data],
    node_to_joint: &HashMap<usize, usize>,
) -> Option<AnimationClip> {
    let name = anim
        .name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("Animation_{}", anim.index()));

    let mut channels = Vec::new();
    let mut max_time = 0.0f32;

    for channel in anim.channels() {
        let target_node = channel.target().node().index();
        let joint_index = match node_to_joint.get(&target_node) {
            Some(&ji) => ji,
            None => continue,
        };
        let property = match channel.target().property() {
            gltf::animation::Property::Translation => AnimProperty::Translation,
            gltf::animation::Property::Rotation => AnimProperty::Rotation,
            gltf::animation::Property::Scale => AnimProperty::Scale,
            _ => continue,
        };
        let reader = channel.reader(|b| Some(&buffers[b.index()].0[..]));
        let Some(inputs) = reader.read_inputs() else {
            continue;
        };
        let inputs: Vec<f32> = inputs.collect();
        let outputs: Vec<[f32; 4]> = match reader.read_outputs() {
            Some(gltf::animation::util::ReadOutputs::Translations(iter))
                if matches!(property, AnimProperty::Translation) =>
            {
                iter.map(|v| [v[0], v[1], v[2], 0.0]).collect()
            }
            Some(gltf::animation::util::ReadOutputs::Scales(iter))
                if matches!(property, AnimProperty::Scale) =>
            {
                iter.map(|v| [v[0], v[1], v[2], 0.0]).collect()
            }
            Some(gltf::animation::util::ReadOutputs::Rotations(
                gltf::animation::util::Rotations::F32(iter),
            )) if matches!(property, AnimProperty::Rotation) => {
                iter.map(|q| [q[0], q[1], q[2], q[3]]).collect()
            }
            _ => continue,
        };

        if inputs.is_empty() || outputs.is_empty() {
            continue;
        }

        let mut keyframes = Vec::new();
        for (t, out) in inputs.iter().zip(outputs.iter()) {
            max_time = max_time.max(*t);
            let kf = match property {
                AnimProperty::Translation => AnimKeyframe {
                    time: *t,
                    translation: Some([out[0], out[1], out[2]]),
                    rotation: None,
                    scale: None,
                },
                AnimProperty::Scale => AnimKeyframe {
                    time: *t,
                    translation: None,
                    rotation: None,
                    scale: Some([out[0], out[1], out[2]]),
                },
                AnimProperty::Rotation => AnimKeyframe {
                    time: *t,
                    translation: None,
                    rotation: Some([out[0], out[1], out[2], out[3]]),
                    scale: None,
                },
            };
            keyframes.push(kf);
        }
        keyframes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
        channels.push(AnimChannel {
            joint_index,
            property,
            keyframes,
        });
    }

    let duration_s = max_time.max(1.0 / 30.0);
    let fps = 30.0;

    Some(AnimationClip {
        name,
        duration_s,
        fps,
        channels,
    })
}

/// Misma regla que `mesh_3d::center_and_normalize_vertices`: v' = scale * (v - center).
fn center_scale_normalize_mat(min: [f32; 3], max: [f32; 3], target_extent: f32) -> Mat4 {
    let center = Vec3::new(
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    );
    let extent = (max[0] - min[0])
        .max(max[1] - min[1])
        .max(max[2] - min[2]);
    let scale = if extent > 1e-5 {
        (target_extent / extent).clamp(0.001, 50.0)
    } else {
        1.0
    };
    Mat4::from_scale(Vec3::splat(scale)) * Mat4::from_translation(-center)
}

fn apply_normalize_to_skinned_vertices(vertices: &mut [SkinnedVertex], norm: Mat4) {
    for v in vertices.iter_mut() {
        let p = norm.transform_point3(Vec3::from_array(v.position));
        v.position = p.to_array();
    }
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

fn skinned_mesh_bounds(vertices: &[SkinnedVertex]) -> Option<([f32; 3], [f32; 3])> {
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

// ── FBX (ufbx) ─────────────────────────────────────────────────────────────

fn load_fbx_asset(path: &Path, normalize_to_extent: Option<f32>) -> Option<Arc<ModelAsset>> {
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
                 Reexporta el modelo con las curvas de animación incluidas en el FBX/GLB \
                 (evita referencias solo a .mb u otros archivos externos).",
                path.display()
            );
        }
        return None;
    }

    let skinned_nodes = collect_fbx_skinned_nodes(&scene);
    if skinned_nodes.is_empty() {
        log::warn!(
            "[model_asset] FBX sin malla skinned (skin_deformers vacío): {}",
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
            "[model_asset] FBX tiene {} huesos (máx {MAX_JOINTS}): {}",
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
            mesh: mesh_data,
            mesh_bind_world: world,
            inverse_bind: part_ibm,
        });
    }
    if asset_parts.is_empty() {
        log::warn!("[model_asset] FBX sin geometría skinned legible: {}", path.display());
        return None;
    }

    let extent = normalize_to_extent.unwrap_or(1.8);
    let mesh_normalize = fbx_scene_world_bounds(&scene)
        .map(|(min, max)| center_scale_normalize_mat(min, max, extent))
        .unwrap_or(Mat4::IDENTITY);
    for part in asset_parts.iter_mut() {
        apply_normalize_to_skinned_vertices(&mut part.mesh.vertices, mesh_normalize);
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

    Some(Arc::new(ModelAsset {
        path: path.display().to_string(),
        joint_parents: skeleton.joint_parents,
        inverse_bind: skeleton.inverse_bind,
        bind_local: skeleton.bind_local,
        joint_names: skeleton.joint_names,
        parts: asset_parts,
        clips,
        mesh_normalize,
        facing_forward_xz: crate::config_3d::fbx_facing::forward_xz_from_ufbx_front(
            scene.settings.axes.front,
        ),
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

/// Incluye nodos padre (p. ej. Armature de Mixamo) que no tienen cluster pero afectan la jerarquía.
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
                        "[model_asset] ancestros FBX omitidos: límite {MAX_JOINTS} huesos"
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

// ── Orientación jugador FP (solo FBX) ────────────────────────────────────────

fn torso_vertex_indices(vertices: &[crate::mesh::Vertex]) -> Vec<usize> {
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

fn estimate_fbx_forward_from_normals(vertices: &[crate::mesh::Vertex]) -> glam::Vec2 {
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

fn estimate_forward_xz_from_positions(positions: &[[f32; 3]]) -> glam::Vec2 {
    if positions.is_empty() {
        return glam::Vec2::new(0.0, 1.0);
    }
    let mut pos_z = 0.0f32;
    let mut neg_z = 0.0f32;
    let mut pos_x = 0.0f32;
    let mut neg_x = 0.0f32;
    for p in positions {
        if p[2] > 0.0 {
            pos_z += p[2];
        } else {
            neg_z += -p[2];
        }
        if p[0] > 0.0 {
            pos_x += p[0];
        } else {
            neg_x += -p[0];
        }
    }
    let mut weights = [pos_z, neg_z, pos_x, neg_x];
    weights.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let primary = weights[0];
    let secondary = weights[1].max(1e-8);
    if primary < secondary * 1.08 {
        return glam::Vec2::new(0.0, 1.0);
    }
    if pos_z.max(neg_z) >= pos_x.max(neg_x) {
        if pos_z >= neg_z {
            glam::Vec2::new(0.0, 1.0)
        } else {
            glam::Vec2::new(0.0, -1.0)
        }
    } else if pos_x >= neg_x {
        glam::Vec2::new(1.0, 0.0)
    } else {
        glam::Vec2::new(-1.0, 0.0)
    }
}

fn estimate_fbx_forward_torso(vertices: &[crate::mesh::Vertex]) -> glam::Vec2 {
    if vertices.is_empty() {
        return glam::Vec2::new(0.0, 1.0);
    }
    let torso_ix = torso_vertex_indices(vertices);
    let positions: Vec<[f32; 3]> = if torso_ix.len() >= 64 {
        torso_ix.iter().map(|&i| vertices[i].position).collect()
    } else {
        vertices.iter().map(|v| v.position).collect()
    };
    estimate_forward_xz_from_positions(&positions)
}

fn find_hips_joint_index(asset: &ModelAsset) -> Option<usize> {
    asset
        .joint_names
        .iter()
        .position(|n| n.to_ascii_lowercase().contains("hips"))
}

fn skinned_forward_core_vertices(asset: &ModelAsset) -> Vec<crate::mesh::Vertex> {
    use crate::mesh::Vertex;
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

fn estimate_skinned_forward_from_vertices(asset: &ModelAsset) -> glam::Vec2 {
    let verts = skinned_forward_core_vertices(asset);
    if verts.is_empty() {
        return asset.facing_forward_xz;
    }
    let est_pos = crate::config_3d::mesh_3d::estimate_mesh_forward_xz(&verts);
    let est_norm = estimate_fbx_forward_from_normals(&verts);
    if asset.facing_forward_xz.dot(est_norm).abs() >= asset.facing_forward_xz.dot(est_pos).abs() {
        est_norm
    } else {
        est_pos
    }
}

fn estimate_skinned_forward_torso(asset: &ModelAsset) -> glam::Vec2 {
    let verts = skinned_forward_core_vertices(asset);
    if verts.is_empty() {
        return asset.facing_forward_xz;
    }
    estimate_fbx_forward_torso(&verts)
}

fn needs_skinned_forward_correction(asset: &ModelAsset) -> bool {
    let Some(hi) = find_hips_joint_index(asset) else {
        return false;
    };
    let (_, rot, _) = asset.bind_local[hi]
        .to_scale_rotation_translation();
    let (bone_yaw, _, _) = rot.to_euler(glam::EulerRot::YXZ);
    bone_yaw.abs() >= 0.35
}

/// Compensa yaw del hueso raíz (Mixamo suele traer ~±90° en bind).
fn apply_hips_bind_yaw_to_forward(fwd: glam::Vec2, asset: &ModelAsset) -> glam::Vec2 {
    let Some(hi) = find_hips_joint_index(asset) else {
        return fwd;
    };
    let (_, rot, _) = asset.bind_local[hi]
        .to_scale_rotation_translation();
    let (bone_yaw, _, _) = rot.to_euler(glam::EulerRot::YXZ);
    if bone_yaw.abs() < 0.35 {
        return fwd;
    }
    let (s, c) = (-bone_yaw).sin_cos();
    let fx = fwd.x;
    let fz = fwd.y;
    glam::Vec2::new(fx * c - fz * s, fx * s + fz * c).normalize_or_zero()
}

fn part_counts_for_play_forward(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n.is_empty() {
        return true;
    }
    const SKIP: &[&str] = &[
        "hair", "gun", "weapon", "boost", "cape", "wing", "shield", "backpack", "acc",
        "accessory", "prop",
    ];
    !SKIP.iter().any(|s| n.contains(s))
}

fn display_forward_xz(asset: &ModelAsset) -> glam::Vec2 {
    let fwd = estimate_skinned_forward_torso(asset);
    apply_hips_bind_yaw_to_forward(fwd, asset)
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
            mesh: mesh_data,
            mesh_bind_world: world,
            inverse_bind: Vec::new(),
        });
    }
    if parts.is_empty() {
        return None;
    }
    let mesh_normalize = fbx_scene_world_bounds(&scene)
        .map(|(min, max)| center_scale_normalize_mat(min, max, normalize_to_extent))
        .unwrap_or(Mat4::IDENTITY);
    for part in parts.iter_mut() {
        apply_normalize_to_skinned_vertices(&mut part.mesh.vertices, mesh_normalize);
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
    let est_torso = estimate_fbx_forward_torso(&verts);
    let est_norm = estimate_fbx_forward_from_normals(&verts);
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
            crate::config_3d::mesh_3d::estimate_mesh_forward_xz(&verts)
        }
    };
    let est_torso = estimate_skinned_forward_torso(asset);
    if est_pos.dot(est_torso) < -0.5 {
        return crate::config_3d::fbx_facing::resolve_fbx_forward_xz(meta, est_torso);
    }
    let est = estimate_skinned_forward_from_vertices(asset);
    crate::config_3d::fbx_facing::resolve_fbx_forward_xz(meta, est)
}

pub fn clip_infos(asset: &ModelAsset) -> Vec<ModelClipInfo> {
    asset
        .clips
        .iter()
        .map(|c| ModelClipInfo {
            name: c.name.clone(),
            duration_s: c.duration_s,
            fps: c.fps,
        })
        .collect()
}
