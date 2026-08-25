//! Serialización binaria de `ModelAsset` (skeleton + clips) para chunks `.rerasset`.

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

use glam::{Mat4, Vec2};

use crate::config_3d::model_asset::{
    AnimChannel, AnimKeyframe, AnimProperty, AnimationClip, ModelAsset, SkinnedMeshPart,
};

const SKEL_MAGIC: &[u8; 4] = b"SKEL";
const SKEL_VERSION: u16 = 2;
const SKEL_VERSION_MIN: u16 = 1;
const ANIM_MAGIC: &[u8; 4] = b"ANIM";
const ANIM_VERSION: u16 = 1;

fn write_string(w: &mut impl Write, s: &str) -> std::io::Result<()> {
    let bytes = s.as_bytes();
    let len = u16::try_from(bytes.len()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "string demasiado largo")
    })?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(bytes)
}

fn read_string(r: &mut impl Read) -> std::io::Result<String> {
    let mut len_buf = [0u8; 2];
    r.read_exact(&mut len_buf)?;
    let len = u16::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

fn write_mat4(w: &mut impl Write, m: &Mat4) -> std::io::Result<()> {
    for f in m.to_cols_array() {
        w.write_all(&f.to_le_bytes())?;
    }
    Ok(())
}

fn read_mat4(r: &mut impl Read) -> std::io::Result<Mat4> {
    let mut arr = [0f32; 16];
    for f in &mut arr {
        let mut buf = [0u8; 4];
        r.read_exact(&mut buf)?;
        *f = f32::from_le_bytes(buf);
    }
    Ok(Mat4::from_cols_array(&arr))
}

fn write_option_usize_vec(w: &mut impl Write, values: &[Option<usize>]) -> std::io::Result<()> {
    let count = u16::try_from(values.len())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "demasiados joints"))?;
    w.write_all(&count.to_le_bytes())?;
    for v in values {
        let raw = v.map(|x| x as u32).unwrap_or(u32::MAX);
        w.write_all(&raw.to_le_bytes())?;
    }
    Ok(())
}

fn read_option_usize_vec(r: &mut impl Read) -> std::io::Result<Vec<Option<usize>>> {
    let mut count_buf = [0u8; 2];
    r.read_exact(&mut count_buf)?;
    let count = u16::from_le_bytes(count_buf) as usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut buf = [0u8; 4];
        r.read_exact(&mut buf)?;
        let raw = u32::from_le_bytes(buf);
        out.push(if raw == u32::MAX {
            None
        } else {
            Some(raw as usize)
        });
    }
    Ok(out)
}

fn write_mat4_vec(w: &mut impl Write, mats: &[Mat4]) -> std::io::Result<()> {
    let count = u16::try_from(mats.len()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "demasiadas matrices")
    })?;
    w.write_all(&count.to_le_bytes())?;
    for m in mats {
        write_mat4(w, m)?;
    }
    Ok(())
}

fn read_mat4_vec(r: &mut impl Read) -> std::io::Result<Vec<Mat4>> {
    let mut count_buf = [0u8; 2];
    r.read_exact(&mut count_buf)?;
    let count = u16::from_le_bytes(count_buf) as usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(read_mat4(r)?);
    }
    Ok(out)
}

fn write_ibm_vec(w: &mut impl Write, ibms: &[[[f32; 4]; 4]]) -> std::io::Result<()> {
    let count = u16::try_from(ibms.len())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "demasiados IBM"))?;
    w.write_all(&count.to_le_bytes())?;
    for m in ibms {
        for row in m {
            for f in row {
                w.write_all(&f.to_le_bytes())?;
            }
        }
    }
    Ok(())
}

fn read_ibm_vec(r: &mut impl Read) -> std::io::Result<Vec<[[f32; 4]; 4]>> {
    let mut count_buf = [0u8; 2];
    r.read_exact(&mut count_buf)?;
    let count = u16::from_le_bytes(count_buf) as usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut m = [[0f32; 4]; 4];
        for row in &mut m {
            for f in row {
                let mut buf = [0u8; 4];
                r.read_exact(&mut buf)?;
                *f = f32::from_le_bytes(buf);
            }
        }
        out.push(m);
    }
    Ok(out)
}

fn property_to_u8(p: &AnimProperty) -> u8 {
    match p {
        AnimProperty::Translation => 0,
        AnimProperty::Rotation => 1,
        AnimProperty::Scale => 2,
    }
}

