//! Índices espaciales XZ para sitios de features y shapes de carving.
//!
//! Ambos usan buckets de rejilla: cada elemento se inserta en **todas** las
//! celdas que solapa su AABB expandido por su radio máximo, de modo que la
//! consulta solo necesita la celda del punto consultado (O(1) por punto).

use std::collections::HashMap;

use glam::{Vec2, Vec3};

use crate::field::Terrain;
use crate::planner::{CaveSystem, FeatureKind, FeaturePlan, RiverPath, Site};

/// Tamaño de celda para sitios de features (metros).
const SITE_CELL: i32 = 16;
/// Tamaño de celda para shapes de carving (metros).
const CARVE_CELL: i32 = 8;

/// Índice espacial de sitios de features (montañas, colinas, valles, llanuras,
/// acantilados, lagos).
#[derive(Debug, Default)]
pub(crate) struct SiteIndex {
    sites: Vec<Site>,
    buckets: HashMap<(i32, i32), Vec<u32>>,
}

impl SiteIndex {
    /// Construye el índice a partir de los sitios del plan.
    #[must_use]
    pub fn new(sites: Vec<Site>) -> Self {
        let mut index = Self {
            sites,
            buckets: HashMap::new(),
        };
        for (i, site) in index.sites.iter().enumerate() {
            insert_aabb(
                &mut index.buckets,
                site.pos,
                site.radius.max(1.0),
                i as u32,
                SITE_CELL,
            );
        }
        index
    }

    /// Sitios cuyas celdas contienen el punto XZ (candidatos con solape real).
    #[inline]
    pub fn query(&self, x: f32, z: f32) -> impl Iterator<Item = &Site> {
        let key = (
            x.div_euclid(SITE_CELL as f32) as i32,
            z.div_euclid(SITE_CELL as f32) as i32,
        );
        self.buckets
            .get(&key)
            .into_iter()
            .flatten()
            .map(move |&i| &self.sites[i as usize])
    }
}

/// Shape de carving: cápsulas/esferas que restan o añaden densidad.
#[derive(Debug, Clone, Copy)]
pub(crate) enum CarveShape {
    /// Túnel de cueva. `entrance` activa la sección elíptica y el gate suave
    /// cerca de la superficie (boca de entrada).
    Tunnel {
        a: Vec3,
        b: Vec3,
        radius: f32,
        entrance: bool,
    },
    /// Sala subterránea (esfera con achatamiento vertical 1.25).
    Room { center: Vec3, radius: f32 },
    /// Cauce de río: cápsula XZ con superficie de referencia.
    River {
        a: Vec3,
        b: Vec3,
        width: f32,
        depth: f32,
        surface_y: f32,
    },
    /// Vaso de lago (cuenca paraboloidal XZ).
    Lake {
        center: Vec3,
        radius: f32,
        depth: f32,
    },
    /// Formación rocosa (bulto aditivo: overhangs de acantilados).
    Rock {
        center: Vec3,
        radius: f32,
        strength: f32,
    },
}

impl CarveShape {}

/// Índice espacial de carving con ruido de rugosidad de paredes de cueva.
#[derive(Debug)]
pub(crate) struct CarveIndex {
    shapes: Vec<CarveShape>,
    buckets: HashMap<(i32, i32), Vec<u32>>,
    /// Simplex 3D para rugosidad de paredes (solo ±18 % de la forma).
    cave_noise: crate::noise::Noise,
}

impl CarveIndex {
    /// Construye el índice a partir del plan (cuevas, ríos, lagos y roca de
    /// acantilados). Las alturas de anclaje usan el terreno **con plan** para
    /// que bocas, cauces y roca queden pegados a la superficie real.
    #[must_use]
    pub fn from_plan(
        plan: &FeaturePlan,
        terrain: &Terrain,
        cave_noise: crate::noise::Noise,
    ) -> Self {
        let height_at = |x: f32, z: f32| terrain.height_at(x, z);
        let mut shapes: Vec<CarveShape> = Vec::new();

        for system in &plan.caves {
            push_cave_system(&mut shapes, system);
        }
        for river in &plan.rivers {
            push_river(&mut shapes, river);
        }
        for lake in &plan.lakes {
            shapes.push(CarveShape::Lake {
                center: Vec3::new(lake.pos.x, lake.water_level, lake.pos.y),
                radius: lake.radius,
                depth: lake.depth,
            });
        }
        // Formaciones rocosas aditivas en los acantilados (overhangs).
        for site in plan.sites.iter().filter(|s| s.kind == FeatureKind::Cliff) {
            push_rock_formation(&mut shapes, site, &height_at);
        }

        let mut index = Self {
            shapes,
            buckets: HashMap::new(),
            cave_noise,
        };
        for (i, shape) in index.shapes.iter().enumerate() {
            insert_shape(&mut index.buckets, shape, i as u32);
        }
        index
    }

