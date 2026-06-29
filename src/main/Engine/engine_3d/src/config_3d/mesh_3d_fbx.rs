//! Carga de mallas estáticas FBX (ufbx) para editor y precarga CPU.

use std::path::Path;
use std::sync::Arc;

use crate::config_3d::model_asset::MaterialTextureCpu;
use super::mesh_3d::{
    estimate_mesh_forward_xz, normalize_vertices_height_feet_pivot, vertex_local_bounds,
    CpuModelMeshPart, LoadedModelMesh,
};
use crate::mesh::{upload, Vertex};

fn white_pixel() -> (Vec<u8>, u32, u32) {
    (vec![255, 255, 255, 255], 1, 1)
}

fn decode_image_bytes(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Some((rgba.into_raw(), w, h))
}

fn load_image_path(path: &Path) -> (Vec<u8>, u32, u32) {
    match image::open(path) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            (rgba.into_raw(), w, h)
        }
        Err(_) => white_pixel(),
    }
}

fn ufbx_matrix_to_mat4(m: &ufbx::Matrix) -> glam::Mat4 {
    glam::Mat4::from_cols(
        glam::Vec4::new(m.m00 as f32, m.m10 as f32, m.m20 as f32, 0.0),
        glam::Vec4::new(m.m01 as f32, m.m11 as f32, m.m21 as f32, 0.0),
        glam::Vec4::new(m.m02 as f32, m.m12 as f32, m.m22 as f32, 0.0),
        glam::Vec4::new(m.m03 as f32, m.m13 as f32, m.m23 as f32, 1.0),
    )
}

fn push_fbx_mesh_triangles(
    mesh: &ufbx::Mesh,
    world: glam::Mat4,
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
) {
    use glam::{Mat3, Vec3};

    let mut tri_indices = vec![0u32; mesh.max_face_triangles * 3];
    let world3 = Mat3::from_mat4(world);

    let face_iter: Box<dyn Iterator<Item = u32>> = if !mesh.material_parts.is_empty() {
        Box::new(mesh.material_parts.iter().flat_map(|part| {
            part.face_indices.iter().copied()
        }))
    } else {
        Box::new((0..mesh.faces.len() as u32).collect::<Vec<_>>().into_iter())
    };

    for face_index in face_iter {
        let face = mesh.faces[face_index as usize];
        let num_tris = mesh.triangulate_face(&mut tri_indices, face);
        let corner_count = num_tris as usize * 3;
        for &index in &tri_indices[..corner_count] {
            let ix = index as usize;
            let pos = ufbx::get_vertex_vec3(&mesh.vertex_position, ix);
            let norm = ufbx::get_vertex_vec3(&mesh.vertex_normal, ix);
            let uv = ufbx::get_vertex_vec2(&mesh.vertex_uv, ix);
            let wp = world.transform_point3(Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32));
            let wn = (world3 * Vec3::new(norm.x as f32, norm.y as f32, norm.z as f32))
                .normalize_or_zero();
            vertices.push(Vertex {
                position: wp.to_array(),
                normal: wn.to_array(),
                uv: [uv.x as f32, uv.y as f32],
            });
            indices.push(vertices.len() as u32 - 1);
        }
    }
}

pub(crate) fn load_fbx(
    device: &wgpu::Device,
    path: &Path,
    normalize_to_extent: Option<f32>,
) -> Result<Vec<LoadedModelMesh>, String> {
    let mut opts = ufbx::LoadOpts::default();
    opts.generate_missing_normals = true;
    opts.load_external_files = true;
    opts.target_axes = ufbx::CoordinateAxes::right_handed_y_up();
    opts.target_unit_meters = 1.0;
    opts.space_conversion = ufbx::SpaceConversion::ModifyGeometry;

    let scene = ufbx::load_file(
        path.to_str()
            .ok_or_else(|| "ruta FBX no válida (UTF-8)".to_string())?,
        opts,
    )
    .map_err(|e| format!("fbx error: {e:?}"))?;

    let fbx_dir = path.parent().unwrap_or_else(|| Path::new("."));

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut rgba = white_pixel().0;
    let mut tex_w = 1u32;
    let mut tex_h = 1u32;
    let mut texture_picked = false;

    for node in scene.nodes.iter() {
        let Some(mesh_ref) = node.mesh.as_ref() else {
            continue;
        };
        let mesh: &ufbx::Mesh = mesh_ref.as_ref();

        if !texture_picked {
            if let Some(mat) = mesh.materials.first().map(|m| m.as_ref()) {
                if let Some(tex) = texture_from_material(mat) {
                    let (r, w, h) = texture_rgba(tex, fbx_dir);
                    rgba = r;
                    tex_w = w;
                    tex_h = h;
                    texture_picked = true;
                }
            }
        }

        let world = ufbx_matrix_to_mat4(&node.geometry_to_world);
        push_fbx_mesh_triangles(mesh, world, &mut vertices, &mut indices);
    }

    if vertices.is_empty() {
        return Err("el archivo FBX no contiene mallas".into());
    }

    normalize_vertices_height_feet_pivot(&mut vertices, normalize_to_extent.unwrap_or(1.8));
    let local_bounds = vertex_local_bounds(&vertices);
    let meta =
        crate::config_3d::fbx_facing::forward_xz_from_ufbx_front(scene.settings.axes.front);
    let est = estimate_mesh_forward_xz(&vertices);
    let forward_xz = crate::config_3d::fbx_facing::resolve_fbx_forward_xz(meta, est);

    Ok(vec![LoadedModelMesh {
        mesh: upload(device, &vertices, &indices, "fbx-mesh", None),
        rgba,
        width: tex_w,
        height: tex_h,
        forward_xz,
        local_bounds,
    }])
}

