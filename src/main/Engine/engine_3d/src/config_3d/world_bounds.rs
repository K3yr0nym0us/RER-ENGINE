use glam::Vec3;

use crate::engine::State;
use crate::gizmo::{self, GizmoBuffer, GizmoVertex};

pub(crate) const DEFAULT_WORLD_RADIUS_3D: f32 = 50.0;
pub(crate) const MIN_WORLD_RADIUS_3D: f32 = 5.0;
pub(crate) const MAX_WORLD_RADIUS_3D: f32 = 500.0;
const WORLD_BOUNDS_COLOR: [f32; 4] = [1.0, 0.35, 0.35, 0.55];

/// Cielo interior esférico (también referencia para IBL/SSR procedural).
pub(crate) const SKY_HORIZON: [f32; 3] = [0.72, 0.86, 0.98];

const SKY_SHELL_INSET: f32 = 0.35;
const SKY_SPHERE_RINGS: u32 = 32;
const SKY_SPHERE_SECTORS: u32 = 48;
const BOUNDS_WIRE_MERIDIANS: usize = 16;
const BOUNDS_WIRE_LATITUDE_RINGS: usize = 8;

#[derive(Clone, Copy, Debug)]
pub(crate) struct WorldBounds3D {
    pub(crate) radius: f32,
}

impl Default for WorldBounds3D {
    fn default() -> Self {
        Self {
            radius: DEFAULT_WORLD_RADIUS_3D,
        }
    }
}

fn sky_rgba(rgb: [f32; 3]) -> [f32; 4] {
    [rgb[0], rgb[1], rgb[2], 1.0]
}

fn push_sky_tri(
    verts: &mut Vec<GizmoVertex>,
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    color: [f32; 4],
) {
    verts.push(GizmoVertex { position: a, color });
    verts.push(GizmoVertex { position: c, color });
    verts.push(GizmoVertex { position: b, color });
}

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

impl WorldBounds3D {
    pub(crate) fn new(radius: f32) -> Self {
        Self {
            radius: radius.clamp(MIN_WORLD_RADIUS_3D, MAX_WORLD_RADIUS_3D),
        }
    }

    pub(crate) fn from_legacy_box(width: f32, height: f32, depth: f32) -> Self {
        let r = (width.min(height).min(depth) * 0.5).max(MIN_WORLD_RADIUS_3D);
        Self::new(r)
    }

    /// Centro en el origen: el ecuador (y=0) coincide con el suelo.
    pub(crate) fn sphere_center(&self) -> Vec3 {
        Vec3::ZERO
    }

    pub(crate) fn diameter(&self) -> f32 {
        self.radius * 2.0
    }

    pub(crate) fn min_corner(&self) -> Vec3 {
        Vec3::splat(-self.radius)
    }

    pub(crate) fn max_corner(&self) -> Vec3 {
        Vec3::splat(self.radius)
    }

    pub(crate) fn clamp_sphere_center(&self, center: Vec3, entity_radius: f32) -> Vec3 {
        let margin = entity_radius.max(0.0);
        let max_dist = (self.radius - margin).max(0.01);
        let dist = center.length();
        let mut out = if dist > max_dist && dist > 1.0e-6 {
            center * (max_dist / dist)
        } else {
            center
        };
        // Hemisferio jugable: el suelo es el ecuador (y=0).
        out.y = out.y.max(0.0);
        out
    }

    pub(crate) fn intersects_world_aabb(&self, center: Vec3, half: Vec3) -> bool {
        let entity_min = center - half;
        let entity_max = center + half;
        let sc = self.sphere_center();
        let r = self.radius;
        let closest = Vec3::new(
            sc.x.clamp(entity_min.x, entity_max.x),
            sc.y.clamp(entity_min.y, entity_max.y),
            sc.z.clamp(entity_min.z, entity_max.z),
        );
        (closest - sc).length_squared() <= r * r
    }

