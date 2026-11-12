//! Distribución de assets sobre el terreno (F7): convierte reglas declarativas
//! por blueprint en colocaciones concretas y deterministas.
//!
//! Reglas respetadas (nunca violadas): biomas compatibles, rango de altitud,
//! pendiente máxima, cantidad máxima de instancias, densidad, escala mín/máx,
//! rotación aleatoria y distancia mínima entre objetos. El agrupamiento opcional
//! concentra instancias alrededor de centros de clúster.
//!
//! La colocación final la hace el engine vía `spawn_cached_model_part_at` con el
//! `blueprint_id`: el plugin solo produce datos espaciales, jamás carga modelos.

use std::collections::HashMap;

use glam::Vec2;

use serde::{Deserialize, Serialize};

use crate::biomes::{Biome, BiomeMap};
use crate::config::WorldGenConfig;
use crate::field::DensityField;
use crate::seed::{Rng, domain_seed, mix_u32};

/// Reglas de distribución para un blueprint (JSON camelCase, espejo del editor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScatterRules {
    /// Biomas compatibles por nombre canónico (`grassland`, `forest`, `desert`,
    /// `snow`, `volcanic`). Vacío = todos.
    pub biomes: Vec<String>,
    /// Altitud mínima permitida (metros).
    pub altitude_min: f32,
    /// Altitud máxima permitida (metros).
    pub altitude_max: f32,
    /// Pendiente máxima permitida (grados).
    pub max_slope_deg: f32,
    /// Cantidad máxima de instancias (límite duro).
    pub max_instances: u32,
    /// Densidad relativa `[0, 1]`: fracción de celdas válidas que se rellenan.
    pub density: f32,
    /// Escala mínima (uniforme).
    pub scale_min: f32,
    /// Escala máxima (uniforme).
    pub scale_max: f32,
    /// Rotación yaw aleatoria.
    pub random_rotation: bool,
    /// Distancia mínima entre objetos del mismo blueprint (metros).
    pub min_distance: f32,
    /// Agrupación opcional en clústeres.
    pub cluster: ScatterCluster,
}

impl Default for ScatterRules {
    fn default() -> Self {
        Self {
            biomes: Vec::new(),
            altitude_min: 0.0,
            altitude_max: f32::MAX,
            max_slope_deg: 35.0,
            max_instances: 100,
            density: 0.5,
            scale_min: 1.0,
            scale_max: 1.0,
            random_rotation: true,
            min_distance: 4.0,
            cluster: ScatterCluster::default(),
        }
    }
}

/// Reglas de agrupación.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScatterCluster {
    /// Agrupar en clústeres.
    pub enabled: bool,
    /// Instancias objetivo por clúster.
    pub size: u32,
    /// Radio de cada clúster (metros); 0 = derivado de `min_distance * size`.
    pub radius: f32,
}

/// Colocación concreta de una instancia.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Posición apoyada en la superficie (metros).
    pub position: [f32; 3],
    /// Rotación yaw (radianes).
    pub yaw: f32,
    /// Escala uniforme.
    pub scale: [f32; 3],
}

/// Salt de dominio del scatter, mezclado con el hash del blueprint para
/// descorrelacionar blueprints distintos con las mismas reglas.
pub const SALT_SCATTER: u32 = 10_000;

/// Intentos de colocación por instancia (dart-throwing con rechazo).
const ATTEMPTS_PER_INSTANCE: u32 = 24;

