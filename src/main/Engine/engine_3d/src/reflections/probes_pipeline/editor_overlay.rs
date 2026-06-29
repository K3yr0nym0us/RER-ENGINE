//! Gizmo de editor para reflection probes (wireframe + cruz central).

use glam::Vec3;

use crate::engine::State;
use crate::gizmo::{self, GizmoBuffer, GizmoVertex};
use crate::reflections::probes_pipeline::registry::{self, REFLECTION_PROBE_GIZMO_RADIUS_M};

const PROBE_SPHERE_COLOR: [f32; 4] = [0.28, 0.82, 1.0, 0.88];
const PROBE_CENTER_COLOR: [f32; 4] = [1.0, 0.92, 0.35, 1.0];
const WIRE_MERIDIANS: usize = 12;
const WIRE_LATITUDE_RINGS: usize = 6;

fn push_wire_line(verts: &mut Vec<GizmoVertex>, a: Vec3, b: Vec3, color: [f32; 4]) {
    verts.push(GizmoVertex {
        position: a.to_array(),
        color,
    });
    verts.push(GizmoVertex {
        position: b.to_array(),
        color,
    });
}

fn push_latitude_ring(
    verts: &mut Vec<GizmoVertex>,
    center: Vec3,
    radius: f32,
    phi: f32,
    segments: usize,
    color: [f32; 4],
) {
    let sin_phi = phi.sin();
    let cos_phi = phi.cos();
    let rr = radius * sin_phi.abs().max(0.001);
    let y = center.y + radius * cos_phi;
    for i in 0..segments {
        let a0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
        let p0 = Vec3::new(center.x + a0.cos() * rr, y, center.z + a0.sin() * rr);
        let p1 = Vec3::new(center.x + a1.cos() * rr, y, center.z + a1.sin() * rr);
        push_wire_line(verts, p0, p1, color);
    }
}

fn append_probe_wire_sphere(verts: &mut Vec<GizmoVertex>, center: Vec3, radius: f32) {
    let segments = WIRE_MERIDIANS * 2;
    for ring in 0..=WIRE_LATITUDE_RINGS {
        let phi = (ring as f32 / WIRE_LATITUDE_RINGS as f32) * std::f32::consts::PI;
        push_latitude_ring(verts, center, radius, phi, segments, PROBE_SPHERE_COLOR);
    }
    for m in 0..WIRE_MERIDIANS {
        let azimuth = (m as f32 / WIRE_MERIDIANS as f32) * std::f32::consts::TAU;
        let ca = azimuth.cos();
        let sa = azimuth.sin();
        let steps = WIRE_LATITUDE_RINGS * 2;
        let mut prev = center + Vec3::new(0.0, radius, 0.0);
        for s in 1..=steps {
            let phi = (s as f32 / steps as f32) * std::f32::consts::PI;
            let sin_phi = phi.sin();
            let cos_phi = phi.cos();
            let p = center
                + Vec3::new(radius * ca * sin_phi, radius * cos_phi, radius * sa * sin_phi);
            push_wire_line(verts, prev, p, PROBE_SPHERE_COLOR);
            prev = p;
        }
    }
}

fn append_probe_center_marker(verts: &mut Vec<GizmoVertex>, center: Vec3, radius: f32) {
    let s = (radius * 0.12).clamp(0.15, 0.45);
    push_wire_line(verts, center + Vec3::new(-s, 0.0, 0.0), center + Vec3::new(s, 0.0, 0.0), PROBE_CENTER_COLOR);
    push_wire_line(verts, center + Vec3::new(0.0, -s, 0.0), center + Vec3::new(0.0, s, 0.0), PROBE_CENTER_COLOR);
    push_wire_line(verts, center + Vec3::new(0.0, 0.0, -s), center + Vec3::new(0.0, 0.0, s), PROBE_CENTER_COLOR);
}

pub(crate) fn build_reflection_probe_editor_overlay(
    device: &wgpu::Device,
    state: &State,
) -> GizmoBuffer {
    let mut verts = Vec::new();
    for id in registry::reflection_probe_entities(&state.save_registry) {
        let Some(t) = state.world.get::<crate::ecs::Transform>(id) else {
            continue;
        };
        let center = t.position;
        let radius = REFLECTION_PROBE_GIZMO_RADIUS_M;
        append_probe_wire_sphere(&mut verts, center, radius);
        append_probe_center_marker(&mut verts, center, radius);
    }
    gizmo::build_from_vertices(device, &verts)
}
