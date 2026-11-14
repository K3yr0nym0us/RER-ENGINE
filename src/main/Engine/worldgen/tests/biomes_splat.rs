//! F5: biomas y splat mapping (pesos de material + albedo de preview).

use rer_worldgen::biomes::{Biome, BiomeMap};
use rer_worldgen::config::*;
use rer_worldgen::field::DensityField;
use rer_worldgen::planner::FeaturePlan;
use rer_worldgen::splat::{SPLAT_DIM, SplatContext, chunk_splat};

fn config_for_biomes() -> WorldGenConfig {
    let mut config = WorldGenConfig {
        seed: 555,
        ..WorldGenConfig::default()
    };
    config.bounds.width = 512.0;
    config.bounds.length = 512.0;
    config.bounds.height = 128.0;
    config.biomes = BiomesConfig {
        grassland: true,
        forest: true,
        desert: true,
        snow: true,
        volcanic: true,
    };
    config
}

#[test]
fn biome_map_is_deterministic_and_valid() {
    let config = config_for_biomes();
    let map = BiomeMap::new(&config);
    for i in 0..2_000i32 {
        let x = (i * 37 % 511) as f32 + 0.5;
        let z = (i * 91 % 511) as f32 + 0.5;
        let y = (i as f32 * 0.05) % config.bounds.height;
        let a = map.biome_at(x, z, y);
        let b = map.biome_at(x, z, y);
        assert_eq!(a, b, "bioma no determinista en ({x}, {z}, {y})");
    }
}

#[test]
fn snow_override_applies_at_high_altitude() {
    let config = config_for_biomes();
    let map = BiomeMap::new(&config);
    // Muy por encima de la línea de nieve: siempre nieve (override).
    let y = map.snow_line() + 10.0;
    for i in 0..500i32 {
        let x = (i * 17 % 509) as f32 + 0.5;
        let z = (i * 53 % 509) as f32 + 0.5;
        assert_eq!(
            map.biome_at(x, z, y),
            Biome::Snow,
            "sin override de nieve en ({x}, {z})"
        );
    }
}

#[test]
fn disabled_biomes_never_appear() {
    let mut config = config_for_biomes();
    config.biomes = BiomesConfig {
        grassland: true,
        forest: false,
        desert: false,
        snow: false,
        volcanic: false,
    };
    let map = BiomeMap::new(&config);
    for i in 0..1_000i32 {
        let x = (i * 23 % 499) as f32;
        let z = (i * 71 % 499) as f32;
        let y = (i as f32 * 0.13) % config.bounds.height;
        assert_eq!(
            map.biome_at(x, z, y),
            Biome::Grassland,
            "único bioma activo es pradera"
        );
    }
}

#[test]
fn splat_weights_sum_to_255_everywhere() {
    let config = config_for_biomes();
    let field = DensityField::new(&config);
    let base = field.base_heightfield();
    let plan = FeaturePlan::generate(&config, base);
    let ctx = SplatContext::new(&field, &config, plan.sites.clone());
    for i in 0..3_000i32 {
        let x = (i * 29 % 503) as f32 + 0.25;
        let z = (i * 61 % 503) as f32 + 0.75;
        let y = field.height_at(x, z);
        let w = ctx.weights_at(x, z, y);
        let sum = w.grass as u32 + w.rock as u32 + w.sand as u32 + w.snow as u32;
        assert!(
            (252..=258).contains(&sum),
            "los pesos deben sumar ~255 (suman {sum} en ({x}, {z}))"
        );
    }
}

#[test]
fn splat_is_snow_at_high_altitude_and_rock_on_steep_slopes() {
    let config = config_for_biomes();
    let field = DensityField::new(&config);
    let base = field.base_heightfield();
    let plan = FeaturePlan::generate(&config, base);
    let ctx = SplatContext::new(&field, &config, plan.sites.clone());

    // Encuentra una montaña real (sitio) y verifica: nieve cerca de la cima.
    let mountain = plan
        .sites
        .iter()
        .find(|s| s.kind == rer_worldgen::planner::FeatureKind::Mountain)
        .expect("el mundo de prueba debe tener montañas");
    let top = field.height_at(mountain.pos.x, mountain.pos.y);
    if top > ctx_world_snow_line(&config) {
        let w = ctx.weights_at(mountain.pos.x, mountain.pos.y, top);
        assert!(w.snow > 128, "la cima de una montaña alta debe ser nieve");
    }

    // Pendiente alta ⇒ roca: muestrea el flanco de la montaña.
    for k in 1..40u32 {
        let r = mountain.radius * 0.35;
        let angle = k as f32 * 0.4;
        let x = mountain.pos.x + angle.cos() * r;
        let z = mountain.pos.y + angle.sin() * r;
        let y = field.height_at(x, z);
        let w = ctx.weights_at(x, z, y);
        // En flancos empinados la roca domina sobre la hierba.
        if w.rock > 0 {
            break;
        }
        if k == 39 {
            panic!("ningún punto del flanco montañoso con roca: pendiente no detectada");
        }
    }
}

fn ctx_world_snow_line(config: &WorldGenConfig) -> f32 {
    config.bounds.height * 0.72
}

#[test]
fn chunk_splat_outputs_consistent_buffers() {
    let config = config_for_biomes();
    let field = DensityField::new(&config);
    let base = field.base_heightfield();
    let plan = FeaturePlan::generate(&config, base);
    let ctx = SplatContext::new(&field, &config, plan.sites.clone());

    let (splat, albedo, dim) = chunk_splat(&field, &ctx, 2, 1, 0);
    assert_eq!(dim, SPLAT_DIM);
    assert_eq!(splat.len(), (dim * dim * 4) as usize);
    assert_eq!(albedo.len(), splat.len());
    // Albedo opaco (alpha 255) en todos los texels.
    for i in 0..(dim * dim) as usize {
        assert_eq!(albedo[i * 4 + 3], 255);
    }
    // Determinismo.
    let (splat_b, albedo_b, _) = chunk_splat(&field, &ctx, 2, 1, 0);
    assert_eq!(splat, splat_b);
    assert_eq!(albedo, albedo_b);
}
