//! Límites del mundo circular: radio, cupos de features y biomas.
//!
//! El terreno se recorta a un disco. Las cuotas por defecto (15 montañas de
//! 180 m, 5 biomas a partes iguales) no caben en un radio pequeño y dejan el
//! mapa como un desierto con nieve suelta. Aquí se escala todo al radio real.
//!
//! Referencias: Whittaker / Neo Terrain (temp + humedad + lapse rate),
//! `primitive-voxels-godot` (salts por dominio) y el hilo de biomes de
//! VoxelLodTerrain (Zylann).

use crate::config::{
    BiomesConfig, FeatureConfig, TERRAIN_FLOOR, WorldBounds, WorldGenConfig, plains_height,
};

/// Radio mínimo del disco generado (metros). Coincide con un mundo jugable.
pub const MIN_WORLDGEN_RADIUS: f32 = 32.0;
/// Radio máximo del disco (diámetro 2000 m).
pub const MAX_WORLDGEN_RADIUS: f32 = 1000.0;
/// Radio por defecto: isla compacta para iterar.
pub const DEFAULT_WORLDGEN_RADIUS: f32 = 128.0;
/// Altura mínima del volumen (cuevas + relieve).
pub const MIN_WORLDGEN_HEIGHT: f32 = 48.0;
/// Altura máxima del volumen.
pub const MAX_WORLDGEN_HEIGHT: f32 = 256.0;
/// Margen al borde del disco para no plantar sitios en el recorte.
pub const SITE_EDGE_MARGIN: f32 = 24.0;

/// Radio del disco cubierto por el rectángulo de generación.
#[inline]
#[must_use]
pub fn world_radius(bounds: &WorldBounds) -> f32 {
    bounds.width.min(bounds.length) * 0.5
}

/// Altura del volumen a partir del radio (el usuario solo elige el radio).
#[must_use]
pub fn height_for_radius(radius: f32) -> f32 {
    (radius * 0.55).clamp(MIN_WORLDGEN_HEIGHT, MAX_WORLDGEN_HEIGHT)
}

/// Cuántos círculos de `spacing` caben en el disco (empaquetado ~45 %).
#[must_use]
pub fn max_sites_in_disk(radius: f32, spacing: f32) -> u32 {
    let spacing = spacing.max(8.0);
    let usable = (radius - SITE_EDGE_MARGIN).max(0.0);
    if usable < spacing * 0.35 {
        return u32::from(usable > 8.0);
    }
    let area = std::f32::consts::PI * usable * usable;
    let cell = std::f32::consts::PI * (spacing * 0.5) * (spacing * 0.5);
    ((area / cell) * 0.45).floor().max(0.0) as u32
}

/// Biomas que un radio puede hospedar sin fragmentar el clima.
///
/// Prioridad: pradera, bosque, nieve, desierto, volcánico.
#[must_use]
pub fn max_biome_slots(radius: f32) -> u32 {
    if radius < 80.0 {
        2
    } else if radius < 160.0 {
        3
    } else if radius < 280.0 {
        4
    } else {
        5
    }
}

/// Línea de nieve en metros: cimas, no una banda 2D a cualquier altura.
#[must_use]
pub fn snow_line_meters(config: &WorldGenConfig) -> f32 {
    let plains = plains_height();
    let relief = if config.mountains.base.enabled {
        mountain_relief(config)
    } else {
        (config.bounds.height - plains) * 0.55
    };
    plains + relief * 0.97
}

/// Altura de montaña que cabe bajo el techo del volumen.
#[must_use]
pub fn mountain_relief(config: &WorldGenConfig) -> f32 {
    let cap = (config.bounds.height - plains_height() - 12.0).max(8.0);
    config.mountains.max_height.max(8.0).min(cap)
}

/// Ajusta cotas, tamaños y biomas al radio del disco. Idempotente.
pub fn fit_config_to_world(config: &mut WorldGenConfig) {
    let radius = world_radius(&config.bounds).clamp(MIN_WORLDGEN_RADIUS, MAX_WORLDGEN_RADIUS);
    let diameter = radius * 2.0;
    config.bounds.width = diameter;
    config.bounds.length = diameter;
    config.bounds.height = config
        .bounds
        .height
        .clamp(MIN_WORLDGEN_HEIGHT, MAX_WORLDGEN_HEIGHT);

    fit_feature(&mut config.mountains.base, radius, 0.10, 0.22, 0.62);
    config.mountains.max_height = mountain_relief(config);
    config.mountains.sharpness = config.mountains.sharpness.clamp(0.8, 1.4);

    fit_feature(&mut config.hills.base, radius, 0.08, 0.22, 0.28);
    fit_feature(&mut config.valleys.base, radius, 0.12, 0.30, 0.36);
    fit_feature(&mut config.plains.base, radius, 0.16, 0.40, 0.32);
    fit_feature(&mut config.cliffs.base, radius, 0.08, 0.22, 0.30);
    fit_feature(&mut config.lakes.base, radius, 0.07, 0.20, 0.38);

    let river_span = (radius * 1.5).max(40.0);
    config.rivers.length_max = config.rivers.length_max.min(river_span).max(40.0);
    config.rivers.length_min = config
        .rivers
        .length_min
        .min(config.rivers.length_max * 0.45)
        .max(20.0);
    let river_cap = max_sites_in_disk(radius, radius * 0.55).max(1);
    config.rivers.max_count = config.rivers.max_count.min(river_cap);

    let cave_cap = ((radius / 22.0).floor() as u32).clamp(1, 16);
    config.caves.max_count = config.caves.max_count.min(cave_cap);
    config.caves.depth_max = config
        .caves
        .depth_max
        .min((config.bounds.height - TERRAIN_FLOOR - 16.0).max(8.0));
    config.caves.depth_min = config.caves.depth_min.min(config.caves.depth_max).max(4.0);

    let fall_cap = max_sites_in_disk(radius, radius * 0.7).max(1);
    config.waterfalls.max_count = config.waterfalls.max_count.min(fall_cap);

    config.biomes = clamp_biomes(config.biomes, radius);
}