/// Genera las colocaciones de un blueprint según las reglas.
///
/// `blueprint_hash`: hash determinista del `blueprint_id` (FNV-1a; ver
/// [`blueprint_hash`]) para que cada blueprint tenga su propia secuencia.
#[must_use]
pub fn scatter(
    field: &DensityField,
    config: &WorldGenConfig,
    rules: &ScatterRules,
    blueprint_hash: u64,
) -> Vec<Placement> {
    let mut out = Vec::new();
    if rules.max_instances == 0 {
        return out;
    }
    let extent = Vec2::new(config.bounds.width, config.bounds.length);
    let rng_seed = domain_seed(
        config.seed,
        SALT_SCATTER.wrapping_add(mix_u32(blueprint_hash as u32)),
    );
    let mut rng = Rng::new(rng_seed ^ (blueprint_hash >> 32) as i32);

    let min_distance = rules.min_distance.max(0.01);
    let mut grid: HashMap<(i32, i32), Vec<Vec2>> = HashMap::new();
    let attempts = rules.max_instances.saturating_mul(ATTEMPTS_PER_INSTANCE);

    // Centros de clúster si el agrupamiento está activo: dart-throwing con
    // separación amplia para que los clústeres no se fusionen del todo.
    let cluster_size = rules.cluster.size.max(1);
    let cluster_radius = if rules.cluster.radius > 0.0 {
        rules.cluster.radius
    } else {
        (min_distance * cluster_size as f32 * 0.5).max(min_distance * 2.0)
    };
    let cluster_count = if rules.cluster.enabled {
        ((rules.max_instances as f32 / cluster_size as f32)
            .ceil()
            .max(1.0)) as usize
    } else {
        0
    };
    let mut cluster_centers: Vec<Vec2> = Vec::new();
    for _ in 0..cluster_count.saturating_mul(12) {
        if cluster_centers.len() >= cluster_count {
            break;
        }
        let pos = Vec2::new(rng.range_f32(0.0, extent.x), rng.range_f32(0.0, extent.y));
        if cluster_centers
            .iter()
            .all(|c| (*c - pos).length() >= cluster_radius * 2.0)
        {
            cluster_centers.push(pos);
        }
    }

    let mut biome_cache: Option<BiomeMap> = None;
    for _ in 0..attempts {
        if out.len() as u32 >= rules.max_instances {
            break;
        }

        // Posición candidata: alrededor de un clúster o uniforme en el mundo.
        let (x, z) = if !cluster_centers.is_empty() {
            let index = ((rng.next_f32() * cluster_centers.len() as f32) as usize)
                .min(cluster_centers.len() - 1);
            let center = cluster_centers[index];
            let offset = Vec2::from_angle(rng.range_f32(0.0, std::f32::consts::TAU))
                * rng.gaussian(cluster_radius * 0.5);
            (center.x + offset.x, center.y + offset.y)
        } else {
            (rng.range_f32(0.0, extent.x), rng.range_f32(0.0, extent.y))
        };

        // Altura y validaciones de terreno.
        let y = field.height_at(x, z);
        if !(rules.altitude_min..=rules.altitude_max).contains(&y) {
            continue;
        }
        // Pendiente por gradiente numérico de la altura con plan.
        let eps = 2.0;
        let gx = (field.height_at(x + eps, z) - field.height_at(x - eps, z)) / (2.0 * eps);
        let gz = (field.height_at(x, z + eps) - field.height_at(x, z - eps)) / (2.0 * eps);
        let slope_deg = gx.hypot(gz).atan().to_degrees();
        if slope_deg > rules.max_slope_deg {
            continue;
        }
        // Bioma compatible.
        let biome = biome_cache
            .get_or_insert_with(|| BiomeMap::new(config))
            .biome_at(x, z, y);
        if !rules.biomes.is_empty() && !rules.biomes.iter().any(|name| name == biome_name(biome)) {
            continue;
        }
        // Densidad: probabilidad de aceptar la celda.
        if !rng.chance(rules.density.clamp(0.0, 1.0)) {
            continue;
        }
        // Distancia mínima (grid hash 3×3).
        let pos = Vec2::new(x, z);
        if !far_enough(&grid, pos, min_distance) {
            continue;
        }

        let scale = rng.range_f32(rules.scale_min, rules.scale_max).max(0.01);
        let yaw = if rules.random_rotation {
            rng.range_f32(0.0, std::f32::consts::TAU)
        } else {
            0.0
        };
        grid.entry((
            x.div_euclid(min_distance) as i32,
            z.div_euclid(min_distance) as i32,
        ))
        .or_default()
        .push(pos);
        out.push(Placement {
            position: [x, y, z],
            yaw,
            scale: [scale, scale, scale],
        });
    }
    out
}

/// Hash determinista de un `blueprint_id` (FNV-1a sobre bytes UTF-8).
#[must_use]
pub fn blueprint_hash(blueprint_id: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in blueprint_id.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Verifica la distancia mínima contra los vecinos en el grid hash.
fn far_enough(grid: &HashMap<(i32, i32), Vec<Vec2>>, pos: Vec2, min_distance: f32) -> bool {
    let (cx, cz) = (
        pos.x.div_euclid(min_distance) as i32,
        pos.y.div_euclid(min_distance) as i32,
    );
    for dx in -1..=1 {
        for dz in -1..=1 {
            if let Some(points) = grid.get(&(cx + dx, cz + dz)) {
                for p in points {
                    if (*p - pos).length() < min_distance {
                        return false;
                    }
                }
            }
        }
    }
    true
}

/// Nombre canónico del bioma (el que usan las reglas del editor).
#[must_use]
pub fn biome_name(biome: Biome) -> &'static str {
    match biome {
        Biome::Grassland => "grassland",
        Biome::Forest => "forest",
        Biome::Desert => "desert",
        Biome::Snow => "snow",
        Biome::Volcanic => "volcanic",
    }
}
