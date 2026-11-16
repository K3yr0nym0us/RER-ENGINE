use glam::Vec3;

use crate::engine3d::engine::State;
use crate::engine3d::gizmo::{GizmoBuffer, GizmoVertex};

/// Extensión por defecto de cada cara desde el origen (mundo 100×100).
pub(crate) const DEFAULT_WORLD_FACE_3D: f32 = 50.0;
/// Extensión mínima por cara (10 m de lado mínimo).
pub(crate) const MIN_WORLD_FACE_3D: f32 = 5.0;
/// Extensión máxima por cara (2000 m de lado máximo).
pub(crate) const MAX_WORLD_FACE_3D: f32 = 1000.0;
/// Compatibilidad con el modelo esférico anterior (radio = semieje).
/// Solo lo usa el editor (slider legacy); en el motor 3D aislado no se referencia.
#[cfg_attr(not(feature = "editor"), allow(dead_code))]
pub(crate) const MIN_WORLD_RADIUS_3D: f32 = MIN_WORLD_FACE_3D;
#[cfg_attr(not(feature = "editor"), allow(dead_code))]
pub(crate) const MAX_WORLD_RADIUS_3D: f32 = MAX_WORLD_FACE_3D;
/// Altura por defecto del cielo/volumen.
pub(crate) const DEFAULT_WORLD_HEIGHT_3D: f32 = 100.0;
pub(crate) const MIN_WORLD_HEIGHT_3D: f32 = 10.0;
pub(crate) const MAX_WORLD_HEIGHT_3D: f32 = 2000.0;

const WORLD_BOUNDS_COLOR: [f32; 4] = [1.0, 0.35, 0.35, 0.55];

/// Cielo interior (también referencia para IBL/SSR procedural).
pub(crate) const SKY_HORIZON: [f32; 3] = [0.72, 0.86, 0.98];

const SKY_SHELL_INSET: f32 = 0.35;

/// Mundo rectangular: cada cara es su extensión desde el origen.
///
/// - `north` crece hacia **-Z**, `south` hacia **+Z**
/// - `west` crece hacia **-X**, `east` hacia **+X**
///
/// Al cambiar una cara la opuesta se queda fija, así el mundo puede crecer por
/// un lado sin recentrarse (`docs/yaml/editor/Editor_Native_Shell.yaml`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WorldBounds3D {
    pub(crate) north: f32,
    pub(crate) south: f32,
    pub(crate) east: f32,
    pub(crate) west: f32,
    pub(crate) height: f32,
}

impl Default for WorldBounds3D {
    fn default() -> Self {
        Self::new_square(DEFAULT_WORLD_FACE_3D * 2.0)
    }
}

