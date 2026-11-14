//! Planificador de features (F3/F4): convierte la config en una lista concreta y
//! determinista de sitios, cuevas y ríos que respetan **cuotas máximas**,
//! tamaños y separaciones.
//!
//! El plan es una función pura de `(seed, config)` y de la heightfield base
//! (para ríos descendentes y niveles de agua), por lo que es reproducible y
//! serializable para persistencia (F8).

use glam::{Vec2, Vec3};

use crate::config::WorldGenConfig;
use crate::field::HeightField;
use crate::seed::{Rng, SALT_CAVES, SALT_PLANNER, SALT_RIVERS, domain_seed};

/// Clases de feature localizable con sitio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureKind {
    /// Montaña (crestas ridged sobre el sitio).
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

/// Margen desde el borde del mundo para colocar sitios (metros).
const SITE_EDGE_MARGIN: f32 = 24.0;

/// Multiplicador de intentos por sitio aceptado (dart-throwing con rechazo).
const ATTEMPTS_PER_SITE: u32 = 12;

impl FeaturePlan {
    /// Genera el plan completo de forma determinista.
    #[must_use]
    pub fn generate(config: &WorldGenConfig, base: &HeightField) -> Self {
        let mut rng = Rng::new(domain_seed(config.seed, SALT_PLANNER));
        let extent = Vec2::new(config.bounds.width, config.bounds.length);
        let mut plan = Self::default();

        let mountains = place_sites(
            &config.mountains.base,
            &mut rng,
            &[],
            |_, _| true,
            |rng, r| Site {
                kind: FeatureKind::Mountain,
                pos: rng_pos(rng, extent),
                radius: r,
                value: config.mountains.max_height * rng.range_f32(0.6, 1.0),
            },
        );
        plan.sites.extend(mountains);

        let hills = place_sites(
            &config.hills.base,
            &mut rng,
            &plan.sites,
            // Las colinas evitan solapar montañas (por distancia).
            |site, d| site.kind != FeatureKind::Mountain || d >= site.radius,
            |rng, r| Site {
                kind: FeatureKind::Hill,
                pos: rng_pos(rng, extent),
                radius: r,
                value: config.hills.max_height * rng.range_f32(0.5, 1.0),
            },
        );
        plan.sites.extend(hills);

        let valleys = place_sites(
            &config.valleys.base,
            &mut rng,
            &plan.sites,
            |_, _| true,
            |rng, r| Site {
                kind: FeatureKind::Valley,
                pos: rng_pos(rng, extent),
                radius: r,
                value: config.valleys.max_depth * rng.range_f32(0.6, 1.0),
            },
        );
        plan.sites.extend(valleys);

        let plains = place_sites(
            &config.plains.base,
            &mut rng,
            &plan.sites,
            |_, _| true,
            |rng, r| Site {
                kind: FeatureKind::Plain,
                pos: rng_pos(rng, extent),
                radius: r,
                value: config.plains.max_roughness,
            },
        );
        plan.sites.extend(plains);

        let cliffs = place_sites(
            &config.cliffs.base,
            &mut rng,
            &plan.sites,
            |_, _| true,
            |rng, r| Site {
                kind: FeatureKind::Cliff,
                pos: rng_pos(rng, extent),
                radius: r,
                value: config.cliffs.drop_height,
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
        let terrain_height = crate::field::plan_height_fn(plan.sites.clone(), base);

        // Lagos DESPUÉS de fijar la función de altura: su nivel de agua se
        // calcula con la superficie **sin el propio vaso** (el lago llena la
        // cuenca hasta el nivel del terreno original). Se añaden al final del
        // canonical order (sin re-sort) para no alterar la suma en float.
        let lake_sites = place_sites(
            &config.lakes.base,
            &mut rng,
            &plan.sites,
            // Los lagos evitan montañas y colinas (cuencas en terreno bajo/medio).
            |site, d| {
                (site.kind != FeatureKind::Mountain && site.kind != FeatureKind::Hill)
                    || d >= site.radius
            },
            |rng, r| Site {
                kind: FeatureKind::Lake,
                pos: rng_pos(rng, extent),
                radius: r,
                value: config.lakes.max_depth * rng.range_f32(0.5, 1.0),
            },
        );
        for site in &lake_sites {
            let water_level = terrain_height(site.pos.x, site.pos.y) - 0.5;
            plan.lakes.push(LakeInfo {
                pos: site.pos,
                radius: site.radius,
                water_level,
                depth: site.value,
            });
        }
        plan.sites.extend(lake_sites);

        // Ríos: random-walk descendente sobre la superficie con plan.
        if config.rivers.enabled {
            let mut river_rng = Rng::new(domain_seed(config.seed, SALT_RIVERS));
            for _ in 0..config.rivers.max_count {
                let river = walk_river(config, &terrain_height, &mut river_rng, &plan.lakes);
                if river.points.len() >= 8 {
                    plan.waterfalls.extend(river.waterfalls.iter().copied());
                    plan.rivers.push(river);
                }
            }
        }

        // Cuevas: túneles-gusano con entrada en superficie (F4).
        if config.caves.enabled {
            let mut cave_rng = Rng::new(domain_seed(config.seed, SALT_CAVES));
            for _ in 0..config.caves.max_count {
                let Some(system) = walk_cave(config, &terrain_height, &mut cave_rng, &plan.lakes)
                else {
                    break;
                };
                plan.caves.push(system);
            }
        }

        plan
    }
}

/// Coloca sitios de una clase respetando `max_count` (límite duro), tamaños y
/// separación mínima entre ellos y con los sitios existentes filtrados por
/// `cross_ok`. Dart-throwing con rechazo: nunca excede el máximo.
fn place_sites<F, G>(
    config: &crate::config::FeatureConfig,
    rng: &mut Rng,
    existing: &[Site],
    cross_ok: F,
    make: G,
) -> Vec<Site>
where
    F: Fn(&Site, f32) -> bool,
    G: Fn(&mut Rng, f32) -> Site,
{
    let mut placed: Vec<Site> = Vec::new();
    if !config.enabled || config.max_count == 0 {
        return placed;
    }
    let attempts = config.max_count.saturating_mul(ATTEMPTS_PER_SITE);
    for _ in 0..attempts {
        if placed.len() as u32 >= config.max_count {
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
            .all(|s| cross_ok(s, dist(s.pos, candidate.pos)));
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

/// Posición aleatoria dentro de los límites con margen al borde.
fn rng_pos(rng: &mut Rng, extent: Vec2) -> Vec2 {
    Vec2::new(
        rng.range_f32(
            SITE_EDGE_MARGIN,
            (extent.x - SITE_EDGE_MARGIN).max(SITE_EDGE_MARGIN + 1.0),
        ),
        rng.range_f32(
            SITE_EDGE_MARGIN,
            (extent.y - SITE_EDGE_MARGIN).max(SITE_EDGE_MARGIN + 1.0),
        ),
    )
}

/// Random-walk descendente para un río sobre la heightfield base.
///
/// Sigue el gradiente de máxima pendiente con inercia y jitter; registra la
/// polilínea con alturas y detecta cascadas (saltos > `min_drop`).
fn walk_river(
    config: &WorldGenConfig,
    height_at: &dyn Fn(f32, f32) -> f32,
    rng: &mut Rng,
    lakes: &[LakeInfo],
) -> RiverPath {
    let extent = Vec2::new(config.bounds.width, config.bounds.length);
    let mut river = RiverPath {
        width: config.rivers.width,
        depth: config.rivers.depth,
        ..RiverPath::default()
    };
    let margin = SITE_EDGE_MARGIN;

    // Inicio: en la mitad alta de la world height para garantizar descenso.
    let mut pos = Vec2::new(
        rng.range_f32(margin, (extent.x - margin).max(margin + 1.0)),
        rng.range_f32(margin, (extent.y - margin).max(margin + 1.0)),
    );
    // Evita nacer dentro de un lago.
    for _ in 0..8 {
        if lakes.iter().all(|l| dist(l.pos, pos) > l.radius * 1.2) {
            break;
        }
        pos = rng_pos(rng, extent);
    }

    let mut dir = Vec2::from_angle(rng.range_f32(0.0, std::f32::consts::TAU));
    let mut height = height_at(pos.x, pos.y);
    river.points.push((pos, height));

    let max_length = config.rivers.length_max;
    let mut traveled = 0.0f32;
    let step_min = 8.0;
    let step_max = 14.0;

    while traveled < max_length {
        // Gradiente numérico de la altura (2 muestras ortogonales).
        let eps = 4.0;
        let gx = height_at(pos.x + eps, pos.y) - height_at(pos.x - eps, pos.y);
        let gz = height_at(pos.x, pos.y + eps) - height_at(pos.x, pos.y - eps);
        let downhill = Vec2::new(-gx, -gz);
        if downhill.length_squared() > f32::EPSILON {
            // Mezcla inercia + descenso + jitter.
            let target = downhill.normalize();
            dir = (dir * 0.55 + target * 0.45 + dir * rng.gaussian(0.18)).normalize_or_zero();
        } else {
            dir = (dir + dir * rng.gaussian(0.3)).normalize_or_zero();
        }

        let step = rng.range_f32(step_min, step_max);
        let next = pos + dir * step;
        // Fuera de límites: termina el río con margen.
        if next.x < margin
            || next.y < margin
            || next.x > extent.x - margin
            || next.y > extent.y - margin
        {
            break;
        }
        // Dentro de un lago: el río desemboca y termina.
        if lakes.iter().any(|l| dist(l.pos, next) < l.radius * 0.8) {
            river.points.push((next, height_at(next.x, next.y)));
            break;
        }

        let next_height = height_at(next.x, next.y);
        let drop = height - next_height;
        if drop > config.waterfalls.min_drop {
            river.waterfalls.push(next);
        }
        // Si el terreno sube de forma persistente, el río muere (cuencas cerradas).
        if drop < -step * 0.5 {
            break;
        }

        pos = next;
        height = next_height;
        traveled += step;
        river.points.push((pos, height));
    }

    // Longitud mínima: descarta ríos demasiado cortos.
    if traveled < config.rivers.length_min {
        river.points.clear();
    }
    river
}

/// Genera un sistema de cuevas: túnel con entrada en superficie + rooms (F4).
///
/// Replica el enfoque validado de la referencia: descenso suave hasta la
/// profundidad objetivo, radio variable con onda + jitter, primeros tramos más
/// anchos como embudo de entrada y rooms por probabilidad a partir del tramo 8.
fn walk_cave(
    config: &WorldGenConfig,
    height_at: &dyn Fn(f32, f32) -> f32,
    rng: &mut Rng,
    lakes: &[LakeInfo],
) -> Option<CaveSystem> {
    let caves = &config.caves;
    let extent = Vec2::new(config.bounds.width, config.bounds.length);

    // Boca en superficie, lejos de lagos.
    let mut pos = rng_pos(rng, extent);
    for _ in 0..8 {
        if lakes.iter().all(|l| dist(l.pos, pos) > l.radius * 1.1) {
            break;
        }
        pos = rng_pos(rng, extent);
    }
    let surface = height_at(pos.x, pos.y);
    if surface < 6.0 {
        return None;
    }

    let target_depth = rng.range_f32(caves.depth_min, caves.depth_max);
    let base_radius = caves.radius;
    let mut system = CaveSystem {
        target_depth,
        ..CaveSystem::default()
    };

    let mut dir = Vec2::from_angle(rng.range_f32(0.0, std::f32::consts::TAU));
    let mut prev = Vec3::new(pos.x, surface - 0.3, pos.y);
    let mut segments = 0u32;
    let total = caves.segments.max(2);

    while segments < total {
        let progress = segments as f32 / (total - 1) as f32;
        // Descenso suave: smoothstep(0..0.65) hasta la profundidad objetivo.
        let descend = smoothstep01((progress / 0.65).min(1.0)) * target_depth;
        let next_y = (surface - descend) + rng.gaussian(0.5);

        // Dirección con jitter (±0.25 rad aprox).
        dir = (dir + Vec2::from_angle(rng.gaussian(0.25))).normalize_or_zero();
        let step = rng.range_f32(5.0, 8.5);
        let next_xz = pos + dir * step;
        let next = Vec3::new(next_xz.x, next_y, next_xz.y);

        // Radio variable: onda sinusoidal + jitter; tramos de entrada más anchos.
        let wave = (segments as f32 * 0.7).sin() * 0.3;
        let mut radius = base_radius * (1.0 + wave + rng.gaussian(0.12));
        if segments < 5 {
            radius += 1.4;
        } else if segments.is_multiple_of(7) {
            radius += 0.8;
        }
        radius = radius.clamp(base_radius * 0.6, base_radius * 1.5);

        system.segments.push(TunnelSegment {
            a: prev,
            b: next,
            radius,
            entrance: segments < 7,
        });

        // Rooms a partir del tramo 8.
        if segments >= 8 && rng.chance(caves.room_chance) {
            system.rooms.push(CaveRoom {
                center: next + Vec3::new(0.0, -0.5, 0.0),
                radius: rng.range_f32(2.5, 4.5),
            });
        }

        pos = next_xz;
        prev = next;
        segments += 1;
    }

    Some(system)
}

/// smoothstep(0, 1, t).
#[inline]
fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
