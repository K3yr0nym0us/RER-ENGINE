//! Límites del mundo circular: radio, cupos de features y biomas.
//!
//! El terreno se recorta a un disco. Las cuotas por defecto (15 montañas de
//! 180 m, 5 biomas a partes iguales) no caben en un radio pequeño y dejan el
//! mapa como un desierto con nieve suelta. Aquí se escala todo al radio real.
//!
//! El modelo de biomas usa temperatura, humedad y lapse rate, con salts por
//! dominio y una cuadrícula de chunks por capas Y. Créditos y referencias de
//! diseño en `THIRD_PARTY_NOTICES.md` en la raíz del repositorio.

use crate::config::{
    BiomesConfig, FeatureConfig, TERRAIN_FLOOR, WorldBounds, WorldGenConfig, plains_height,
};

/// Radio mínimo del disco generado (metros). Coincide con un mundo jugable.
///
/// Espejo de `MIN_WORLD_FACE_3D` / `MIN_WORLD_RADIUS_3D` del motor
/// (`engine/src/engine3d/config_3d/world_bounds.rs`). El motor no depende de
/// este crate (el plugin puede no estar instalado), así que ambos lugares
/// mantienen el valor duplicado a propósito con esta referencia cruzada.
pub const MIN_WORLDGEN_RADIUS: f32 = 32.0;
/// Radio máximo del disco (diámetro 2000 m). Espejo de `MAX_WORLD_FACE_3D`.
pub const MAX_WORLDGEN_RADIUS: f32 = 1000.0;
/// Radio por defecto: isla compacta para iterar.
pub const DEFAULT_WORLDGEN_RADIUS: f32 = 128.0;
/// Altura mínima del volumen (cuevas + relieve). Espejo de `MIN_WORLD_HEIGHT_3D`.
pub const MIN_WORLDGEN_HEIGHT: f32 = 48.0;
/// Altura máxima del volumen. Espejo de `MAX_WORLD_HEIGHT_3D`.
pub const MAX_WORLDGEN_HEIGHT: f32 = 256.0;
/// Margen al borde del disco para no plantar sitios en el recorte.
pub const SITE_EDGE_MARGIN: f32 = 24.0;

/// Radio del **mundo visible**. Si el área generada es mayor, el grade y los
/// rasgos se escalan a este radio.
#[inline]
#[must_use]
pub fn world_radius(bounds: &WorldBounds) -> f32 {
    let width = if bounds.visible_width > 0.0 {
        bounds.visible_width
    } else {
        bounds.width
    };
    let length = if bounds.visible_length > 0.0 {
        bounds.visible_length
    } else {
        bounds.length
    };
    width.min(length) * 0.5
}

/// Radio del área generada (puede incluir margen de crecimiento).
#[inline]
#[must_use]
pub fn generation_radius(bounds: &WorldBounds) -> f32 {
    bounds.width.min(bounds.length) * 0.5
}