fn clamp_face(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(MIN_WORLD_FACE_3D, MAX_WORLD_FACE_3D)
    } else {
        DEFAULT_WORLD_FACE_3D
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

impl WorldBounds3D {
    /// Mundo cuadrado centrado en el origen (lado = `size`).
    pub(crate) fn new_square(size: f32) -> Self {
        let half = clamp_face(size * 0.5);
        Self {
            north: half,
            south: half,
            east: half,
            west: half,
            height: DEFAULT_WORLD_HEIGHT_3D,
        }
    }

    /// Conserva el cuadrado histórico (`width × depth`) centrado en el origen.
    pub(crate) fn from_legacy_box(width: f32, height: f32, depth: f32) -> Self {
        Self {
            north: clamp_face(depth * 0.5),
            south: clamp_face(depth * 0.5),
            east: clamp_face(width * 0.5),
            west: clamp_face(width * 0.5),
            height: height.clamp(MIN_WORLD_HEIGHT_3D, MAX_WORLD_HEIGHT_3D),
        }
    }

    /// Aplica las cuatro caras y la altura (cada cara se satura por separado).
    pub(crate) fn with_faces(north: f32, south: f32, east: f32, west: f32, height: f32) -> Self {
        Self {
            north: clamp_face(north),
            south: clamp_face(south),
            east: clamp_face(east),
            west: clamp_face(west),
            height: height.clamp(MIN_WORLD_HEIGHT_3D, MAX_WORLD_HEIGHT_3D),
        }
    }

    pub(crate) fn width(&self) -> f32 {
        self.east + self.west
    }

    pub(crate) fn depth(&self) -> f32 {
        self.north + self.south
    }

    /// Centro XZ de la huella (el origen no es el centro si las caras difieren).
    pub(crate) fn center(&self) -> Vec3 {
        Vec3::new(
            (self.east - self.west) * 0.5,
            0.0,
            (self.south - self.north) * 0.5,
        )
    }

    /// Radio equivalente (mayor semieje) para sombras, sol y niebla.
    pub(crate) fn radius(&self) -> f32 {
        self.width().max(self.depth()) * 0.5
    }

    pub(crate) fn min_x(&self) -> f32 {
        -self.west
    }

    pub(crate) fn max_x(&self) -> f32 {
        self.east
    }

    pub(crate) fn min_z(&self) -> f32 {
        -self.north
    }

    pub(crate) fn max_z(&self) -> f32 {
        self.south
    }

    pub(crate) fn min_corner(&self) -> Vec3 {
        Vec3::new(self.min_x(), 0.0, self.min_z())
    }

    pub(crate) fn max_corner(&self) -> Vec3 {
        Vec3::new(self.max_x(), self.height, self.max_z())
    }

    /// `true` si `(x, z)` cae dentro de la huella del mundo.
    pub(crate) fn contains_xz(&self, x: f32, z: f32) -> bool {
        x >= self.min_x() && x <= self.max_x() && z >= self.min_z() && z <= self.max_z()
    }

    pub(crate) fn clamp_sphere_center(&self, center: Vec3, entity_radius: f32) -> Vec3 {
        let margin = entity_radius.max(0.0);
        let mut out = center;
        let x_lo = self.min_x() + margin;
        let x_hi = self.max_x() - margin;
        let z_lo = self.min_z() + margin;
        let z_hi = self.max_z() - margin;
        out.x = if x_lo <= x_hi {
            center.x.clamp(x_lo, x_hi)
        } else {
            (self.min_x() + self.max_x()) * 0.5
        };
        out.z = if z_lo <= z_hi {
            center.z.clamp(z_lo, z_hi)
        } else {
            (self.min_z() + self.max_z()) * 0.5
        };
        out.y = out.y.max(0.0);
        out
    }

    pub(crate) fn intersects_world_aabb(&self, center: Vec3, half: Vec3) -> bool {
        let entity_min = center - half;
        let entity_max = center + half;
        entity_max.x >= self.min_x()
            && entity_min.x <= self.max_x()
            && entity_max.z >= self.min_z()
            && entity_min.z <= self.max_z()
            && entity_max.y >= 0.0
            && entity_min.y <= self.height
    }

    /// Aristas de la caja (base y techo + verticales).
    fn build_wireframe_vertices(&self) -> Vec<GizmoVertex> {
        let mut verts = Vec::new();
        let (x0, x1) = (self.min_x(), self.max_x());
        let (z0, z1) = (self.min_z(), self.max_z());
        let (y0, y1) = (0.0, self.height);

        let corners = |y: f32| {
            [
                Vec3::new(x0, y, z0),
                Vec3::new(x1, y, z0),
                Vec3::new(x1, y, z1),
                Vec3::new(x0, y, z1),
            ]
        };
        let bottom = corners(y0);
        let top = corners(y1);
        for i in 0..4 {
            push_wire_line(
                &mut verts,
                bottom[i],
                bottom[(i + 1) % 4],
                WORLD_BOUNDS_COLOR,
            );
            push_wire_line(&mut verts, top[i], top[(i + 1) % 4], WORLD_BOUNDS_COLOR);
            push_wire_line(&mut verts, bottom[i], top[i], WORLD_BOUNDS_COLOR);
        }
        verts
    }

    /// Caja interior: paredes + techo (el suelo lo cubre el checker).
    fn build_sky_sphere_vertices(&self) -> Vec<GizmoVertex> {
        let inset = SKY_SHELL_INSET;
        let color = sky_rgba(SKY_HORIZON);
        let x0 = self.min_x() + inset;
        let x1 = self.max_x() - inset;
        let z0 = self.min_z() + inset;
        let z1 = self.max_z() - inset;
        let y0 = inset;
        let y1 = self.height - inset;
        if x0 >= x1 || z0 >= z1 || y0 >= y1 {
            return Vec::new();
        }

        let mut verts = Vec::new();
        // Paredes (triángulos con las dos orientaciones: el sky no hace culling).
        let walls = [
            // -Z (norte)
            [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0]],
            // +Z (sur)
            [[x1, y0, z1], [x0, y0, z1], [x0, y1, z1], [x1, y1, z1]],
            // -X (oeste)
            [[x0, y0, z1], [x0, y0, z0], [x0, y1, z0], [x0, y1, z1]],
            // +X (este)
            [[x1, y0, z0], [x1, y0, z1], [x1, y1, z1], [x1, y1, z0]],
        ];
        for [a, b, c, d] in walls {
            push_sky_tri(&mut verts, a, b, c, color);
            push_sky_tri(&mut verts, a, c, d, color);
            push_sky_tri(&mut verts, a, c, b, color);
            push_sky_tri(&mut verts, a, d, c, color);
        }
        // Techo
        let (a, b, c, d) = ([x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]);
        push_sky_tri(&mut verts, a, b, c, color);
        push_sky_tri(&mut verts, a, c, d, color);
        push_sky_tri(&mut verts, a, c, b, color);
        push_sky_tri(&mut verts, a, d, c, color);
        verts
    }

    pub(crate) fn build_buffer(&self, gpu: &crate::engine3d::vulkan::SharedGpu) -> GizmoBuffer {
        crate::engine3d::gizmo::build_lines_from_vertices(gpu, &self.build_wireframe_vertices())
    }

    pub(crate) fn build_sky_buffer(&self, gpu: &crate::engine3d::vulkan::SharedGpu) -> GizmoBuffer {
        crate::engine3d::gizmo::build_from_vertices(gpu, &self.build_sky_sphere_vertices())
    }
}