    /// Carve reductivo en el punto: **máximo** de las influencias (túneles,
    /// salas, ríos, lagos). `surface_h` es la altura del terreno con plan en
    /// `(x, z)`, necesaria para los gates de profundidad.
    pub fn carve_at(&self, x: f32, y: f32, z: f32, surface_h: f32) -> f32 {
        let key = (
            x.div_euclid(CARVE_CELL as f32) as i32,
            z.div_euclid(CARVE_CELL as f32) as i32,
        );
        let Some(indices) = self.buckets.get(&key) else {
            return 0.0;
        };
        let p = Vec3::new(x, y, z);
        let depth_below_surface = surface_h - y;
        let rough = 1.0 + self.cave_noise.get3(x, y, z) * 0.18;
        let mut carve = 0.0f32;

        for &i in indices {
            match &self.shapes[i as usize] {
                CarveShape::Tunnel {
                    a,
                    b,
                    radius,
                    entrance,
                } => {
                    let d = segment_distance3(p, *a, *b);
                    if d >= *radius {
                        continue;
                    }
                    let factor = if *entrance {
                        // Sección elíptica (aplastada verticalmente) en la boca.
                        let dy = (y - segment_point_y(p, *a, *b)) * 2.4;
                        horizontal_segment_distance(p, *a, *b).hypot(dy)
                    } else {
                        d
                    };
                    if factor >= *radius {
                        continue;
                    }
                    let tunnel_factor = (1.0 - factor / radius).powf(0.65);
                    let gate = if *entrance {
                        smoothstep(-0.2, 2.4, depth_below_surface)
                    } else {
                        smoothstep(2.5, 6.5, depth_below_surface)
                    };
                    // La fuerza escala con la profundidad (término clave de la
                    // referencia): garantiza que el túnel excave a cualquier
                    // profundidad y no solo cerca de la superficie.
                    let strength =
                        tunnel_factor * gate * (2.0 + depth_below_surface + radius * 2.5) * rough;
                    if strength > carve {
                        carve = strength;
                    }
                }
                CarveShape::Room { center, radius } => {
                    let dy = (y - center.y) * 1.25;
                    let d_ell = ((center.x - x).hypot(center.z - z)).hypot(dy.abs());
                    if d_ell >= *radius {
                        continue;
                    }
                    let factor = (1.0 - d_ell / radius).powf(0.55);
                    let gate = smoothstep(5.0, 10.0, depth_below_surface);
                    let strength =
                        factor * gate * 1.25 * (2.0 + depth_below_surface + radius * 2.5) * rough;
                    if strength > carve {
                        carve = strength;
                    }
                }
                CarveShape::River {
                    a,
                    b,
                    width,
                    depth,
                    surface_y,
                } => {
                    let d = horizontal_segment_distance(p, *a, *b);
                    if d >= *width {
                        continue;
                    }
                    let profile = (1.0 - d / width).max(0.0).sqrt();
                    // Solo talla cerca de la superficie del propio cauce.
                    let gate = 1.0 - smoothstep(surface_y - 0.5, surface_y + 0.6, y);
                    let strength = depth * profile * gate;
                    if strength > carve {
                        carve = strength;
                    }
                }
                CarveShape::Lake {
                    center,
                    radius,
                    depth,
                } => {
                    let d = (center.x - x).hypot(center.z - z);
                    if d >= *radius {
                        continue;
                    }
                    let t = 1.0 - (d / radius) * (d / radius);
                    let strength = depth * t;
                    if strength > carve {
                        carve = strength;
                    }
                }
                CarveShape::Rock { .. } => {}
            }
        }
        carve
    }

    /// Carve aditivo (formaciones rocosas): **suma** determinista en orden de
    /// inserción; los overhangs cuelgan de los acantilados.
    pub fn add_at(&self, x: f32, y: f32, z: f32) -> f32 {
        let key = (
            x.div_euclid(CARVE_CELL as f32) as i32,
            z.div_euclid(CARVE_CELL as f32) as i32,
        );
        let Some(indices) = self.buckets.get(&key) else {
            return 0.0;
        };
        let p = Vec3::new(x, y, z);
        let mut add = 0.0f32;
        for &i in indices {
            if let CarveShape::Rock {
                center,
                radius,
                strength,
            } = &self.shapes[i as usize]
            {
                let d = p.distance(*center);
                if d < *radius {
                    let t = 1.0 - d / radius;
                    add += strength * t * t;
                }
            }
        }
        add
    }
}

/// Inserta un túnel/rooms de un sistema de cuevas.
fn push_cave_system(shapes: &mut Vec<CarveShape>, system: &CaveSystem) {
    for segment in &system.segments {
        shapes.push(CarveShape::Tunnel {
            a: segment.a,
            b: segment.b,
            radius: segment.radius,
            entrance: segment.entrance,
        });
    }
    for room in &system.rooms {
        shapes.push(CarveShape::Room {
            center: room.center,
            radius: room.radius,
        });
    }
}