fn property_from_u8(v: u8) -> Result<AnimProperty, String> {
    match v {
        0 => Ok(AnimProperty::Translation),
        1 => Ok(AnimProperty::Rotation),
        2 => Ok(AnimProperty::Scale),
        other => Err(format!("AnimProperty desconocido: {other}")),
    }
}

pub fn serialize_skeleton(asset: &ModelAsset) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.write_all(SKEL_MAGIC).unwrap();
    buf.write_all(&SKEL_VERSION.to_le_bytes()).unwrap();
    write_string(&mut buf, &asset.path).unwrap();
    write_option_usize_vec(&mut buf, &asset.joint_parents).unwrap();
    let joint_count = u16::try_from(asset.joint_names.len()).unwrap();
    buf.write_all(&joint_count.to_le_bytes()).unwrap();
    for name in &asset.joint_names {
        write_string(&mut buf, name).unwrap();
    }
    write_mat4_vec(&mut buf, &asset.bind_local).unwrap();
    write_mat4_vec(&mut buf, &asset.joint_prefix_world).unwrap();
    write_ibm_vec(&mut buf, &asset.inverse_bind).unwrap();
    write_mat4(&mut buf, &asset.mesh_normalize).unwrap();
    buf.write_all(&asset.facing_forward_xz.x.to_le_bytes())
        .unwrap();
    buf.write_all(&asset.facing_forward_xz.y.to_le_bytes())
        .unwrap();
    buf.write_all(&[u8::from(asset.bind_pose_from_ibm)])
        .unwrap();

    let node_count = u16::try_from(asset.joint_gltf_nodes.len()).unwrap();
    buf.write_all(&node_count.to_le_bytes()).unwrap();
    for n in &asset.joint_gltf_nodes {
        buf.write_all(&(*n as u32).to_le_bytes()).unwrap();
    }

    let parent_count = u16::try_from(asset.gltf_scene_parents.len()).unwrap();
    buf.write_all(&parent_count.to_le_bytes()).unwrap();
    for (node, parent) in &asset.gltf_scene_parents {
        buf.write_all(&(*node as u32).to_le_bytes()).unwrap();
        buf.write_all(&(*parent as u32).to_le_bytes()).unwrap();
    }

    let bind_local_count = u16::try_from(asset.gltf_bind_node_local.len()).unwrap();
    buf.write_all(&bind_local_count.to_le_bytes()).unwrap();
    for (node, mat) in &asset.gltf_bind_node_local {
        buf.write_all(&(*node as u32).to_le_bytes()).unwrap();
        write_mat4(&mut buf, mat).unwrap();
    }

    let part_count = u16::try_from(asset.parts.len()).unwrap();
    buf.write_all(&part_count.to_le_bytes()).unwrap();
    for part in &asset.parts {
        buf.write_all(&part.material_index.to_le_bytes()).unwrap();
        write_string(&mut buf, &part.name).unwrap();
        write_mat4(&mut buf, &part.mesh_bind_world).unwrap();
        write_ibm_vec(&mut buf, &part.inverse_bind).unwrap();
    }
    buf
}