impl State {
    pub(crate) fn sync_world_bounds_3d_runtime(&mut self) {
        self.world_bounds_buffer = self.world_bounds_3d.build_buffer(&self.vulkan);
        self.world_sky_buffer = self.world_bounds_3d.build_sky_buffer(&self.vulkan);
        self.physics
            .rebuild_world_bounds_colliders(&self.world_bounds_3d);
    }

    /// Aplica las cuatro caras del mundo; la cara opuesta no se mueve.
    pub(crate) fn set_world_bounds_3d_faces(
        &mut self,
        north: f32,
        south: f32,
        east: f32,
        west: f32,
    ) {
        let height = self.world_bounds_3d.height;
        self.world_bounds_3d = WorldBounds3D::with_faces(north, south, east, west, height);
        self.sync_world_bounds_3d_runtime();
        self.sync_ground_plane_to_world_bounds();
        self.keep_sun_inside_sky();
    }

    /// Compatibilidad: mundo cuadrado centrado con el radio indicado.
    pub(crate) fn set_world_bounds_3d_radius(&mut self, radius: f32) {
        let size = (radius.clamp(MIN_WORLD_FACE_3D, MAX_WORLD_FACE_3D)) * 2.0;
        self.world_bounds_3d = WorldBounds3D::new_square(size);
        self.sync_world_bounds_3d_runtime();
        self.sync_ground_plane_to_world_bounds();
        self.keep_sun_inside_sky();
    }

    /// Compatibilidad: caja centrada (width × depth) con altura.
    pub(crate) fn set_world_bounds_3d_size(&mut self, width: f32, height: f32, depth: Option<f32>) {
        self.world_bounds_3d =
            WorldBounds3D::from_legacy_box(width, height, depth.unwrap_or(height));
        self.sync_world_bounds_3d_runtime();
        self.sync_ground_plane_to_world_bounds();
        self.keep_sun_inside_sky();
    }
}