/// Inserta el cauce de un río como cápsulas consecutivas.
fn push_river(shapes: &mut Vec<CarveShape>, river: &RiverPath) {
    for window in river.points.windows(2) {
        let (a_xz, a_h) = window[0];
        let (b_xz, b_h) = window[1];
        shapes.push(CarveShape::River {
            a: Vec3::new(a_xz.x, a_h, a_xz.y),
            b: Vec3::new(b_xz.x, b_h, b_xz.y),
            width: river.width,
            depth: river.depth,
            surface_y: a_h.min(b_h),
        });
    }
}

/// Formaciones rocosas aditivas ancladas a la superficie local del acantilado.
fn push_rock_formation(
    shapes: &mut Vec<CarveShape>,
    site: &Site,
    height_at: &dyn Fn(f32, f32) -> f32,
) {
    let mut rng = crate::seed::Rng::from_u64(
        (site.pos.x.to_bits() as u64) << 32 | site.pos.y.to_bits() as u64,
    );
    let count = 3 + (rng.next_f32() * 3.0) as u32;
    for _ in 0..count {
        let angle = rng.range_f32(0.0, std::f32::consts::TAU);
        let dist = site.radius * rng.range_f32(0.3, 0.7);
        let x = site.pos.x + angle.cos() * dist;
        let z = site.pos.y + angle.sin() * dist;
        // Anclado a la superficie local (con plan), sobresaliendo 1–3 m.
        let y = height_at(x, z) + rng.range_f32(1.0, 3.0);
        shapes.push(CarveShape::Rock {
            center: Vec3::new(x, y, z),
            radius: rng.range_f32(2.0, 5.0),
            strength: site.value * 0.5,
        });
    }
}

/// Inserta un shape en todas las celdas XZ que solapa su AABB expandido.
fn insert_shape(buckets: &mut HashMap<(i32, i32), Vec<u32>>, shape: &CarveShape, index: u32) {
    let (center, reach) = match shape {
        CarveShape::Tunnel { a, b, radius, .. } => (
            Vec3::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5, (a.z + b.z) * 0.5),
            a.distance(*b) * 0.5 + radius,
        ),
        CarveShape::Room { center, radius }
        | CarveShape::Lake { center, radius, .. }
        | CarveShape::Rock { center, radius, .. } => (*center, *radius),
        CarveShape::River { a, b, width, .. } => {
            let reach = a.distance(*b) * 0.5 + width.max(4.0);
            (
                Vec3::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5, (a.z + b.z) * 0.5),
                reach,
            )
        }
    };
    insert_aabb(
        buckets,
        Vec2::new(center.x, center.z),
        reach.max(1.0),
        index,
        CARVE_CELL,
    );
}

/// Inserta un disco XZ de radio `reach` en las celdas que solapa su AABB.
fn insert_aabb(
    buckets: &mut HashMap<(i32, i32), Vec<u32>>,
    pos: Vec2,
    reach: f32,
    index: u32,
    cell: i32,
) {
    let cell_f = cell as f32;
    let min_x = ((pos.x - reach) / cell_f).floor() as i32;
    let max_x = ((pos.x + reach) / cell_f).floor() as i32;
    let min_z = ((pos.y - reach) / cell_f).floor() as i32;
    let max_z = ((pos.y + reach) / cell_f).floor() as i32;
    for cx in min_x..=max_x {
        for cz in min_z..=max_z {
            buckets.entry((cx, cz)).or_default().push(index);
        }
    }
}

/// Distancia 3D punto→segmento.
fn segment_distance3(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if len_sq < f32::EPSILON {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Distancia XZ punto→segmento (para cauces de río).
fn horizontal_segment_distance(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let pa = Vec2::new(p.x, p.z);
    let aa = Vec2::new(a.x, a.z);
    let bb = Vec2::new(b.x, b.z);
    let ab = bb - aa;
    let len_sq = ab.length_squared();
    if len_sq < f32::EPSILON {
        return pa.distance(aa);
    }
    let t = ((pa - aa).dot(ab) / len_sq).clamp(0.0, 1.0);
    pa.distance(aa + ab * t)
}

/// Altura Y del punto del segmento más cercano en XZ (para la elipse de la boca).
fn segment_point_y(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = Vec2::new(b.x - a.x, b.z - a.z);
    let len_sq = ab.length_squared();
    if len_sq < f32::EPSILON {
        return a.y;
    }
    let t = ((p.x - a.x) * ab.x + (p.z - a.z) * ab.y) / len_sq;
    a.y + (b.y - a.y) * t.clamp(0.0, 1.0)
}

/// smoothstep estándar con orden (e0, e1, x).
#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
