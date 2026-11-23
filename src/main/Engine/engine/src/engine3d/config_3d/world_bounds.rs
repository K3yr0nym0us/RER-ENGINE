use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use crate::engine3d::config_3d::directional_light::DEFAULT_LIGHT_DIR;
use crate::engine3d::ecs::Transform;
use crate::engine3d::engine::State;
use crate::engine3d::gizmo::{GizmoBuffer, GizmoVertex};

/// Extensión por defecto de cada cara desde el origen (mundo 100×100).
pub(crate) const DEFAULT_WORLD_FACE_3D: f32 = 50.0;
/// Cara mínima = 15 m → lado 30 m, radio 15 m.
///
/// Coincide con `worldgen::MIN_WORLDGEN_RADIUS` (15 m) y con el `MIN_DIM`
/// de `WorldBounds::validate` del plugin (30 m de lado). Duplicado a propósito:
/// el plugin puede no estar instalado, así que el motor no depende de él.
/// Si cambias este valor, actualiza también los comentarios de
/// `worldgen/src/config.rs` y `worldgen/src/scale.rs`.
pub(crate) const MIN_WORLD_FACE_3D: f32 = 15.0;
/// Cara máxima = 1000 m → lado 2000 m, radio 1000 m.
/// Coincide con `worldgen::MAX_WORLDGEN_RADIUS` y el `MAX_DIM` del plugin.
pub(crate) const MAX_WORLD_FACE_3D: f32 = 1000.0;
/// Radio equivalente a la cara (radio = semieje del cuadrado).
/// Usado por el editor para el slider `set_world_radius`.
#[cfg_attr(not(feature = "editor"), allow(dead_code))]
pub(crate) const MIN_WORLD_RADIUS_3D: f32 = MIN_WORLD_FACE_3D;
#[cfg_attr(not(feature = "editor"), allow(dead_code))]
pub(crate) const MAX_WORLD_RADIUS_3D: f32 = MAX_WORLD_FACE_3D;
/// Altura por defecto del cielo/volumen.
pub(crate) const DEFAULT_WORLD_HEIGHT_3D: f32 = 100.0;
/// Altura mínima del volumen: `worldgen::MIN_WORLDGEN_HEIGHT` / `MIN_HEIGHT` del plugin.
pub(crate) const MIN_WORLD_HEIGHT_3D: f32 = 48.0;
/// Altura máxima del volumen: `worldgen::MAX_WORLDGEN_HEIGHT` / `MAX_HEIGHT` del plugin.
pub(crate) const MAX_WORLD_HEIGHT_3D: f32 = 640.0;

const WORLD_BOUNDS_COLOR: [f32; 4] = [1.0, 0.35, 0.35, 0.55];

/// Cara inferior de la esfera de cielo: azul muy oscuro, casi negro.
pub(crate) const SKY_BOTTOM: [f32; 3] = [0.02, 0.03, 0.08];
/// Cara superior de la esfera de cielo: azul cielo pleno.
pub(crate) const SKY_TOP: [f32; 3] = [0.30, 0.60, 0.95];
/// Fracción de la altura de la esfera a la que el degradado ya es azul al 100 %.
/// Cuanto más bajo, más espacio ocupa el azul: con 0.35 el azul domina ~2/3 de la esfera.
const SKY_GRADIENT_END: f32 = 0.35;

/// Azul más claro que se mezcla en la zona del cielo alineada con el sol.
const SKY_SUN_TINT: [f32; 3] = [0.62, 0.84, 1.0];
/// Intensidad máxima del brillo junto al sol (coincide con el exponente `pow(..., 6.0)`
/// de los shaders de fondo).
const SKY_SUN_GLOW: f32 = 0.6;
/// Radio FIJO de la esfera del cielo, independiente del tamaño del mundo.
/// Debe ser mayor que la semidiagonal del mundo máximo permitido:
/// sqrt(1000² + 640² + 1000²) ≈ 1554 m; 1600 m deja margen con el piso expandido.
/// Referencia Cruzada: `MIN/MAX_WORLD_FACE_3D` y `MIN/MAX_WORLD_HEIGHT_3D`.
pub(crate) const SKY_SPHERE_RADIUS: f32 = 1600.0;

