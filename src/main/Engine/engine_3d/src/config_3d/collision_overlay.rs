use glam::Vec3 as GlamVec3;

use crate::ecs::Transform;
use crate::engine::State;
use crate::entity_save_meta::entity_category_uses_character_capsule;
use crate::gizmo::{self, GizmoVertex};

const COLOR_RAPIER: [f32; 4] = [0.2, 0.9, 1.0, 1.0];
const COLOR_CHARACTER_CAPSULE: [f32; 4] = [1.0, 0.55, 0.1, 1.0];
const COLOR_PLAYER_PLACEHOLDER: [f32; 4] = [1.0, 0.92, 0.25, 1.0];
const COLOR_PLAYER_RAPIER_BUG: [f32; 4] = [1.0, 0.15, 0.15, 1.0];

const CAPSULE_WIRE_SEGMENTS: usize = 24;
const CAPSULE_HEMI_ARC_STEPS: usize = 10;

fn push_wire_box(verts: &mut Vec<GizmoVertex>, center: GlamVec3, half: GlamVec3, color: [f32; 4]) {
    let hx = half.x.max(0.01);
    let hy = half.y.max(0.01);
    let hz = half.z.max(0.01);
    let cx = center.x;
    let cy = center.y;
    let cz = center.z;

    let c000 = [cx - hx, cy - hy, cz - hz];
    let c001 = [cx - hx, cy - hy, cz + hz];
    let c010 = [cx - hx, cy + hy, cz - hz];
    let c011 = [cx - hx, cy + hy, cz + hz];
    let c100 = [cx + hx, cy - hy, cz - hz];
    let c101 = [cx + hx, cy - hy, cz + hz];
    let c110 = [cx + hx, cy + hy, cz - hz];
    let c111 = [cx + hx, cy + hy, cz + hz];

    let mut line = |a: [f32; 3], b: [f32; 3]| {
        verts.push(GizmoVertex { position: a, color });
        verts.push(GizmoVertex { position: b, color });
    };

    line(c000, c100);
    line(c001, c101);
    line(c010, c110);
    line(c011, c111);
    line(c000, c010);
    line(c001, c011);
    line(c100, c110);
    line(c101, c111);
    line(c000, c001);
    line(c010, c011);
    line(c100, c101);
    line(c110, c111);
}

fn push_line(verts: &mut Vec<GizmoVertex>, a: GlamVec3, b: GlamVec3, color: [f32; 4]) {
    verts.push(GizmoVertex {
        position: a.to_array(),
        color,
    });
    verts.push(GizmoVertex {
        position: b.to_array(),
        color,
    });
}

fn push_horizontal_ring(
    verts: &mut Vec<GizmoVertex>,
    center_x: f32,
    y: f32,
    center_z: f32,
    ring_radius: f32,
    color: [f32; 4],
) {
    let rr = ring_radius.max(0.02);
    for i in 0..CAPSULE_WIRE_SEGMENTS {
        let a0 = (i as f32 / CAPSULE_WIRE_SEGMENTS as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / CAPSULE_WIRE_SEGMENTS as f32) * std::f32::consts::TAU;
        let p0 = GlamVec3::new(center_x + a0.cos() * rr, y, center_z + a0.sin() * rr);
        let p1 = GlamVec3::new(center_x + a1.cos() * rr, y, center_z + a1.sin() * rr);
        push_line(verts, p0, p1, color);
    }
}

fn push_bottom_hemisphere_meridian(
    verts: &mut Vec<GizmoVertex>,
    tip: GlamVec3,
    center: GlamVec3,
    radius: f32,
    azimuth: f32,
    color: [f32; 4],
) {
    let ca = azimuth.cos();
    let sa = azimuth.sin();
    let mut prev = tip;
    for s in 1..=CAPSULE_HEMI_ARC_STEPS {
        let theta = (s as f32 / CAPSULE_HEMI_ARC_STEPS as f32) * std::f32::consts::FRAC_PI_2;
        let sin_t = theta.sin();
        let cos_t = theta.cos();
        let p = GlamVec3::new(
            center.x + radius * ca * sin_t,
            center.y - radius * cos_t,
            center.z + radius * sa * sin_t,
        );
        push_line(verts, prev, p, color);
        prev = p;
    }
}

fn push_top_hemisphere_meridian(
    verts: &mut Vec<GizmoVertex>,
    equator: GlamVec3,
    center: GlamVec3,
    radius: f32,
    azimuth: f32,
    color: [f32; 4],
) {
    let ca = azimuth.cos();
    let sa = azimuth.sin();
    let mut prev = equator;
    for s in 1..=CAPSULE_HEMI_ARC_STEPS {
        let phi = (s as f32 / CAPSULE_HEMI_ARC_STEPS as f32) * std::f32::consts::FRAC_PI_2;
        let cos_p = phi.cos();
        let sin_p = phi.sin();
        let p = GlamVec3::new(
            center.x + radius * ca * cos_p,
            center.y + radius * sin_p,
            center.z + radius * sa * cos_p,
        );
        push_line(verts, prev, p, color);
        prev = p;
    }
}

fn push_bottom_hemisphere_latitudes(
    verts: &mut Vec<GizmoVertex>,
    center: GlamVec3,
    radius: f32,
    color: [f32; 4],
) {
    for ring in 1..=3 {
        let theta = (ring as f32 / 4.0) * std::f32::consts::FRAC_PI_2;
        let y = center.y - radius * theta.cos();
        let rr = radius * theta.sin();
        push_horizontal_ring(verts, center.x, y, center.z, rr, color);
    }
}