pub fn deserialize_skeleton(
    bytes: &[u8],
    mut skinned_parts: Vec<SkinnedMeshPart>,
) -> Result<ModelAsset, String> {
    let mut r = Cursor::new(bytes);
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic).map_err(|e| e.to_string())?;
    if &magic != SKEL_MAGIC {
        return Err("magic SKEL inválido".into());
    }
    let mut ver = [0u8; 2];
    r.read_exact(&mut ver).map_err(|e| e.to_string())?;
    if u16::from_le_bytes(ver) < SKEL_VERSION_MIN || u16::from_le_bytes(ver) > SKEL_VERSION {
        return Err("versión SKEL no soportada".into());
    }
    let skel_version = u16::from_le_bytes(ver);

    let path = read_string(&mut r).map_err(|e| e.to_string())?;
    let joint_parents = read_option_usize_vec(&mut r).map_err(|e| e.to_string())?;
    let mut count_buf = [0u8; 2];
    r.read_exact(&mut count_buf).map_err(|e| e.to_string())?;
    let joint_count = u16::from_le_bytes(count_buf) as usize;
    let mut joint_names = Vec::with_capacity(joint_count);
    for _ in 0..joint_count {
        joint_names.push(read_string(&mut r).map_err(|e| e.to_string())?);
    }
    let bind_local = read_mat4_vec(&mut r).map_err(|e| e.to_string())?;
    let joint_prefix_world = read_mat4_vec(&mut r).map_err(|e| e.to_string())?;
    let inverse_bind = read_ibm_vec(&mut r).map_err(|e| e.to_string())?;
    let mesh_normalize = read_mat4(&mut r).map_err(|e| e.to_string())?;
    let mut xz = [0u8; 8];
    r.read_exact(&mut xz).map_err(|e| e.to_string())?;
    let facing_forward_xz = Vec2::new(
        f32::from_le_bytes(xz[0..4].try_into().unwrap()),
        f32::from_le_bytes(xz[4..8].try_into().unwrap()),
    );

    let bind_pose_from_ibm = if skel_version >= 2 {
        let mut flag = [0u8; 1];
        r.read_exact(&mut flag).map_err(|e| e.to_string())?;
        flag[0] != 0
    } else {
        false
    };

    r.read_exact(&mut count_buf).map_err(|e| e.to_string())?;
    let node_count = u16::from_le_bytes(count_buf) as usize;
    let mut joint_gltf_nodes = Vec::with_capacity(node_count);
    for _ in 0..node_count {
        let mut buf = [0u8; 4];
        r.read_exact(&mut buf).map_err(|e| e.to_string())?;
        joint_gltf_nodes.push(u32::from_le_bytes(buf) as usize);
    }

    r.read_exact(&mut count_buf).map_err(|e| e.to_string())?;
    let parent_count = u16::from_le_bytes(count_buf) as usize;
    let mut gltf_scene_parents = HashMap::new();
    for _ in 0..parent_count {
        let mut buf = [0u8; 4];
        r.read_exact(&mut buf).map_err(|e| e.to_string())?;
        let node = u32::from_le_bytes(buf) as usize;
        r.read_exact(&mut buf).map_err(|e| e.to_string())?;
        let parent = u32::from_le_bytes(buf) as usize;
        gltf_scene_parents.insert(node, parent);
    }

    r.read_exact(&mut count_buf).map_err(|e| e.to_string())?;
    let bind_local_count = u16::from_le_bytes(count_buf) as usize;
    let mut gltf_bind_node_local = HashMap::new();
    for _ in 0..bind_local_count {
        let mut buf = [0u8; 4];
        r.read_exact(&mut buf).map_err(|e| e.to_string())?;
        let node = u32::from_le_bytes(buf) as usize;
        let mat = read_mat4(&mut r).map_err(|e| e.to_string())?;
        gltf_bind_node_local.insert(node, mat);
    }

    r.read_exact(&mut count_buf).map_err(|e| e.to_string())?;
    let part_meta_count = u16::from_le_bytes(count_buf) as usize;
    let mut parts = Vec::with_capacity(part_meta_count);
    for i in 0..part_meta_count {
        let mut buf = [0u8; 4];
        r.read_exact(&mut buf).map_err(|e| e.to_string())?;
        let material_index = u32::from_le_bytes(buf);
        let name = read_string(&mut r).map_err(|e| e.to_string())?;
        let mesh_bind_world = read_mat4(&mut r).map_err(|e| e.to_string())?;
        let inverse_bind = read_ibm_vec(&mut r).map_err(|e| e.to_string())?;
        let mesh = if i < skinned_parts.len() {
            std::mem::replace(
                &mut skinned_parts[i].mesh,
                crate::config_3d::model_asset::SkinnedMeshData {
                    vertices: vec![],
                    indices: vec![],
                    rgba: vec![],
                    width: 1,
                    height: 1,
                },
            )
        } else {
            crate::config_3d::model_asset::SkinnedMeshData {
                vertices: vec![],
                indices: vec![],
                rgba: vec![255, 255, 255, 255],
                width: 1,
                height: 1,
            }
        };
        parts.push(SkinnedMeshPart {
            name,
            material_index,
            mesh,
            mesh_bind_world,
            inverse_bind,
        });
    }

    Ok(ModelAsset {
        path,
        joint_parents,
        inverse_bind,
        bind_local,
        joint_prefix_world,
        joint_names,
        parts,
        clips: vec![],
        mesh_normalize,
        facing_forward_xz,
        joint_gltf_nodes,
        gltf_scene_parents,
        gltf_bind_node_local,
        bind_pose_from_ibm,
    })
}

