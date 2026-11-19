//! Planificador de features (F3/F4): convierte la config en una lista concreta y
//! determinista de sitios, cuevas y ríos que respetan **cuotas máximas**,
//! tamaños y separaciones.
//!
//! El plan es una función pura de `(seed, config)` y de la heightfield base
//! (para ríos descendentes y niveles de agua), por lo que es reproducible y
//! serializable para persistencia (F8).

use glam::{Vec2, Vec3};

use crate::config::WorldGenConfig;
use crate::field::{HeightField, LandGrade};
use crate::scale::{SITE_EDGE_MARGIN, fit_config_to_world, generation_density};
use crate::seed::{Rng, SALT_CAVES, SALT_PLANNER, SALT_RIVERS, domain_seed};

/// Clases de feature localizable con sitio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureKind {
    /// Montaña (máscara irregular; mesetas + crestas).
    Mountain,
    /// Colina (amplitud FBM local).
    Hill,
    /// Valle (depresión suave).
    Valley,
    /// Llanura (aplanamiento del microdetalle).
    Plain,
    /// Acantilado (terrazas).
    Cliff,
    /// Lago (cuenca con nivel de agua plano).
    Lake,
}

/// Sitio concreto de un feature en el mundo.
#[derive(Debug, Clone, Copy)]
pub struct Site {
    /// Clase del feature.
    pub kind: FeatureKind,
    /// Centro XZ del sitio (metros).
    pub pos: Vec2,
    /// Radio de influencia (metros).
    pub radius: f32,
    /// Valor principal: altura (montaña/colina), profundidad (valle/lago) o
    /// salto de terraza (acantilado).
    pub value: f32,
    /// Perfil de montaña: `0` = meseta, `1` = cresta afilada. Otros sitios 0.5.
    pub ridge: f32,
}

impl Site {
    /// Alcance XZ del sitio con elipse y costa irregular (AABB / índice).
    #[inline]
    #[must_use]
    pub fn footprint_reach(&self) -> f32 {
        self.radius * 1.55
    }

    /// Distancia normalizada al borde (`1` = orilla). Elipse + armónicos: no
    /// es un círculo. La máscara de ruido deforma el cupo del planificador.
    #[must_use]
    pub fn norm_dist(&self, x: f32, z: f32) -> f32 {
        let dx = x - self.pos.x;
        let dz = z - self.pos.y;
        let mut rng = footprint_rng(self);
        let yaw = rng.range_f32(0.0, std::f32::consts::TAU);
        let (s, c) = yaw.sin_cos();
        let aspect = rng.range_f32(0.52, 0.82);
        let rx = dx * c + dz * s;
        let rz = -dx * s + dz * c;
        let lx = rx;
        let lz = rz / aspect;
        let ang = lz.atan2(lx);
        let a1 = rng.range_f32(0.16, 0.34);
        let p1 = rng.range_f32(0.0, std::f32::consts::TAU);
        let a2 = rng.range_f32(0.08, 0.20);
        let p2 = rng.range_f32(0.0, std::f32::consts::TAU);
        let a3 = rng.range_f32(0.04, 0.12);
        let p3 = rng.range_f32(0.0, std::f32::consts::TAU);
        let shore = (1.0
            + a1 * (2.0 * ang + p1).sin()
            + a2 * (3.0 * ang + p2).sin()
            + a3 * (5.0 * ang + p3).sin())
        .clamp(0.55, 1.55);
        let radius = self.radius * shore;
        if radius <= 1.0e-4 {
            return f32::INFINITY;
        }
        lx.hypot(lz) / radius
    }
}