fn push_top_hemisphere_latitudes(
    verts: &mut Vec<GizmoVertex>,
    center: GlamVec3,
    radius: f32,
    color: [f32; 4],
) {
    for ring in 1..=3 {
        let phi = (ring as f32 / 4.0) * std::f32::consts::FRAC_PI_2;
        let y = center.y + radius * phi.sin();
        let rr = radius * phi.cos();
        push_horizontal_ring(verts, center.x, y, center.z, rr, color);
    }
}

/// Píldora simétrica: mismo radio arriba/abajo, lados rectos, 1.7 m de altura total.
fn push_wire_capsule_y(
    verts: &mut Vec<GizmoVertex>,
    feet: GlamVec3,
    radius: f32,
    total_height: f32,
    color: [f32; 4],
) {
    let r = radius.max(0.05);
    let total_h = total_height.max(2.0 * r + 0.05);
    let cyl_h = (total_h - 2.0 * r).max(0.02);

    let tip = feet;
    let bot_center = GlamVec3::new(feet.x, feet.y + r, feet.z);
    let y_cyl_bot = feet.y + r;
    let y_cyl_top = y_cyl_bot + cyl_h;
    let top_center = GlamVec3::new(feet.x, y_cyl_top + r, feet.z);

    push_bottom_hemisphere_latitudes(verts, bot_center, r, color);
    push_top_hemisphere_latitudes(verts, top_center, r, color);

    push_horizontal_ring(verts, feet.x, y_cyl_bot, feet.z, r, color);
    push_horizontal_ring(verts, feet.x, y_cyl_top, feet.z, r, color);

    for i in 0..CAPSULE_WIRE_SEGMENTS {
        let a = (i as f32 / CAPSULE_WIRE_SEGMENTS as f32) * std::f32::consts::TAU;
        push_bottom_hemisphere_meridian(verts, tip, bot_center, r, a, color);

        let ca = a.cos();
        let sa = a.sin();
        let p_bot = GlamVec3::new(feet.x + r * ca, y_cyl_bot, feet.z + r * sa);
        let p_top = GlamVec3::new(feet.x + r * ca, y_cyl_top, feet.z + r * sa);
        push_line(verts, p_bot, p_top, color);

        let equator = p_top;
        push_top_hemisphere_meridian(verts, equator, top_center, r, a, color);
    }
}

fn push_character_capsule_overlay(
    verts: &mut Vec<GizmoVertex>,
    state: &State,
    id: u32,
    color: [f32; 4],
) {
    if state.play_character_entity == Some(id) {
        if let Some((feet, radius, total_h)) = state.play_character_capsule_wire_dims() {
            push_wire_capsule_y(verts, feet, radius, total_h, color);
        }
        return;
    }

    let Some(t) = state.world.get::<Transform>(id) else {
        return;
    };
    let radius = crate::config_3d::character_anchor::PLAY_CHARACTER_COLLIDER_RADIUS;
    let body_h = crate::config_3d::character_anchor::PLAY_CHARACTER_BODY_HEIGHT * t.scale.y.abs();
    let feet = crate::config_3d::character_anchor::feet_from_transform(
        t.position, t.scale, t.rotation, None,
    );
    push_wire_capsule_y(verts, feet, radius, body_h, color);
}

pub(crate) fn build_editor_collision_overlay(
    device: &wgpu::Device,
    state: &State,
) -> gizmo::GizmoBuffer {
    if state.preview_playing {
        return gizmo::build_from_vertices(device, &[]);
    }

    let mut verts: Vec<GizmoVertex> = Vec::new();

    for (entity, collider_handle) in state.physics.entity_collider_entries() {
        if state.editor_camera_entity == Some(entity) {
            continue;
        }
        let is_player = state.play_character_entity == Some(entity);
        if let Some((center, half)) = state.physics.collider_world_aabb(collider_handle) {
            let color = if is_player {
                COLOR_PLAYER_RAPIER_BUG
            } else {
                COLOR_RAPIER
            };
            push_wire_box(&mut verts, center, half, color);
        }
    }

    if let Some(id) = state.play_character_entity {
        if state.play_character_mesh_extents.is_none() {
            if let Some(t) = state.world.get::<Transform>(id) {
                let half = GlamVec3::new(
                    t.scale.x.abs() * 0.5,
                    t.scale.y.abs() * 0.5,
                    t.scale.z.abs() * 0.5,
                );
                push_wire_box(&mut verts, t.position, half, COLOR_PLAYER_PLACEHOLDER);
            }
        } else if let Some((feet, radius, total_h)) = state.play_character_capsule_wire_dims() {
            push_wire_capsule_y(&mut verts, feet, radius, total_h, COLOR_CHARACTER_CAPSULE);
        }
    }

    for &id in &state.character_entities {
        if state.play_character_entity == Some(id) {
            continue;
        }
        let category = state
            .save_registry
            .meta
            .get(&id)
            .and_then(|m| m.entity_category.as_deref());
        if entity_category_uses_character_capsule(category) {
            push_character_capsule_overlay(&mut verts, state, id, COLOR_CHARACTER_CAPSULE);
        }
    }

    gizmo::build_from_vertices(device, &verts)
}
