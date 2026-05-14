use glam::Vec3;

use crate::engine::State;
use crate::gizmo::{self, GizmoBuffer, GizmoVertex};

pub(crate) const DEFAULT_WORLD_WIDTH_3D: f32 = 100.0;
pub(crate) const DEFAULT_WORLD_HEIGHT_3D: f32 = 50.0;
pub(crate) const DEFAULT_WORLD_DEPTH_3D: f32 = 100.0;
const MIN_WORLD_DIMENSION_3D: f32 = 1.0;
const WORLD_BOUNDS_COLOR: [f32; 4] = [1.0, 0.35, 0.35, 0.24];

#[derive(Clone, Copy, Debug)]
pub(crate) struct WorldBounds3D {
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) depth: f32,
}

impl Default for WorldBounds3D {
    fn default() -> Self {
        Self {
            width: DEFAULT_WORLD_WIDTH_3D,
            height: DEFAULT_WORLD_HEIGHT_3D,
            depth: DEFAULT_WORLD_DEPTH_3D,
        }
    }
}

impl WorldBounds3D {
    pub(crate) fn new(width: f32, height: f32, depth: f32) -> Self {
        Self {
            width: width.max(MIN_WORLD_DIMENSION_3D),
            height: height.max(MIN_WORLD_DIMENSION_3D),
            depth: depth.max(MIN_WORLD_DIMENSION_3D),
        }
    }

    pub(crate) fn min_corner(&self) -> Vec3 {
        Vec3::new(-self.width * 0.5, 0.0, -self.depth * 0.5)
    }

    pub(crate) fn max_corner(&self) -> Vec3 {
        Vec3::new(self.width * 0.5, self.height, self.depth * 0.5)
    }

    pub(crate) fn clamp_sphere_center(&self, center: Vec3, radius: f32) -> Vec3 {
        let min = self.min_corner() + Vec3::splat(radius.max(0.0));
        let max = self.max_corner() - Vec3::splat(radius.max(0.0));
        Vec3::new(
            center.x.clamp(min.x.min(max.x), min.x.max(max.x)),
            center.y.clamp(min.y.min(max.y), min.y.max(max.y)),
            center.z.clamp(min.z.min(max.z), min.z.max(max.z)),
        )
    }

    pub(crate) fn intersects_aabb(&self, center: Vec3, scale: Vec3) -> bool {
        let half = scale.abs() * 0.5;
        let entity_min = center - half;
        let entity_max = center + half;
        let bounds_min = self.min_corner();
        let bounds_max = self.max_corner();

        !(entity_max.x < bounds_min.x
            || entity_min.x > bounds_max.x
            || entity_max.y < bounds_min.y
            || entity_min.y > bounds_max.y
            || entity_max.z < bounds_min.z
            || entity_min.z > bounds_max.z)
    }

    fn build_wireframe_vertices(&self) -> Vec<GizmoVertex> {
        let min = self.min_corner();
        let max = self.max_corner();

        let p000 = [min.x, min.y, min.z];
        let p001 = [min.x, min.y, max.z];
        let p010 = [min.x, max.y, min.z];
        let p011 = [min.x, max.y, max.z];
        let p100 = [max.x, min.y, min.z];
        let p101 = [max.x, min.y, max.z];
        let p110 = [max.x, max.y, min.z];
        let p111 = [max.x, max.y, max.z];

        let mut verts = Vec::with_capacity(24);
        let mut push_edge = |a: [f32; 3], b: [f32; 3]| {
            verts.push(GizmoVertex {
                position: a,
                color: WORLD_BOUNDS_COLOR,
            });
            verts.push(GizmoVertex {
                position: b,
                color: WORLD_BOUNDS_COLOR,
            });
        };

        push_edge(p000, p001);
        push_edge(p001, p011);
        push_edge(p011, p010);
        push_edge(p010, p000);

        push_edge(p100, p101);
        push_edge(p101, p111);
        push_edge(p111, p110);
        push_edge(p110, p100);

        push_edge(p000, p100);
        push_edge(p001, p101);
        push_edge(p010, p110);
        push_edge(p011, p111);

        verts
    }

    pub(crate) fn build_buffer(&self, device: &wgpu::Device) -> GizmoBuffer {
        gizmo::build_from_vertices(device, &self.build_wireframe_vertices())
    }
}

impl State {
    pub(crate) fn sync_world_bounds_3d_runtime(&mut self) {
        self.world_bounds_buffer = self.world_bounds_3d.build_buffer(&self.device);
        self.physics
            .rebuild_world_bounds_colliders(&self.world_bounds_3d);
    }

    pub(crate) fn set_world_bounds_3d_size(
        &mut self,
        width: f32,
        height: f32,
        depth: Option<f32>,
    ) {
        self.world_bounds_3d = WorldBounds3D::new(
            width,
            height,
            depth.unwrap_or(self.world_bounds_3d.depth),
        );
        self.sync_world_bounds_3d_runtime();
    }
}