fn fit_feature(
    feature: &mut FeatureConfig,
    radius: f32,
    min_frac: f32,
    max_frac: f32,
    sep_frac: f32,
) {
    let max_size = (radius * max_frac).max(10.0);
    let min_size = (radius * min_frac).min(max_size);
    feature.max_size = feature.max_size.min(max_size).max(min_size);
    feature.min_size = feature.min_size.clamp(min_size * 0.6, feature.max_size);
    let spacing = (radius * sep_frac).max(feature.min_size);
    feature.min_separation = spacing.min(radius.max(1.0));
    let cap = max_sites_in_disk(radius, feature.min_separation).max(1);
    if feature.enabled {
        feature.max_count = feature.max_count.min(cap);
    }
}

fn clamp_biomes(requested: BiomesConfig, radius: f32) -> BiomesConfig {
    let slots = max_biome_slots(radius) as usize;
    let order = [
        (requested.grassland, 0),
        (requested.forest, 1),
        (requested.snow, 2),
        (requested.desert, 3),
        (requested.volcanic, 4),
    ];
    let mut kept = 0usize;
    let mut flags = [false; 5];
    for (on, index) in order {
        if on && kept < slots {
            flags[index] = true;
            kept += 1;
        }
    }
    if kept == 0 {
        flags[0] = true;
    }
    BiomesConfig {
        grassland: flags[0],
        forest: flags[1],
        snow: flags[2],
        desert: flags[3],
        volcanic: flags[4],
        shares: requested.shares,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WorldGenConfig;

    #[test]
    fn small_world_cannot_host_fifty_mountains() {
        let mut config = WorldGenConfig::default();
        config.bounds.width = 64.0;
        config.bounds.length = 64.0;
        config.mountains.base.max_count = 50;
        config.mountains.base.max_size = 180.0;
        fit_config_to_world(&mut config);
        assert!(config.mountains.base.max_count <= 3);
        assert!(config.mountains.base.max_size < 64.0);
        assert!(world_radius(&config.bounds) <= MAX_WORLDGEN_RADIUS);
    }

    #[test]
    fn default_radius_keeps_a_handful_of_peaks() {
        let mut config = WorldGenConfig::default();
        fit_config_to_world(&mut config);
        let radius = world_radius(&config.bounds);
        assert!((radius - DEFAULT_WORLDGEN_RADIUS).abs() < 0.5);
        assert!(config.mountains.base.max_count <= 8);
        assert!(config.lakes.base.max_count <= 6);
        assert!(!config.biomes.volcanic);
        assert!(config.biomes.grassland);
        assert!(config.biomes.forest);
        assert!(config.biomes.snow);
    }

    #[test]
    fn biome_slots_grow_with_radius() {
        assert_eq!(max_biome_slots(50.0), 2);
        assert_eq!(max_biome_slots(128.0), 3);
        assert_eq!(max_biome_slots(200.0), 4);
        assert_eq!(max_biome_slots(400.0), 5);
    }

    #[test]
    fn kilometre_world_keeps_all_biome_slots() {
        let mut config = WorldGenConfig::default();
        config.bounds.width = 1000.0;
        config.bounds.length = 1000.0;
        fit_config_to_world(&mut config);
        assert!((world_radius(&config.bounds) - 500.0).abs() < 0.5);
        assert_eq!(max_biome_slots(500.0), 5);
        assert!(config.biomes.grassland);
        assert!(config.biomes.forest);
        assert!(config.biomes.snow);
        assert!(config.biomes.desert);
        assert!(config.biomes.volcanic);
    }

    #[test]
    fn sharpness_is_clamped_to_broad_ridges() {
        let mut config = WorldGenConfig::default();
        config.mountains.sharpness = 3.0;
        fit_config_to_world(&mut config);
        assert!(config.mountains.sharpness <= 1.4);
        assert!(config.mountains.sharpness >= 0.8);
    }

    #[test]
    fn fit_is_idempotent() {
        let mut config = WorldGenConfig::default();
        fit_config_to_world(&mut config);
        let first = config.clone();
        fit_config_to_world(&mut config);
        assert_eq!(config, first);
    }
}