/// Factor de densidad de sitios: el plan cubre el área generada (que puede ser
/// mayor que el mundo visible) y necesita más sitios para conservar la misma
/// densidad dentro del mundo jugable.
#[inline]
#[must_use]
pub fn generation_density(bounds: &WorldBounds) -> f32 {
    let visible = world_radius(bounds).max(1.0);
    let area = generation_radius(bounds).max(visible);
    (area / visible).powi(2).max(1.0)
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

/// Amplitud de la rampa actual (mínimo). Es la que deja los mundos pequeños
/// suaves, sin una ladera base exagerada.
#[must_use]
fn current_grade_amplitude(radius: f32) -> f32 {
    (radius * 0.12).clamp(4.0, 56.0)
}

/// Amplitud anterior (máximo). Tenía mucha altura base; se reserva para los
/// mundos grandes, donde el desnivel absoluto debe notarse más.
#[must_use]
fn previous_grade_amplitude(radius: f32) -> f32 {
    (radius * 0.16).clamp(28.0, 64.0)
}

/// Desnivel de la rampa cardinal (metros de lado a lado), calculado por tamaño:
/// los mundos pequeños se acercan al mínimo actual y los grandes al máximo
/// anterior. Así el desnivel crece con el mundo en vez de ser fijo.
#[must_use]
pub fn land_grade_amplitude(radius: f32) -> f32 {
    let radius = radius.max(MIN_WORLDGEN_RADIUS);
    let t = ((radius - MIN_WORLDGEN_RADIUS) / (MAX_WORLDGEN_RADIUS - MIN_WORLDGEN_RADIUS))
        .clamp(0.0, 1.0);
    let low = current_grade_amplitude(radius);
    let high = previous_grade_amplitude(radius);
    low + (high - low) * t
}

/// Altura de montaña **sobre el terreno local** (la rampa ya ocupó `grade` metros).
#[must_use]
pub fn mountain_relief(config: &WorldGenConfig) -> f32 {
    let grade = land_grade_amplitude(world_radius(&config.bounds));
    let cap = (config.bounds.height - plains_height() - grade - 12.0).max(8.0);
    config.mountains.max_height.max(8.0).min(cap)
}

/// Ajusta cotas, tamaños y biomas al rectángulo de generación. Idempotente.
pub fn fit_config_to_world(config: &mut WorldGenConfig) {
    // Rectángulo explícito: no se fuerza cuadrado (caras N/S/E/O independientes).
    config.bounds.width = config.bounds.width.clamp(64.0, 2000.0);
    config.bounds.length = config.bounds.length.clamp(64.0, 2000.0);
    let radius = world_radius(&config.bounds).clamp(MIN_WORLDGEN_RADIUS, MAX_WORLDGEN_RADIUS);
    let area_radius =
        generation_radius(&config.bounds).clamp(MIN_WORLDGEN_RADIUS, MAX_WORLDGEN_RADIUS);
    config.bounds.height = config
        .bounds
        .height
        .clamp(MIN_WORLDGEN_HEIGHT, MAX_WORLDGEN_HEIGHT);

    fit_feature(
        &mut config.hills.base,
        radius,
        area_radius,
        0.04,
        0.09,
        0.24,
    );
    config.hills.max_height = config
        .hills
        .max_height
        .clamp(6.0, (radius * 0.04).clamp(8.0, 16.0));

    let grade = land_grade_amplitude(radius);
    let mountain_want = config
        .mountains
        .max_height
        .max(8.0)
        .max(config.hills.max_height * 3.0);
    let need = plains_height() + grade + mountain_want + 18.0;
    config.bounds.height = config
        .bounds
        .height
        .max(need)
        .clamp(MIN_WORLDGEN_HEIGHT, MAX_WORLDGEN_HEIGHT);

    fit_feature(
        &mut config.mountains.base,
        radius,
        area_radius,
        0.16,
        0.32,
        0.58,
    );
    let hill_span = config.hills.base.max_size.max(1.0);
    let want_span = (hill_span * 3.0).min(radius * 0.45);
    config.mountains.base.max_size = config.mountains.base.max_size.max(want_span);
    config.mountains.base.min_size = (config.hills.base.min_size * 3.0)
        .min(config.mountains.base.max_size * 0.75)
        .max(10.0);
    config.mountains.max_height = config
        .mountains
        .max_height
        .max(config.hills.max_height * 3.0);
    config.mountains.max_height = mountain_relief(config);
    config.mountains.sharpness = config.mountains.sharpness.clamp(0.8, 1.4);
    fit_feature(
        &mut config.valleys.base,
        radius,
        area_radius,
        0.12,
        0.30,
        0.36,
    );
    fit_feature(
        &mut config.plains.base,
        radius,
        area_radius,
        0.16,
        0.40,
        0.32,
    );
    fit_feature(
        &mut config.cliffs.base,
        radius,
        area_radius,
        0.08,
        0.22,
        0.30,
    );
    // sep > 2*max_frac: si no, dos lagos grandes se tocan o se solapan.
    fit_feature(
        &mut config.lakes.base,
        radius,
        area_radius,
        0.07,
        0.18,
        0.58,
    );

    let river_span = (radius * 1.5).max(40.0);
    config.rivers.length_max = config.rivers.length_max.min(river_span).max(40.0);
    config.rivers.length_min = config
        .rivers
        .length_min
        .min(config.rivers.length_max * 0.45)
        .max(20.0);
    let river_cap = max_sites_in_disk(area_radius, radius * 0.55).max(1);
    config.rivers.max_count = config.rivers.max_count.min(river_cap);

    let cave_cap = ((area_radius / 22.0).floor() as u32).clamp(1, 16);
    config.caves.max_count = config.caves.max_count.min(cave_cap);
    config.caves.depth_max = config
        .caves
        .depth_max
        .min((config.bounds.height - TERRAIN_FLOOR - 16.0).max(8.0));
    config.caves.depth_min = config.caves.depth_min.min(config.caves.depth_max).max(4.0);

    let fall_cap = max_sites_in_disk(area_radius, radius * 0.7).max(1);
    config.waterfalls.max_count = config.waterfalls.max_count.min(fall_cap);

    config.biomes = clamp_biomes(config.biomes, radius);
}

fn fit_feature(
    feature: &mut FeatureConfig,
    radius: f32,
    area_radius: f32,
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
    let cap = max_sites_in_disk(area_radius, feature.min_separation).max(1);
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
    fn mountains_are_at_least_three_times_the_hills() {
        for side in [256.0, 512.0, 1000.0] {
            let mut config = WorldGenConfig::default();
            config.bounds.width = side;
            config.bounds.length = side;
            config.bounds.height = 128.0;
            fit_config_to_world(&mut config);
            assert!(
                config.mountains.base.max_size + 0.5 >= config.hills.base.max_size * 3.0,
                "radio montaña {} vs colina {} (lado {side})",
                config.mountains.base.max_size,
                config.hills.base.max_size
            );
            assert!(
                config.mountains.max_height + 0.5 >= config.hills.max_height * 3.0,
                "altura montaña {} vs colina {} (lado {side})",
                config.mountains.max_height,
                config.hills.max_height
            );
        }
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

    #[test]
    fn el_margen_escala_los_rasgos_al_mundo_visible() {
        let mut config = WorldGenConfig::default();
        config.bounds.width = 512.0;
        config.bounds.length = 512.0;
        config.bounds.visible_width = 256.0;
        config.bounds.visible_length = 256.0;
        fit_config_to_world(&mut config);
        // Radio visible 128: una montaña no debe superar 0.32 * 128 ≈ 41 m,
        // aunque el área generada tenga radio 256.
        assert!(config.mountains.base.max_size <= 42.0);
        assert!((world_radius(&config.bounds) - 128.0).abs() < 0.5);
        assert!((generation_radius(&config.bounds) - 256.0).abs() < 0.5);
    }

    #[test]
    fn la_densidad_compensa_el_area_de_crecimiento() {
        let bounds = crate::config::WorldBounds {
            width: 512.0,
            length: 512.0,
            visible_width: 256.0,
            visible_length: 256.0,
            ..crate::config::WorldBounds::default()
        };
        let density = generation_density(&bounds);
        assert!((density - 4.0).abs() < 0.01, "densidad {density}");
    }

    #[test]
    fn la_rampa_cardinal_es_suave_en_cualquier_tamano() {
        for radius in [32.0, 64.0, 128.0, 256.0, 500.0] {
            let side = radius * 2.0;
            let slope = land_grade_amplitude(radius) / side;
            assert!(
                slope <= 0.07,
                "rampa {:.1} m en {:.1} m (radio {radius})",
                land_grade_amplitude(radius),
                side
            );
        }
    }

    #[test]
    fn el_desnivel_crece_con_el_tamano_del_mundo() {
        let small = land_grade_amplitude(MIN_WORLDGEN_RADIUS);
        let medium = land_grade_amplitude(128.0);
        let big = land_grade_amplitude(MAX_WORLDGEN_RADIUS);
        assert!((small - 4.0).abs() < 0.1, "mínimo {small}");
        assert!(medium > small, "medio {medium} debe superar {small}");
        assert!((big - 64.0).abs() < 0.1, "máximo {big}");

        let mut previous = 0.0;
        for radius in [32.0, 64.0, 128.0, 256.0, 400.0, 500.0, 800.0, 1000.0] {
            let current = land_grade_amplitude(radius);
            assert!(
                current >= previous - 1.0e-3,
                "el desnivel bajó de {previous} a {current} en radio {radius}"
            );
            previous = current;
        }
    }
}