/// Parámetros de la esfera de cielo para el shader de fondo (`TraceU` / `Camera`).
/// Cinco `vec4` en std140.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct SkyUniform {
    /// xyz = centro de la esfera, w = radio.
    pub(crate) center_radius: [f32; 4],
    /// rgb = color de la cara inferior (azul oscuro, casi negro).
    pub(crate) bottom: [f32; 4],
    /// rgb = color de la cara superior (azul), w = punto del degradado donde se alcanza el azul.
    pub(crate) top: [f32; 4],
    /// xyz = posición mundial del sol, w = intensidad del brillo.
    pub(crate) sun: [f32; 4],
    /// rgb = azul claro usado en el brillo alineado con el sol, w = factor de día [0,1].
    pub(crate) sun_color: [f32; 4],
}

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
    /// Límite inferior del volumen (m). 0 = llanuras; negativo si una cueva
    /// baja del suelo y el generador ensancha la caja al terminar.
    pub(crate) floor: f32,
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
            floor: 0.0,
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
            floor: 0.0,
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
            floor: 0.0,
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

    /// Esfera de cielo de radio fijo, centrada en la caja.
    /// La usa el shader de fondo del renderer.
    pub(crate) fn sky_uniform(&self) -> SkyUniform {
        let footprint = self.center();
        let center = Vec3::new(footprint.x, (self.floor + self.height) * 0.5, footprint.z);
        let radius = SKY_SPHERE_RADIUS;
        SkyUniform {
            center_radius: [center.x, center.y, center.z, radius],
            bottom: sky_rgba(SKY_BOTTOM),
            top: [SKY_TOP[0], SKY_TOP[1], SKY_TOP[2], SKY_GRADIENT_END],
            // El frame sustituye `sun` por la posición real; mientras tanto la
            // malla usa una posición por defecto sobre la dirección de luz inicial.
            sun: {
                let pos = center + DEFAULT_LIGHT_DIR.normalize() * (radius * 0.96);
                [pos.x, pos.y, pos.z, SKY_SUN_GLOW]
            },
            sun_color: sky_rgba(SKY_SUN_TINT),
        }
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

    pub(crate) fn min_y(&self) -> f32 {
        self.floor.min(0.0)
    }

    pub(crate) fn min_corner(&self) -> Vec3 {
        Vec3::new(self.min_x(), self.min_y(), self.min_z())
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
        out.y = out.y.max(self.min_y());
        out
    }

    pub(crate) fn intersects_world_aabb(&self, center: Vec3, half: Vec3) -> bool {
        let entity_min = center - half;
        let entity_max = center + half;
        entity_max.x >= self.min_x()
            && entity_min.x <= self.max_x()
            && entity_max.z >= self.min_z()
            && entity_min.z <= self.max_z()
            && entity_max.y >= self.min_y()
            && entity_min.y <= self.height
    }

    /// Aristas de la caja (base y techo + verticales).
    fn build_wireframe_vertices(&self) -> Vec<GizmoVertex> {
        let mut verts = Vec::new();
        let (x0, x1) = (self.min_x(), self.max_x());
        let (z0, z1) = (self.min_z(), self.max_z());
        let (y0, y1) = (self.min_y(), self.height);

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

    pub(crate) fn build_buffer(&self, gpu: &crate::engine3d::vulkan::SharedGpu) -> GizmoBuffer {
        crate::engine3d::gizmo::build_lines_from_vertices(gpu, &self.build_wireframe_vertices())
    }
}

impl State {
    /// Sky uniform del frame: el degradado vertical se mantiene y se añade el
    /// brillo en la posición real del sol (la que ve la cámara).
    pub(crate) fn sky_uniform_for_frame(&self) -> SkyUniform {
        let mut sky = self.world_bounds_3d.sky_uniform();
        let sun_pos = self
            .sun_entity
            .and_then(|id| self.world.get::<Transform>(id))
            .map(|t| t.position)
            .unwrap_or_else(|| {
                self.directional_light_scene_center()
                    + self.directional_light_dir * self.sun_orbit_distance()
            });
        sky.sun = [sun_pos.x, sun_pos.y, sun_pos.z, SKY_SUN_GLOW];
        // w del color del sol = factor de luz diurna: el fondo se oscurece
        // al atardecer sin llegar a negro absoluto.
        sky.sun_color[3] = self.day_factor();
        sky
    }

    pub(crate) fn sync_world_bounds_3d_runtime(&mut self) {
        self.world_bounds_buffer = self.world_bounds_3d.build_buffer(&self.vulkan);
        self.physics
            .rebuild_world_bounds_colliders(&self.world_bounds_3d);
        // El sol se coloca fuera de la caja: la cámara debe seguir viéndolo.
        let sky_radius = self.world_bounds_3d.sky_uniform().center_radius[3];
        self.camera.far = self.camera.far.max(sky_radius * 1.1);
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
        let floor = self.world_bounds_3d.floor;
        self.world_bounds_3d = WorldBounds3D::with_faces(north, south, east, west, height);
        self.world_bounds_3d.floor = floor;
        self.sync_world_bounds_3d_runtime();
        self.sync_ground_plane_to_world_bounds();
        self.keep_sun_inside_sky();
    }

    /// Mundo cuadrado centrado con el radio indicado.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_match_the_worldgen_plugin_contract() {
        // Valores en duro del plugin worldgen (no se importa: puede no estar instalado).
        // worldgen/src/scale.rs: MIN/MAX_WORLDGEN_RADIUS, MIN/MAX_WORLDGEN_HEIGHT.
        // worldgen/src/config.rs: WorldBounds::validate -> MIN_DIM/MAX_DIM, MIN/MAX_HEIGHT.
        assert_eq!(MIN_WORLD_FACE_3D * 2.0, 30.0);
        assert_eq!(MAX_WORLD_FACE_3D * 2.0, 2000.0);
        assert_eq!(MIN_WORLD_RADIUS_3D, 15.0);
        assert_eq!(MAX_WORLD_RADIUS_3D, 1000.0);
        assert_eq!(MIN_WORLD_HEIGHT_3D, 48.0);
        assert_eq!(MAX_WORLD_HEIGHT_3D, 640.0);
        // El cielo es fijo y mayor que la semidiagonal del mundo máximo (~1420 m).
        const { assert!(SKY_SPHERE_RADIUS > 1500.0) };
    }

    #[test]
    fn sky_sphere_adapts_to_bounds_and_encloses_the_box() {
        let bounds = WorldBounds3D::with_faces(40.0, 60.0, 80.0, 40.0, 150.0);
        let sky = bounds.sky_uniform();
        let center = Vec3::from_array([
            sky.center_radius[0],
            sky.center_radius[1],
            sky.center_radius[2],
        ]);
        let radius = sky.center_radius[3];

        // Centro en la huella (caras distintas) y a media altura.
        assert!((center.x - 20.0).abs() < 1e-3);
        assert!((center.z - 10.0).abs() < 1e-3);
        assert!((center.y - 75.0).abs() < 1e-3);

        // La esfera es algo mayor que la caja: ningún vértice queda fuera.
        for x in [bounds.min_x(), bounds.max_x()] {
            for y in [0.0, bounds.height] {
                for z in [bounds.min_z(), bounds.max_z()] {
                    assert!(
                        Vec3::new(x, y, z).distance(center) < radius,
                        "la esfera debe contener la esquina ({x}, {y}, {z})"
                    );
                }
            }
        }
    }

    #[test]
    fn sky_uniform_carries_the_gradient_and_sun_glow() {
        let sky = WorldBounds3D::default().sky_uniform();
        assert_eq!(sky.bottom, sky_rgba(SKY_BOTTOM));
        assert_eq!(sky.top[0..3], SKY_TOP);
        assert!(sky.top[3] > 0.0 && sky.top[3] < 1.0);
        assert_eq!(sky.sun_color, sky_rgba(SKY_SUN_TINT));
        assert!(sky.center_radius[3] > 0.0);
    }
}