    fn build_wireframe_vertices(&self) -> Vec<GizmoVertex> {
        let center = self.sphere_center();
        let radius = self.radius;
        let mut verts = Vec::new();

        push_latitude_ring(
            &mut verts,
            center,
            radius,
            std::f32::consts::FRAC_PI_2,
            BOUNDS_WIRE_MERIDIANS * 2,
            WORLD_BOUNDS_COLOR,
        );

        for ring in 1..BOUNDS_WIRE_LATITUDE_RINGS {
            let phi =
                (ring as f32 / BOUNDS_WIRE_LATITUDE_RINGS as f32) * std::f32::consts::FRAC_PI_2;
            push_latitude_ring(
                &mut verts,
                center,
                radius,
                phi,
                BOUNDS_WIRE_MERIDIANS * 2,
                WORLD_BOUNDS_COLOR,
            );
        }

        for m in 0..BOUNDS_WIRE_MERIDIANS {
            let azimuth = (m as f32 / BOUNDS_WIRE_MERIDIANS as f32) * std::f32::consts::TAU;
            let ca = azimuth.cos();
            let sa = azimuth.sin();
            let steps = BOUNDS_WIRE_LATITUDE_RINGS * 2;
            let mut prev = center + Vec3::new(0.0, radius, 0.0);
            for s in 1..=steps {
                let phi = (s as f32 / steps as f32) * std::f32::consts::FRAC_PI_2;
                let sin_phi = phi.sin();
                let cos_phi = phi.cos();
                let p = center
                    + Vec3::new(
                        radius * ca * sin_phi,
                        radius * cos_phi,
                        radius * sa * sin_phi,
                    );
                push_wire_line(&mut verts, prev, p, WORLD_BOUNDS_COLOR);
                prev = p;
            }
        }

        verts
    }

    /// Cúpula interior (hemisferio superior, ecuador en y=0).
    fn build_sky_sphere_vertices(&self) -> Vec<GizmoVertex> {
        let center = self.sphere_center();
        let radius = (self.radius - SKY_SHELL_INSET).max(1.0);
        let color = sky_rgba(SKY_HORIZON);
        let rings = SKY_SPHERE_RINGS.max(8);
        let sectors = SKY_SPHERE_SECTORS.max(12);
        let stride = sectors + 1;

        let mut unit_dirs: Vec<Vec3> = Vec::with_capacity(((rings + 1) * stride) as usize);
        for ring in 0..=rings {
            let v = ring as f32 / rings as f32;
            let phi = v * std::f32::consts::FRAC_PI_2;
            let sin_phi = phi.sin();
            let cos_phi = phi.cos();
            for sector in 0..=sectors {
                let theta = (sector as f32 / sectors as f32) * std::f32::consts::TAU;
                let dir = Vec3::new(sin_phi * theta.cos(), cos_phi, sin_phi * theta.sin());
                unit_dirs.push(dir);
            }
        }

        let mut verts = Vec::with_capacity((rings * sectors * 6) as usize);
        for ring in 0..rings {
            for sector in 0..sectors {
                let cur = ring * stride + sector;
                let next = cur + stride;
                let a = (center + unit_dirs[cur as usize] * radius).to_array();
                let b = (center + unit_dirs[(cur + 1) as usize] * radius).to_array();
                let c = (center + unit_dirs[next as usize] * radius).to_array();
                let d = (center + unit_dirs[(next + 1) as usize] * radius).to_array();
                push_sky_tri(&mut verts, a, b, c, color);
                push_sky_tri(&mut verts, c, b, d, color);
            }
        }

        verts
    }

    pub(crate) fn build_buffer(&self, device: &wgpu::Device) -> GizmoBuffer {
        gizmo::build_from_vertices(device, &self.build_wireframe_vertices())
    }

    pub(crate) fn build_sky_buffer(&self, device: &wgpu::Device) -> GizmoBuffer {
        gizmo::build_from_vertices(device, &self.build_sky_sphere_vertices())
    }
}

impl State {
    pub(crate) fn sync_world_bounds_3d_runtime(&mut self) {
        self.world_bounds_buffer = self.world_bounds_3d.build_buffer(&self.device);
        self.world_sky_buffer = self.world_bounds_3d.build_sky_buffer(&self.device);
        self.physics
            .rebuild_world_bounds_colliders(&self.world_bounds_3d);
    }

    pub(crate) fn set_world_bounds_3d_radius(&mut self, radius: f32) {
        self.world_bounds_3d = WorldBounds3D::new(radius);
        self.sync_world_bounds_3d_runtime();
        self.sync_ground_plane_to_world_bounds();
    }

    pub(crate) fn set_world_bounds_3d_size(&mut self, width: f32, height: f32, depth: Option<f32>) {
        self.set_world_bounds_3d_radius(
            WorldBounds3D::from_legacy_box(width, height, depth.unwrap_or(height)).radius,
        );
    }
}