fn footprint_rng(site: &Site) -> Rng {
    Rng::from_u64(
        (site.pos.x.to_bits() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ (site.pos.y.to_bits() as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)
            ^ (site.radius.to_bits() as u64),
    )
}

/// Segmento de túnel de cueva (cápsula 3D `a → b`).
#[derive(Debug, Clone, Copy)]
pub struct TunnelSegment {
    /// Extremo inicial (metros).
    pub a: Vec3,
    /// Extremo final (metros).
    pub b: Vec3,
    /// Radio del túnel (metros).
    pub radius: f32,
    /// `true` en el tramo de entrada (boca en superficie, sección elíptica).
    pub entrance: bool,
}

/// Sala subterránea (esfera).
#[derive(Debug, Clone, Copy)]
pub struct CaveRoom {
    /// Centro (metros).
    pub center: Vec3,
    /// Radio (metros).
    pub radius: f32,
}

/// Sistema de cuevas: un túnel con entrada en superficie + rooms.
#[derive(Debug, Clone, Default)]
pub struct CaveSystem {
    /// Túneles del sistema.
    pub segments: Vec<TunnelSegment>,
    /// Salas del sistema.
    pub rooms: Vec<CaveRoom>,
    /// Profundidad objetivo bajo la superficie (metros).
    pub target_depth: f32,
}

/// Río: polilínea descendente con cauce y cascadas detectadas.
#[derive(Debug, Clone, Default)]
pub struct RiverPath {
    /// Puntos de la polilínea (x, z) con su altura de superficie.
    pub points: Vec<(Vec2, f32)>,
    /// Semiancho del cauce (metros).
    pub width: f32,
    /// Profundidad del cauce (metros).
    pub depth: f32,
    /// Posiciones de cascada detectadas (salto > umbral).
    pub waterfalls: Vec<Vec2>,
}

/// Lago con su nivel de agua (datos para las mallas de agua de F6).
#[derive(Debug, Clone, Copy)]
pub struct LakeInfo {
    /// Centro XZ.
    pub pos: Vec2,
    /// Radio de la cuenca (metros).
    pub radius: f32,
    /// Altura Y de la superficie del agua.
    pub water_level: f32,
    /// Profundidad máxima del vaso (metros).
    pub depth: f32,
}

/// Plan completo del mundo. Determinista dado `(seed, config)` y la heightfield
/// base; se genera una vez en `DensityField::new`.
#[derive(Debug, Clone, Default)]
pub struct FeaturePlan {
    /// Sitios de features de terreno.
    pub sites: Vec<Site>,
    /// Sistemas de cuevas.
    pub caves: Vec<CaveSystem>,
    /// Ríos.
    pub rivers: Vec<RiverPath>,
    /// Lagos con nivel de agua.
    pub lakes: Vec<LakeInfo>,
    /// Cascadas detectadas (de ríos).
    pub waterfalls: Vec<Vec2>,
}

/// Multiplicador de intentos por sitio aceptado (dart-throwing con rechazo).
const ATTEMPTS_PER_SITE: u32 = 12;
/// Margen mínimo (m) entre el borde de un lago y el pie de una montaña.
const LAKE_MOUNTAIN_GAP: f32 = 12.0;

impl FeaturePlan {
    /// Genera el plan completo de forma determinista.
    #[must_use]
    pub fn generate(config: &WorldGenConfig, base: &HeightField) -> Self {
        let mut fitted = config.clone();
        fit_config_to_world(&mut fitted);
        let config = &fitted;
        let mut rng = Rng::new(domain_seed(config.seed, SALT_PLANNER));
        let extent = Vec2::new(config.bounds.width, config.bounds.length);
        let grade = base.land_grade();
        let density = generation_density(&config.bounds);
        let mut plan = Self::default();

        let mountains = place_spread_sites(
            &config.mountains.base,
            density,
            &mut rng,
            |rng, r, pos| Site {
                kind: FeatureKind::Mountain,
                pos,
                radius: r,
                value: config.mountains.max_height * rng.range_f32(0.6, 1.0),
                ridge: rng.range_f32(0.0, 1.0),
            },
            |rng, index, sectors| rng_pos_along_half(rng, extent, grade, true, index, sectors),
        );
        plan.sites.extend(mountains);

        let hills = place_sites(
            &config.hills.base,
            density,
            &mut rng,
            &plan.sites,
            // Las colinas evitan solapar montañas (por distancia).
            |site, _, d| site.kind != FeatureKind::Mountain || d >= site.radius,
            |rng, r| Site {
                kind: FeatureKind::Hill,
                pos: rng_pos(rng, extent),
                radius: r,
                value: config.hills.max_height * rng.range_f32(0.5, 1.0),
                ridge: 0.5,
            },
        );
        plan.sites.extend(hills);

        let valleys = place_sites(
            &config.valleys.base,
            density,
            &mut rng,
            &plan.sites,
            |_, _, _| true,
            |rng, r| Site {
                kind: FeatureKind::Valley,
                pos: rng_pos(rng, extent),
                radius: r,
                value: config.valleys.max_depth * rng.range_f32(0.6, 1.0),
                ridge: 0.5,
            },
        );
        plan.sites.extend(valleys);

        let plains = place_sites(
            &config.plains.base,
            density,
            &mut rng,
            &plan.sites,
            |_, _, _| true,
            |rng, r| Site {
                kind: FeatureKind::Plain,
                pos: rng_pos(rng, extent),
                radius: r,
                value: config.plains.max_roughness,
                ridge: 0.5,
            },
        );
        plan.sites.extend(plains);

        let cliffs = place_sites(
            &config.cliffs.base,
            density,
            &mut rng,
            &plan.sites,
            |_, _, _| true,
            |rng, r| Site {
                kind: FeatureKind::Cliff,
                pos: rng_pos(rng, extent),
                radius: r,
                value: config.cliffs.drop_height,
                ridge: 0.5,
            },
        );
        plan.sites.extend(cliffs);

        // Orden canónico de los sitios terrestres ANTES de usarlos: la suma de
        // contribuciones de sitios solapados depende del orden en float, y el
        // plan, la función de altura del planner y el terreno final del campo
        // deben coincidir bit a bit.
        plan.sites.sort_by(|a, b| {
            a.pos
                .x
                .total_cmp(&b.pos.x)
                .then(a.pos.y.total_cmp(&b.pos.y))
        });

        // A partir de aquí, TODO lo que se ancla a la superficie usa la altura
        // **con plan** (sitios ya colocados): si un valle rebaja el terreno,
        // bocas de cueva, cauces y niveles de agua quedan pegados a él.
        let height_base = base.with_ceiling(config.bounds.height);
        let terrain_height = crate::field::plan_height_fn(plan.sites.clone(), &height_base);

        // Lagos DESPUÉS de fijar la función de altura: su nivel de agua se
        // calcula con la superficie **sin el propio vaso** (el lago llena la
        // cuenca hasta el nivel del terreno original). Se añaden al final del
        // canonical order (sin re-sort) para no alterar la suma en float.
        let lake_sectors = budget_sites(config.lakes.base.max_count, density).max(1);
        let mut lake_slot = 0u32;
        let lake_sites = place_sites(
            &config.lakes.base,
            density,
            &mut rng,
            &plan.sites,
            // Los lagos evitan montañas y colinas (cuencas en terreno bajo/medio).
            // Con la montaña cuenta el borde del vaso, no solo su centro: si no,
            // la cuenca se come la ladera.
            |site, candidate, d| match site.kind {
                FeatureKind::Mountain => d >= site.radius + candidate.radius + LAKE_MOUNTAIN_GAP,
                FeatureKind::Hill => d >= site.radius,
                _ => true,
            },
            |rng, r| {
                let pos = rng_pos_along_half(rng, extent, grade, false, lake_slot, lake_sectors);
                lake_slot = lake_slot.wrapping_add(1);
                Site {
                    kind: FeatureKind::Lake,
                    pos,
                    radius: r,
                    value: config.lakes.max_depth * rng.range_f32(0.78, 1.0),
                    ridge: 0.5,
                }
            },
        );
        for site in &lake_sites {
            let shore = terrain_height(site.pos.x, site.pos.y);
            // El vaso no puede bajar del piso, así que el agua tampoco: si no,
            // el nivel queda bajo el terreno recortado.
            let water_level = (shore - 0.5).max(crate::config::TERRAIN_FLOOR);
            plan.lakes.push(LakeInfo {
                pos: site.pos,
                radius: site.radius,
                water_level,
                depth: site.value,
            });
        }
        plan.sites.extend(lake_sites);

        // Ríos: nacen en montañas o en el borde del disco y bajan.
        if config.rivers.enabled {
            let mut river_rng = Rng::new(domain_seed(config.seed, SALT_RIVERS));
            let max_rivers = budget_sites(config.rivers.max_count, density);
            let attempts = max_rivers.saturating_mul(16).max(16);
            for _ in 0..attempts {
                if plan.rivers.len() as u32 >= max_rivers {
                    break;
                }
                let river = walk_river(
                    config,
                    &terrain_height,
                    &mut river_rng,
                    &plan.lakes,
                    &plan.sites,
                    grade,
                );
                if river.points.len() >= 8 {
                    plan.waterfalls.extend(river.waterfalls.iter().copied());
                    plan.rivers.push(river);
                }
            }
        }

        // Cuevas: túneles-gusano con entrada en superficie (F4).
        if config.caves.enabled {
            let mut cave_rng = Rng::new(domain_seed(config.seed, SALT_CAVES));
            let max_caves = budget_sites(config.caves.max_count, density);
            let attempts = max_caves.saturating_mul(8).max(8);
            for _ in 0..attempts {
                if plan.caves.len() as u32 >= max_caves {
                    break;
                }
                let Some(system) = walk_cave(
                    config,
                    &terrain_height,
                    &mut cave_rng,
                    &plan.lakes,
                    &plan.sites,
                    &plan.caves,
                    grade,
                ) else {
                    continue;
                };
                plan.caves.push(system);
            }
        }

        plan
    }
}

/// Coloca sitios repartidos (las montañas no se apilan).
fn place_spread_sites<F, P>(
    config: &crate::config::FeatureConfig,
    density: f32,
    rng: &mut Rng,
    make: F,
    sample_pos: P,
) -> Vec<Site>
where
    F: Fn(&mut Rng, f32, Vec2) -> Site,
    P: Fn(&mut Rng, u32, u32) -> Vec2,
{
    let mut placed: Vec<Site> = Vec::new();
    if !config.enabled || config.max_count == 0 {
        return placed;
    }
    let max_count = budget_sites(config.max_count, density);
    let sectors = max_count.max(1);
    for index in 0..sectors {
        if placed.len() as u32 >= max_count {
            break;
        }
        for _ in 0..ATTEMPTS_PER_SITE {
            let radius = rng.range_f32(config.min_size, config.max_size);
            let pos = sample_pos(rng, index, sectors);
            let candidate = make(rng, radius, pos);
            let min_same = config.min_separation.max((radius + radius) * 0.85);
            let ok_same = placed.iter().all(|s| {
                dist(s.pos, candidate.pos) >= min_same.max((s.radius + candidate.radius) * 0.85)
            });
            if ok_same {
                placed.push(candidate);
                break;
            }
        }
    }
    placed
}

/// Presupuesto de sitios para el área generada: conserva la densidad del mundo
/// visible aunque el plan cubra un rectángulo mayor.
#[inline]
fn budget_sites(max_count: u32, density: f32) -> u32 {
    if max_count == 0 {
        return 0;
    }
    ((max_count as f32) * density).round().max(1.0) as u32
}

/// Coloca sitios de una clase respetando `max_count` (límite duro), tamaños y
/// separación mínima entre ellos y con los sitios existentes filtrados por
/// `cross_ok`. Dart-throwing con rechazo: nunca excede el máximo.
fn place_sites<F, G>(
    config: &crate::config::FeatureConfig,
    density: f32,
    rng: &mut Rng,
    existing: &[Site],
    cross_ok: F,
    mut make: G,
) -> Vec<Site>
where
    F: Fn(&Site, &Site, f32) -> bool,
    G: FnMut(&mut Rng, f32) -> Site,
{
    let mut placed: Vec<Site> = Vec::new();
    if !config.enabled || config.max_count == 0 {
        return placed;
    }
    let max_count = budget_sites(config.max_count, density);
    let attempts = max_count.saturating_mul(ATTEMPTS_PER_SITE);
    for _ in 0..attempts {
        if placed.len() as u32 >= max_count {
            break;
        }
        let radius = rng.range_f32(config.min_size, config.max_size);
        let candidate = make(rng, radius);
        let min_same = config.min_separation.max((radius + radius) * 0.75);
        let ok_same = placed.iter().all(|s| {
            dist(s.pos, candidate.pos) >= min_same.max((s.radius + candidate.radius) * 0.75)
        });
        let ok_cross = existing
            .iter()
            .all(|s| cross_ok(s, &candidate, dist(s.pos, candidate.pos)));
        if ok_same && ok_cross {
            placed.push(candidate);
        }
    }
    placed
}

/// Distancia euclídea XZ.
#[inline]
fn dist(a: Vec2, b: Vec2) -> f32 {
    (a - b).length()
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Margen interior del rectángulo jugable (m).
fn site_margin(extent: Vec2) -> f32 {
    SITE_EDGE_MARGIN.min(extent.x * 0.25).min(extent.y * 0.25)
}

/// Posición aleatoria dentro del rectángulo jugable (sin margen de borde).
fn rng_pos(rng: &mut Rng, extent: Vec2) -> Vec2 {
    let margin = site_margin(extent);
    let min = Vec2::splat(margin);
    let max = extent - Vec2::splat(margin);
    Vec2::new(
        rng.range_f32(min.x, max.x.max(min.x)),
        rng.range_f32(min.y, max.y.max(min.y)),
    )
}

fn rng_pos_half(rng: &mut Rng, extent: Vec2, grade: LandGrade, high: bool) -> Vec2 {
    for _ in 0..16 {
        let pos = rng_pos(rng, extent);
        if grade.is_high_half(pos) == high {
            return pos;
        }
    }
    grade.mirror_to_half(rng_pos(rng, extent), high)
}

fn rng_pos_along_half(
    rng: &mut Rng,
    extent: Vec2,
    grade: LandGrade,
    high: bool,
    index: u32,
    sectors: u32,
) -> Vec2 {
    // Reparte por franjas perpendiculares al eje de la rampa y coloca cada sitio
    // en la mitad (alta o baja) solicitada, dentro del rectángulo.
    let dir = grade.high_dir();
    let grade_along_x = dir.x.abs() > dir.y.abs();
    let sectors = sectors.max(1);
    let slot = index % sectors;
    let margin = site_margin(extent);
    let (along_len, perp_len) = if grade_along_x {
        (extent.x, extent.y)
    } else {
        (extent.y, extent.x)
    };
    let perp_span = (perp_len - 2.0 * margin).max(0.01);
    let t = (slot as f32 + rng.range_f32(0.15, 0.85)) / sectors as f32;
    let perp_pos = margin + t.clamp(0.0, 1.0) * perp_span;

    let positive_high = if grade_along_x {
        dir.x > 0.0
    } else {
        dir.y > 0.0
    };
    let (lo, hi) = if high == positive_high {
        (along_len * 0.5, along_len - margin)
    } else {
        (margin, along_len * 0.5)
    };
    let along_pos = rng.range_f32(lo.min(hi).max(margin), hi.max(lo).max(margin));

    if grade_along_x {
        Vec2::new(along_pos, perp_pos)
    } else {
        Vec2::new(perp_pos, along_pos)
    }
}

fn world_radius_from_extent(extent: Vec2) -> f32 {
    extent.x.min(extent.y) * 0.5
}

/// Dentro del rectángulo jugable (con margen de borde).
fn inside_playable_disk(pos: Vec2, extent: Vec2) -> bool {
    let margin = site_margin(extent);
    pos.x >= margin && pos.x <= extent.x - margin && pos.y >= margin && pos.y <= extent.y - margin
}

/// Río que nace en una montaña o en el borde alto del disco, baja y **siempre**
/// desemboca en un lago o en el borde del mapa.
fn walk_river(
    config: &WorldGenConfig,
    height_at: &dyn Fn(f32, f32) -> f32,
    rng: &mut Rng,
    lakes: &[LakeInfo],
    sites: &[Site],
    grade: LandGrade,
) -> RiverPath {
    let extent = Vec2::new(config.bounds.width, config.bounds.length);
    let mut river = RiverPath {
        width: config.rivers.width,
        depth: config.rivers.depth,
        ..RiverPath::default()
    };
    let Some(mut pos) = pick_river_source(rng, extent, height_at, lakes, sites, grade) else {
        return river;
    };

    let grade_down = grade.low_dir();
    let mut dir = (grade_down + Vec2::from_angle(rng.range_f32(0.0, std::f32::consts::TAU)) * 0.18)
        .normalize_or_zero();
    let mut height = height_at(pos.x, pos.y);
    river.points.push((pos, height));

    let max_length = config.rivers.length_max;
    let mut traveled = 0.0f32;
    let step_min = 4.5;
    let step_max = 7.5;

    while traveled < max_length {
        if in_lake(pos, lakes, sites) && river.points.len() >= 2 {
            break;
        }
        if rim_clearance(pos, extent) <= 8.0 && river.points.len() >= 8 && grade.is_low_half(pos) {
            break;
        }
        let eps = 3.0;
        let gx = height_at(pos.x + eps, pos.y) - height_at(pos.x - eps, pos.y);
        let gz = height_at(pos.x, pos.y + eps) - height_at(pos.x, pos.y - eps);
        let downhill = Vec2::new(-gx, -gz);
        let mouth = mouth_dir(pos, extent, lakes, height_at, grade);
        let mut target = grade_down;
        if downhill.length_squared() > f32::EPSILON {
            target = (grade_down * 0.62 + downhill.normalize() * 0.38).normalize_or_zero();
        }
        let mouth_w = 0.22 + 0.5 * (traveled / max_length.max(1.0)).clamp(0.0, 1.0);
        if mouth.length_squared() > 0.0 {
            target = (target * (1.0 - mouth_w) + mouth * mouth_w).normalize_or_zero();
        }
        let tangent = Vec2::new(-target.y, target.x);
        dir = (dir * 0.16 + target * 0.84 + tangent * rng.gaussian(0.12)).normalize_or_zero();

        let step = rng.range_f32(step_min, step_max);
        let next = pos + dir * step;
        if !inside_playable_disk(next, extent) {
            let rim = clamp_to_rim(next, extent);
            river.points.push((rim, height_at(rim.x, rim.y)));
            break;
        }
        if in_lake(next, lakes, sites) {
            river.points.push((next, height_at(next.x, next.y)));
            break;
        }

        let next_height = height_at(next.x, next.y);
        if next_height > height + 0.85 {
            dir = if downhill.length_squared() > f32::EPSILON {
                (grade_down * 0.7 + downhill.normalize() * 0.3).normalize_or_zero()
            } else {
                grade_down
            };
            traveled += step * 0.2;
            continue;
        }
        let drop = height - next_height;
        if drop > config.waterfalls.min_drop {
            river.waterfalls.push(next);
        }

        pos = next;
        height = next_height;
        traveled += step;
        river.points.push((pos, height));
    }

    if !finish_river_to_mouth(
        &mut river,
        height_at,
        extent,
        lakes,
        sites,
        config.rivers.length_max * 0.9,
        grade,
    ) {
        river.points.clear();
        river.waterfalls.clear();
        return river;
    }
    let Some(&(end, end_h)) = river.points.last() else {
        return river;
    };
    let start = river.points[0].0;
    let start_h = river.points[0].1;
    let along_drop = grade.along(start) - grade.along(end);
    let ended_in_lake = in_lake(end, lakes, sites);
    let went_downslope = along_drop >= 0.04 || (ended_in_lake && along_drop >= -0.02);
    if !river_reaches_mouth(end, extent, lakes, sites) || end_h > start_h + 0.5 || !went_downslope {
        river.points.clear();
        river.waterfalls.clear();
    }
    river
}

fn in_lake(pos: Vec2, lakes: &[LakeInfo], sites: &[Site]) -> bool {
    lakes
        .iter()
        .any(|lake| dist(lake.pos, pos) < lake.radius * 0.82)
        || sites
            .iter()
            .filter(|site| site.kind == FeatureKind::Lake)
            .any(|site| site.norm_dist(pos.x, pos.y) < 0.92)
}

fn rim_clearance(pos: Vec2, extent: Vec2) -> f32 {
    let margin = site_margin(extent);
    (pos.x - margin)
        .min(extent.x - margin - pos.x)
        .min(pos.y - margin)
        .min(extent.y - margin - pos.y)
        .max(0.0)
}

fn river_reaches_mouth(pos: Vec2, extent: Vec2, lakes: &[LakeInfo], sites: &[Site]) -> bool {
    in_lake(pos, lakes, sites) || rim_clearance(pos, extent) <= 8.0
}

fn clamp_to_rim(pos: Vec2, extent: Vec2) -> Vec2 {
    let margin = site_margin(extent);
    Vec2::new(
        pos.x.clamp(margin, (extent.x - margin).max(margin)),
        pos.y.clamp(margin, (extent.y - margin).max(margin)),
    )
}

fn mouth_dir(
    pos: Vec2,
    extent: Vec2,
    lakes: &[LakeInfo],
    height_at: &dyn Fn(f32, f32) -> f32,
    grade: LandGrade,
) -> Vec2 {
    let here = height_at(pos.x, pos.y);
    let here_along = grade.along(pos);
    let rim = clamp_to_rim(pos + grade.low_dir() * 96.0, extent);
    let mut best = rim - pos;
    let mut best_score = best.length()
        + (height_at(rim.x, rim.y) - here).max(0.0) * 8.0
        + (grade.along(rim) - here_along).max(0.0) * 240.0;
    for lake in lakes {
        let to_lake = lake.pos - pos;
        let d = (to_lake.length() - lake.radius * 0.7).max(0.0);
        let climb = (lake.water_level - here).max(0.0);
        let uphill = (grade.along(lake.pos) - here_along).max(0.0);
        let score = d + climb * 8.0 + uphill * 280.0;
        if score < best_score {
            best_score = score;
            best = to_lake;
        }
    }
    best.normalize_or_zero()
}

fn finish_river_to_mouth(
    river: &mut RiverPath,
    height_at: &dyn Fn(f32, f32) -> f32,
    extent: Vec2,
    lakes: &[LakeInfo],
    sites: &[Site],
    extra: f32,
    grade: LandGrade,
) -> bool {
    let Some(&(mut pos, _)) = river.points.last() else {
        return false;
    };
    if river_reaches_mouth(pos, extent, lakes, sites) && river.points.len() >= 8 {
        return true;
    }
    let mut height = height_at(pos.x, pos.y);
    let mut traveled = 0.0;
    let step = 6.0;
    while traveled < extra.max(24.0) {
        let eps = 3.0;
        let gx = height_at(pos.x + eps, pos.y) - height_at(pos.x - eps, pos.y);
        let gz = height_at(pos.x, pos.y + eps) - height_at(pos.x, pos.y - eps);
        let downhill = Vec2::new(-gx, -gz);
        let mouth = mouth_dir(pos, extent, lakes, height_at, grade);
        let grade_down = grade.low_dir();
        let mut dir = if downhill.length_squared() > f32::EPSILON {
            (grade_down * 0.55 + downhill.normalize() * 0.45).normalize_or_zero()
        } else {
            grade_down
        };
        if mouth.length_squared() > 0.0 {
            dir = (dir * 0.32 + mouth * 0.68).normalize_or_zero();
        }
        if dir.length_squared() <= f32::EPSILON {
            break;
        }
        let next = pos + dir * step;
        let next_height = if !inside_playable_disk(next, extent) {
            let rim = clamp_to_rim(next, extent);
            let h = height_at(rim.x, rim.y);
            if h > height + 2.8 {
                break;
            }
            river.points.push((rim, h));
            return river.points.len() >= 8;
        } else {
            height_at(next.x, next.y)
        };
        if next_height > height + 2.8 {
            let along_mouth = pos + mouth * step;
            let mouth_h = height_at(along_mouth.x, along_mouth.y);
            if mouth.length_squared() > 0.0 && mouth_h <= height + 2.8 {
                pos = along_mouth;
                height = mouth_h;
                traveled += step;
                river.points.push((pos, height));
            } else {
                break;
            }
        } else if in_lake(next, lakes, sites) {
            river.points.push((next, next_height));
            return river.points.len() >= 8;
        } else {
            pos = next;
            height = next_height;
            traveled += step;
            river.points.push((pos, height));
        }
        if river_reaches_mouth(pos, extent, lakes, sites) {
            return river.points.len() >= 8;
        }
    }
    river_reaches_mouth(pos, extent, lakes, sites) && river.points.len() >= 8
}

/// Fuente: ladera de una montaña (drenaje) o el borde alto del disco.
fn pick_river_source(
    rng: &mut Rng,
    extent: Vec2,
    height_at: &dyn Fn(f32, f32) -> f32,
    lakes: &[LakeInfo],
    sites: &[Site],
    grade: LandGrade,
) -> Option<Vec2> {
    let mountains: Vec<&Site> = sites
        .iter()
        .filter(|s| s.kind == FeatureKind::Mountain)
        .collect();
    let in_lake = |p: Vec2| lakes.iter().any(|l| dist(l.pos, p) <= l.radius * 1.1);
    let floor = source_height_floor(lakes);

    if !mountains.is_empty() && rng.chance(0.82) {
        let idx = rng.range_i32(0, mountains.len() as i32 - 1) as usize;
        let mountain = mountains[idx];
        let mut best = None;
        let mut best_h = f32::MAX;
        for k in 0..12 {
            let ang = (k as f32 + rng.next_f32()) * (std::f32::consts::TAU / 12.0);
            let p = mountain.pos
                + Vec2::from_angle(ang) * (mountain.radius * rng.range_f32(0.48, 0.78));
            if !inside_playable_disk(p, extent) || in_lake(p) || !grade.is_high_half(p) {
                continue;
            }
            let h = height_at(p.x, p.y);
            if h < floor {
                continue;
            }
            if h < best_h {
                best_h = h;
                best = Some(p);
            }
        }
        if best.is_some() {
            return best;
        }
    }

    let center = extent * 0.5;
    let playable = (world_radius_from_extent(extent) - SITE_EDGE_MARGIN).max(8.0);
    let rim = (playable - 16.0).max(8.0);
    let mut best = None;
    let mut best_h = f32::MIN;
    for _ in 0..24 {
        let p = center + Vec2::from_angle(rng.range_f32(0.0, std::f32::consts::TAU)) * rim;
        if in_lake(p) || !grade.is_high_half(p) {
            continue;
        }
        let h = height_at(p.x, p.y);
        if h < floor {
            continue;
        }
        if h > best_h {
            best_h = h;
            best = Some(p);
        }
    }
    if best.is_some() {
        return best;
    }
    let fallback = center + grade.high_dir() * rim;
    (!in_lake(fallback)).then_some(fallback)
}

fn source_height_floor(lakes: &[LakeInfo]) -> f32 {
    if lakes.is_empty() {
        return crate::config::plains_height() + 2.0;
    }
    lakes
        .iter()
        .map(|lake| lake.water_level)
        .fold(f32::MAX, f32::min)
        + 4.0
}

/// Genera un sistema de cuevas: túnel con entrada en superficie + rooms (F4).
///
/// La boca nace en una zona alta. El descenso hace un lerp suave hacia el piso
/// (no un pozo); los primeros 7 tramos se tallan como entrada y el radio crece
/// hacia el interior.
/// Se corta antes de asomar por la ladera opuesta.
fn walk_cave(
    config: &WorldGenConfig,
    height_at: &dyn Fn(f32, f32) -> f32,
    rng: &mut Rng,
    lakes: &[LakeInfo],
    sites: &[Site],
    existing: &[CaveSystem],
    grade: LandGrade,
) -> Option<CaveSystem> {
    let caves = &config.caves;
    let extent = Vec2::new(config.bounds.width, config.bounds.length);

    let (mut pos, picked_host) = pick_cave_mouth(rng, extent, height_at, lakes, sites, grade)?;
    if !cave_mouth_clear(pos, existing) {
        return None;
    }
    let mouth_xz = pos;
    let mouth_surface = height_at(pos.x, pos.y);
    if mouth_surface <= crate::config::plains_height() + 6.0 {
        return None;
    }

    let host = picked_host.or_else(|| nearest_high_mass(pos, sites));
    let mut dir = cave_inward_dir(pos, host, height_at, rng);
    if dir.length_squared() < 1e-6 {
        dir = Vec2::from_angle(rng.range_f32(0.0, std::f32::consts::TAU));
    }

    let target_depth = rng.range_f32(caves.depth_min, caves.depth_max);
    let base_radius = caves.radius;
    let floor_y = crate::config::TERRAIN_FLOOR + base_radius + 0.8;
    let cave_floor_y = (mouth_surface - target_depth).max(floor_y);
    let mut system = CaveSystem {
        target_depth,
        ..CaveSystem::default()
    };

    let mut prev = Vec3::new(pos.x, mouth_surface - 0.3, pos.y);
    let mut y = prev.y;
    let mut segments = 0u32;
    let total = caves.segments.max(8);

    while segments < total {
        let opening = segments < 7;
        let progress = segments as f32 / (total - 1).max(1) as f32;
        let step = rng.range_f32(5.0, 8.5);
        dir = steer_cave_dir(dir, pos, host, rng, step);
        let next_xz = pos + dir * step;
        if !inside_playable_disk(next_xz, extent) {
            break;
        }
        if lakes
            .iter()
            .any(|lake| dist(lake.pos, next_xz) <= lake.radius * 1.05)
        {
            break;
        }
        if segments >= 3 && dist(next_xz, mouth_xz) < 18.0 {
            break;
        }
        if let Some(site) = host {
            let mouth_out = dist(mouth_xz, site.pos);
            let next_out = dist(next_xz, site.pos);
            if segments >= 4 && next_out > mouth_out + 1.5 {
                break;
            }
        }

        let local_h = height_at(next_xz.x, next_xz.y);
        if local_h < crate::config::plains_height() + 2.0 && segments >= 6 {
            break;
        }

        let radius = cave_segment_radius(base_radius, segments, total, rng);
        let ceiling = if opening {
            local_h - 0.4
        } else {
            local_h - radius - 1.8
        };
        let desired_y = lerp(
            mouth_surface - 0.3,
            cave_floor_y,
            crate::field::smoothstep(0.0, 0.65, progress),
        )
        .min(ceiling);
        let pull = if segments < 2 {
            0.12
        } else if opening {
            0.55
        } else {
            0.32
        };
        let hillside_drop = ceiling < y - 0.5;
        let max_pitch = if hillside_drop { 0.45 } else { 0.26 };
        let mut next_y = lerp(y, desired_y, pull).clamp(y - step * max_pitch, y);
        next_y = next_y.max(cave_floor_y);
        if next_y < floor_y {
            break;
        }
        if segments >= 3 && next_y > ceiling + 0.6 {
            break;
        }

        let next = Vec3::new(next_xz.x, next_y, next_xz.y);
        system.segments.push(TunnelSegment {
            a: prev,
            b: next,
            radius,
            entrance: opening,
        });

        if segments >= 8 && rng.chance(caves.room_chance) && local_h - next_y >= 8.0 {
            system.rooms.push(CaveRoom {
                center: next + Vec3::new(0.0, -0.5, 0.0),
                radius: rng.range_f32(2.5, 4.5),
            });
        }

        pos = next_xz;
        y = next_y;
        prev = next;
        segments += 1;
    }

    if system.segments.len() < 5 {
        return None;
    }
    Some(system)
}

/// Boca en ladera de montaña, colina o acantilado; si no hay, el punto más alto.
fn pick_cave_mouth(
    rng: &mut Rng,
    extent: Vec2,
    height_at: &dyn Fn(f32, f32) -> f32,
    lakes: &[LakeInfo],
    sites: &[Site],
    grade: LandGrade,
) -> Option<(Vec2, Option<Site>)> {
    let floor = crate::config::plains_height() + 6.0;
    let in_lake = |p: Vec2| {
        lakes
            .iter()
            .any(|lake| dist(lake.pos, p) <= lake.radius * 1.1)
    };
    let high: Vec<&Site> = sites
        .iter()
        .filter(|site| {
            matches!(
                site.kind,
                FeatureKind::Mountain | FeatureKind::Hill | FeatureKind::Cliff
            ) && grade.is_high_half(site.pos)
        })
        .collect();

    if !high.is_empty() {
        for _ in 0..10 {
            let idx = rng.range_i32(0, high.len() as i32 - 1) as usize;
            let site = high[idx];
            let p = site.pos
                + Vec2::from_angle(rng.range_f32(0.0, std::f32::consts::TAU))
                    * (site.radius * rng.range_f32(0.55, 0.92));
            if !inside_playable_disk(p, extent) || in_lake(p) || !grade.is_high_half(p) {
                continue;
            }
            if height_at(p.x, p.y) >= floor {
                return Some((p, Some(*site)));
            }
        }
    }

    let mut best = None;
    let mut best_h = f32::MIN;
    for _ in 0..18 {
        let p = rng_pos_half(rng, extent, grade, true);
        if in_lake(p) {
            continue;
        }
        let h = height_at(p.x, p.y);
        if h > best_h {
            best_h = h;
            best = Some(p);
        }
    }
    best.filter(|&p| best_h >= floor && grade.is_high_half(p))
        .map(|p| (p, nearest_high_mass(p, sites)))
}

fn nearest_high_mass(pos: Vec2, sites: &[Site]) -> Option<Site> {
    sites
        .iter()
        .filter(|site| {
            matches!(
                site.kind,
                FeatureKind::Mountain | FeatureKind::Hill | FeatureKind::Cliff
            )
        })
        .min_by(|a, b| dist(a.pos, pos).total_cmp(&dist(b.pos, pos)))
        .copied()
        .filter(|site| dist(site.pos, pos) <= site.radius * 1.3)
}

fn cave_inward_dir(
    pos: Vec2,
    host: Option<Site>,
    height_at: &dyn Fn(f32, f32) -> f32,
    rng: &mut Rng,
) -> Vec2 {
    if let Some(site) = host {
        let inward = site.pos - pos;
        if inward.length_squared() > 4.0 {
            return inward.normalize_or_zero();
        }
    }
    let e = 3.5;
    let gx = height_at(pos.x + e, pos.y) - height_at(pos.x - e, pos.y);
    let gz = height_at(pos.x, pos.y + e) - height_at(pos.x, pos.y - e);
    let uphill = Vec2::new(gx, gz).normalize_or_zero();
    if uphill.length_squared() > 1e-6 {
        uphill
    } else {
        Vec2::from_angle(rng.range_f32(0.0, std::f32::consts::TAU))
    }
}

fn cave_mouth_clear(pos: Vec2, existing: &[CaveSystem]) -> bool {
    existing.iter().all(|system| {
        let mouth = Vec2::new(system.segments[0].a.x, system.segments[0].a.z);
        dist(mouth, pos) >= 40.0
    })
}

fn steer_cave_dir(dir: Vec2, pos: Vec2, host: Option<Site>, rng: &mut Rng, step: f32) -> Vec2 {
    let jitter = Vec2::from_angle(rng.gaussian(0.22));
    let mut heading = (dir + jitter * 0.35).normalize_or_zero();
    let Some(site) = host else {
        return heading;
    };
    let to_core = site.pos - pos;
    let dist_core = to_core.length();
    let inward = to_core / dist_core.max(1e-4);
    if dist_core < site.radius * 0.22 {
        let tangent = Vec2::new(-inward.y, inward.x);
        return (heading * 0.25 + tangent * 0.75).normalize_or_zero();
    }
    heading = (heading * 0.5 + inward * 0.5).normalize_or_zero();
    let next = pos + heading * step;
    if dist(next, site.pos) > site.radius * 0.9 && dist(next, site.pos) > dist_core {
        return inward;
    }
    heading
}

/// Radio del tramo: boca transitable y ensancha hacia el interior (no al revés).
fn cave_segment_radius(base: f32, index: u32, total: u32, rng: &mut Rng) -> f32 {
    let progress = index as f32 / total.max(1) as f32;
    let mouth = (base * 1.5).max(4.6);
    let inner = (base * 2.5).max(mouth + 2.4);
    let wave = (progress * std::f32::consts::TAU * 2.0).sin() * 0.45;
    let mut radius = mouth + (inner - mouth) * crate::field::smoothstep(0.05, 0.8, progress) + wave;
    radius += rng.gaussian(0.18).clamp(-0.35, 0.75);
    if index > 8 && index.is_multiple_of(7) {
        radius += rng.range_f32(0.6, 1.4);
    }
    radius.clamp(mouth * 0.92, inner * 1.15)
}
