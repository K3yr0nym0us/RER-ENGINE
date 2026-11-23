//! Gizmos de sockets (pose animada) en editor.

use glam::Vec3;

use crate::engine3d::config_3d::entity_sockets::{bone_world_transform, socket_world_transform};
use crate::engine3d::ecs::EntityId;
use crate::engine3d::engine::State;
use crate::engine3d::gizmo::{self, GizmoVertex};

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

fn push_socket_axes(verts: &mut Vec<GizmoVertex>, p: Vec3, rotation: glam::Quat) {
    let s = 0.08_f32;
    let x = rotation * Vec3::X * s;
    let y = rotation * Vec3::Y * s;
    let z = rotation * Vec3::Z * s;
    push_line(verts, p, p + x, [1.0, 0.2, 0.2, 1.0]);
    push_line(verts, p, p + y, [0.2, 1.0, 0.2, 1.0]);
    push_line(verts, p, p + z, [0.2, 0.4, 1.0, 1.0]);
}

pub(crate) fn entity_socket_overlay_verts(state: &State, entity_id: EntityId) -> Vec<GizmoVertex> {
    let mut verts = Vec::new();
    if let Some(sockets) = state.entity_sockets.get(&entity_id) {
        for socket in sockets {
            let Some(bone) = bone_world_transform(state, entity_id, &socket.bone_name) else {
                continue;
            };
            let world = socket_world_transform(&bone, socket);
            push_socket_axes(&mut verts, world.position, world.rotation);
        }
    }
    verts
}

pub(crate) fn selected_socket_overlay_key(state: &State) -> u64 {
    let mut hash = gizmo::overlay_hash();
    let Some(id) = state.selected_entity else {
        return hash;
    };
    gizmo::mix_u64(&mut hash, u64::from(id));
    if let Some(t) = state.world.get::<crate::engine3d::ecs::Transform>(id) {
        gizmo::mix_vec3(&mut hash, t.position.to_array());
        gizmo::mix_quat(&mut hash, t.rotation.to_array());
        gizmo::mix_vec3(&mut hash, t.scale.to_array());
    }
    if let Some(clip) = state.active_model_clips.get(&id) {
        gizmo::mix_u64(&mut hash, clip.playing as u64);
        gizmo::mix_u64(&mut hash, clip.finished as u64);
        gizmo::mix_f32(&mut hash, clip.time_s);
        for byte in clip.clip_name.as_bytes() {
            gizmo::mix_u64(&mut hash, u64::from(*byte));
        }
    }
    if let Some(sockets) = state.entity_sockets.get(&id) {
        gizmo::mix_u64(&mut hash, sockets.len() as u64);
        for socket in sockets {
            for byte in socket.name.as_bytes() {
                gizmo::mix_u64(&mut hash, u64::from(*byte));
            }
            for byte in socket.bone_name.as_bytes() {
                gizmo::mix_u64(&mut hash, u64::from(*byte));
            }
            gizmo::mix_vec3(&mut hash, socket.local_position.to_array());
            gizmo::mix_quat(&mut hash, socket.local_rotation.to_array());
        }
    }
    hash
}

pub(crate) fn selected_socket_overlay_verts(state: &State) -> Vec<GizmoVertex> {
    let id = match state.selected_entity {
        Some(id) => id,
        None => return Vec::new(),
    };
    entity_socket_overlay_verts(state, id)
}