pub(crate) fn load_fbx_cpu(
    path: &Path,
    normalize_to_extent: Option<f32>,
) -> Result<Vec<CpuModelMeshPart>, String> {
    let mut opts = ufbx::LoadOpts::default();
    opts.generate_missing_normals = true;
    opts.load_external_files = true;
    opts.target_axes = ufbx::CoordinateAxes::right_handed_y_up();
    opts.target_unit_meters = 1.0;
    opts.space_conversion = ufbx::SpaceConversion::ModifyGeometry;

    let scene = ufbx::load_file(
        path.to_str()
            .ok_or_else(|| "ruta FBX no válida (UTF-8)".to_string())?,
        opts,
    )
    .map_err(|e| format!("fbx error: {e:?}"))?;

    let fbx_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut rgba = white_pixel().0;
    let mut tex_w = 1u32;
    let mut tex_h = 1u32;
    let mut texture_picked = false;
    let mut roughness = 0.5;
    let mut metallic = 0.0;

    for node in scene.nodes.iter() {
        let Some(mesh_ref) = node.mesh.as_ref() else {
            continue;
        };
        let mesh: &ufbx::Mesh = mesh_ref.as_ref();

        if let Some(mat) = mesh.materials.first().map(|m| m.as_ref()) {
            if !texture_picked {
                if let Some(tex) = texture_from_material(mat) {
                    let (r, w, h) = texture_rgba(tex, fbx_dir);
                    rgba = r;
                    tex_w = w;
                    tex_h = h;
                    texture_picked = true;
                }
            }
            roughness = mat.pbr.roughness.value_vec4.x as f32;
            metallic = mat.pbr.metalness.value_vec4.x as f32;
        }

        let world = ufbx_matrix_to_mat4(&node.geometry_to_world);
        push_fbx_mesh_triangles(mesh, world, &mut vertices, &mut indices);
    }

    if vertices.is_empty() {
        return Err("el archivo FBX no contiene mallas".into());
    }

    normalize_vertices_height_feet_pivot(&mut vertices, normalize_to_extent.unwrap_or(1.8));

    let ior = if metallic > 0.5 { 0.0 } else { 1.5 };

    Ok(vec![CpuModelMeshPart {
        forward_xz: estimate_mesh_forward_xz(&vertices),
        local_bounds: vertex_local_bounds(&vertices),
        vertices,
        indices,
        material_index: 0,
        texture: Arc::new(MaterialTextureCpu {
            rgba: Arc::from(rgba.into_boxed_slice()),
            width: tex_w,
            height: tex_h,
            layer_mips: None,
        }),
        roughness,
        metallic,
        ior,
    }])
}

fn texture_from_material<'a>(material: &'a ufbx::Material) -> Option<&'a ufbx::Texture> {
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

fn texture_rgba(texture: &ufbx::Texture, fbx_dir: &Path) -> (Vec<u8>, u32, u32) {
    if !texture.content.is_empty() {
        if let Some(decoded) = decode_image_bytes(&texture.content) {
            return decoded;
        }
    }

    for name in [
        texture.absolute_filename.as_ref(),
        texture.filename.as_ref(),
        texture.relative_filename.as_ref(),
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
            return load_image_path(&resolved);
        }
    }

    white_pixel()
}