pub fn serialize_animation_clip(clip: &AnimationClip) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.write_all(ANIM_MAGIC).unwrap();
    buf.write_all(&ANIM_VERSION.to_le_bytes()).unwrap();
    write_string(&mut buf, &clip.name).unwrap();
    buf.write_all(&clip.duration_s.to_le_bytes()).unwrap();
    buf.write_all(&clip.fps.to_le_bytes()).unwrap();
    let ch_count = u16::try_from(clip.channels.len()).unwrap();
    buf.write_all(&ch_count.to_le_bytes()).unwrap();
    for ch in &clip.channels {
        buf.write_all(&(ch.joint_index as u16).to_le_bytes())
            .unwrap();
        buf.write_all(&[property_to_u8(&ch.property)]).unwrap();
        let kf_count = u16::try_from(ch.keyframes.len()).unwrap();
        buf.write_all(&kf_count.to_le_bytes()).unwrap();
        for kf in &ch.keyframes {
            buf.write_all(&kf.time.to_le_bytes()).unwrap();
            buf.write_all(&[kf.translation.is_some() as u8]).unwrap();
            if let Some(t) = kf.translation {
                for f in t {
                    buf.write_all(&f.to_le_bytes()).unwrap();
                }
            }
            buf.write_all(&[kf.rotation.is_some() as u8]).unwrap();
            if let Some(q) = kf.rotation {
                for f in q {
                    buf.write_all(&f.to_le_bytes()).unwrap();
                }
            }
            buf.write_all(&[kf.scale.is_some() as u8]).unwrap();
            if let Some(s) = kf.scale {
                for f in s {
                    buf.write_all(&f.to_le_bytes()).unwrap();
                }
            }
        }
    }
    buf
}

pub fn deserialize_animation_clip(bytes: &[u8]) -> Result<AnimationClip, String> {
    let mut r = Cursor::new(bytes);
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic).map_err(|e| e.to_string())?;
    if &magic != ANIM_MAGIC {
        return Err("magic ANIM inválido".into());
    }
    let mut ver = [0u8; 2];
    r.read_exact(&mut ver).map_err(|e| e.to_string())?;
    if u16::from_le_bytes(ver) != ANIM_VERSION {
        return Err("versión ANIM no soportada".into());
    }
    let name = read_string(&mut r).map_err(|e| e.to_string())?;
    let mut fbuf = [0u8; 4];
    r.read_exact(&mut fbuf).map_err(|e| e.to_string())?;
    let duration_s = f32::from_le_bytes(fbuf);
    r.read_exact(&mut fbuf).map_err(|e| e.to_string())?;
    let fps = f32::from_le_bytes(fbuf);
    let mut count_buf = [0u8; 2];
    r.read_exact(&mut count_buf).map_err(|e| e.to_string())?;
    let ch_count = u16::from_le_bytes(count_buf) as usize;
    let mut channels = Vec::with_capacity(ch_count);
    for _ in 0..ch_count {
        r.read_exact(&mut count_buf).map_err(|e| e.to_string())?;
        let joint_index = u16::from_le_bytes(count_buf) as usize;
        let mut prop_buf = [0u8; 1];
        r.read_exact(&mut prop_buf).map_err(|e| e.to_string())?;
        let property = property_from_u8(prop_buf[0])?;
        r.read_exact(&mut count_buf).map_err(|e| e.to_string())?;
        let kf_count = u16::from_le_bytes(count_buf) as usize;
        let mut keyframes = Vec::with_capacity(kf_count);
        for _ in 0..kf_count {
            r.read_exact(&mut fbuf).map_err(|e| e.to_string())?;
            let time = f32::from_le_bytes(fbuf);
            let mut flag = [0u8; 1];
            r.read_exact(&mut flag).map_err(|e| e.to_string())?;
            let translation = if flag[0] != 0 {
                let mut t = [0f32; 3];
                for f in &mut t {
                    r.read_exact(&mut fbuf).map_err(|e| e.to_string())?;
                    *f = f32::from_le_bytes(fbuf);
                }
                Some(t)
            } else {
                None
            };
            r.read_exact(&mut flag).map_err(|e| e.to_string())?;
            let rotation = if flag[0] != 0 {
                let mut q = [0f32; 4];
                for f in &mut q {
                    r.read_exact(&mut fbuf).map_err(|e| e.to_string())?;
                    *f = f32::from_le_bytes(fbuf);
                }
                Some(q)
            } else {
                None
            };
            r.read_exact(&mut flag).map_err(|e| e.to_string())?;
            let scale = if flag[0] != 0 {
                let mut s = [0f32; 3];
                for f in &mut s {
                    r.read_exact(&mut fbuf).map_err(|e| e.to_string())?;
                    *f = f32::from_le_bytes(fbuf);
                }
                Some(s)
            } else {
                None
            };
            keyframes.push(AnimKeyframe {
                time,
                translation,
                rotation,
                scale,
            });
        }
        channels.push(AnimChannel {
            joint_index,
            property,
            keyframes,
        });
    }
    Ok(AnimationClip {
        name,
        duration_s,
        fps,
        channels,
    })
}
