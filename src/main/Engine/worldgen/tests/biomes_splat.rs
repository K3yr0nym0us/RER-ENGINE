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
        shares: None,
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
        shares: None,
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
    let biomes = BiomeMap::new(&config);
    let ctx = SplatContext::new(
        &field,
        &biomes,
        config.bounds.height,
        config.resolution.voxel_size,
    );
    for i in 0..3_000i32 {
        let x = (i * 29 % 503) as f32 + 0.25;
        let z = (i * 61 % 503) as f32 + 0.75;
        let y = field.height_at(x, z);
        let w = ctx.weights_at(x, z, y);
        let sum = w.grass as u32 + w.rock as u32 + w.sand as u32 + w.snow as u32;
        let in_lake = field.plan().sites.iter().any(|site| {
            site.kind == rer_worldgen::planner::FeatureKind::Lake && site.norm_dist(x, z) < 1.14
        });
        let point = glam::Vec2::new(x, z);
        let in_river = field.plan().rivers.iter().any(|river| {
            river.points.windows(2).any(|pair| {
                let a = pair[0].0;
                let b = pair[1].0;
                let ab = b - a;
                let len2 = ab.length_squared();
                let t = if len2 <= f32::EPSILON {
                    0.0
                } else {
                    ((point - a).dot(ab) / len2).clamp(0.0, 1.0)
                };
                (a + ab * t - point).length() < river.width * 1.85
            })
        });
        if in_lake || in_river {
            assert!(
                sum <= 258,
                "en el vaso los pesos no deben pasar de 255 (suman {sum} en ({x}, {z}))"
            );
        } else {
            assert!(
                (252..=258).contains(&sum),
                "los pesos deben sumar ~255 (suman {sum} en ({x}, {z}))"
            );
        }
    }
}

#[test]
fn splat_is_snow_at_high_altitude_and_rock_on_steep_slopes() {
    let config = config_for_biomes();
    let field = DensityField::new(&config);
    let base = field.base_heightfield();
    let plan = FeaturePlan::generate(&config, base);
    let biomes = BiomeMap::for_field(&config, &field);
    let mut fitted = config.clone();
    rer_worldgen::scale::fit_config_to_world(&mut fitted);
    let ctx = SplatContext::new(
        &field,
        &biomes,
        fitted.bounds.height,
        fitted.resolution.voxel_size,
    );

    // Encuentra una montaña real (sitio) y verifica: nieve cerca de la cima.
    let mountain = plan
        .sites
        .iter()
        .find(|s| s.kind == rer_worldgen::planner::FeatureKind::Mountain)
        .expect("el mundo de prueba debe tener montañas");
    let top = field.height_at(mountain.pos.x, mountain.pos.y);
    if top > ctx_world_snow_line(&config) {
        let mut snowy = false;
        for k in 0..12u32 {
            let r = mountain.radius * 0.06 * k as f32;
            let angle = k as f32 * 0.7;
            let x = mountain.pos.x + angle.cos() * r;
            let z = mountain.pos.y + angle.sin() * r;
            let y = field.height_at(x, z);
            let w = ctx.weights_at(x, z, y);
            if w.snow > 128 {
                snowy = true;
                break;
            }
        }
        assert!(snowy, "la cima de una montaña alta debe ser nieve");
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
    rer_worldgen::scale::snow_line_meters(config)
}

#[test]
fn snow_stays_off_the_plains() {
    let config = config_for_biomes();
    let map = BiomeMap::new(&config);
    let plains = TERRAIN_FLOOR + TERRAIN_PLAINS_CLEARANCE;
    let mut snow = 0u32;
    for i in 0..800i32 {
        let x = (i * 19 % 509) as f32 + 0.5;
        let z = (i * 47 % 509) as f32 + 0.5;
        if map.biome_at(x, z, plains) == Biome::Snow {
            snow += 1;
        }
    }
    assert_eq!(snow, 0, "la nieve no debe pintar las llanuras");
}

#[test]
fn plains_are_mostly_grass_not_desert() {
    let config = config_for_biomes();
    let map = BiomeMap::new(&config);
    let plains = TERRAIN_FLOOR + TERRAIN_PLAINS_CLEARANCE;
    let mut grass = 0u32;
    let mut desert = 0u32;
    for i in 0..1_200i32 {
        let x = (i * 13 % 511) as f32 + 0.25;
        let z = (i * 29 % 511) as f32 + 0.75;
        match map.biome_at(x, z, plains) {
            Biome::Grassland | Biome::Forest => grass += 1,
            Biome::Desert => desert += 1,
            _ => {}
        }
    }
    assert!(
        grass > desert * 2,
        "la pradera/bosque debe dominar el suelo (hierba {grass}, desierto {desert})"
    );
}

#[test]
fn kilometre_plains_stay_mostly_grass() {
    let mut config = config_for_biomes();
    config.bounds.width = 1000.0;
    config.bounds.length = 1000.0;
    config.bounds.height = 256.0;
    let map = BiomeMap::new(&config);
    let plains = plains_height();
    let mut grass = 0u32;
    let mut desert = 0u32;
    let samples = 2_000i32;
    for i in 0..samples {
        let x = (i * 17 % 997) as f32 + 0.5;
        let z = (i * 41 % 997) as f32 + 0.5;
        match map.biome_at(x, z, plains) {
            Biome::Grassland | Biome::Forest => grass += 1,
            Biome::Desert => desert += 1,
            _ => {}
        }
    }
    assert!(
        grass > desert * 4,
        "a 1 km el suelo debe ser hierba, no desierto (hierba {grass}, desierto {desert})"
    );
    assert!(
        grass * 100 / samples as u32 > 65,
        "al menos 65 % de pradera/bosque (hierba {grass} de {samples})"
    );
}

#[test]
fn lake_shore_blends_mud_with_surroundings() {
    let mut lakes_checked = 0;
    for seed in [7, 555, 91234, 752306577, 2776343831] {
        let mut config = config_for_biomes();
        config.seed = seed;
        config.biomes.desert = false;
        config.biomes.volcanic = false;
        config.lakes.base.enabled = true;
        config.lakes.base.max_count = 4;
        let field = DensityField::new(&config);
        let plan = FeaturePlan::generate(&config, field.base_heightfield());
        let biomes = BiomeMap::new(&config);
        let ctx = SplatContext::new(
            &field,
            &biomes,
            config.bounds.height,
            config.resolution.voxel_size,
        );
        let lakes = plan
            .sites
            .iter()
            .filter(|s| s.kind == rer_worldgen::planner::FeatureKind::Lake);
        for lake in lakes {
            lakes_checked += 1;
            let mut blended = 0u32;
            for k in 0..64 {
                let angle = k as f32 * std::f32::consts::TAU / 64.0;
                let (dx, dz) = (angle.cos(), angle.sin());
                let mut lo = lake.radius * 0.4;
                let mut hi = lake.radius * 1.7;
                for _ in 0..12 {
                    let mid = (lo + hi) * 0.5;
                    let (x, z) = (lake.pos.x + dx * mid, lake.pos.y + dz * mid);
                    if lake.norm_dist(x, z) > 1.0 {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                let (x, z) = (lake.pos.x + dx * lo, lake.pos.y + dz * lo);
                let w = ctx.weights_with_slope(x, z, field.height_at(x, z), 0.0);
                let around = w.grass as u32 + w.rock as u32 + w.snow as u32;
                if w.mud() > 28 && around > 28 && w.sand < 48 {
                    blended += 1;
                }
            }
            assert!(
                blended >= 40,
                "seed {seed}: la orilla debe mezclar lodo con el suelo ({blended}/64)"
            );
        }
    }
    assert!(lakes_checked > 0, "los mundos de prueba deben tener lagos");
}

#[test]
fn lake_basin_is_mud() {
    let mut lakes_checked = 0;
    for seed in [7, 555, 91234, 752306577, 2776343831] {
        let mut config = config_for_biomes();
        config.seed = seed;
        config.biomes.desert = false;
        config.biomes.volcanic = false;
        config.lakes.base.enabled = true;
        config.lakes.base.max_count = 4;
        let field = DensityField::new(&config);
        let biomes = BiomeMap::new(&config);
        let ctx = SplatContext::new(
            &field,
            &biomes,
            config.bounds.height,
            config.resolution.voxel_size,
        );
        for lake in field
            .plan()
            .sites
            .iter()
            .filter(|site| site.kind == rer_worldgen::planner::FeatureKind::Lake)
        {
            lakes_checked += 1;
            let (x, z) = (lake.pos.x, lake.pos.y);
            let w = ctx.weights_with_slope(x, z, field.height_at(x, z), 0.0);
            assert!(
                w.mud() > 160,
                "seed {seed}: el vaso del lago debe ser lodo (mud {} en {:?})",
                w.mud(),
                lake.pos
            );
            assert!(
                w.grass < 40,
                "seed {seed}: el vaso no debe verse como pradera (hierba {})",
                w.grass
            );
        }
    }
    assert!(lakes_checked > 0, "los mundos de prueba deben tener lagos");
}

#[test]
fn river_channel_is_mud() {
    let mut rivers_checked = 0;
    for seed in [7, 42, 555, 91234, 752306577] {
        let mut config = config_for_biomes();
        config.seed = seed;
        config.rivers.enabled = true;
        config.rivers.max_count = 5;
        let field = DensityField::new(&config);
        let biomes = BiomeMap::new(&config);
        let ctx = SplatContext::new(
            &field,
            &biomes,
            config.bounds.height,
            config.resolution.voxel_size,
        );
        for river in &field.plan().rivers {
            if river.points.len() < 8 {
                continue;
            }
            rivers_checked += 1;
            let mut best_mud = 0u8;
            let mut best_grass = 255u8;
            let mut sample = river.points[river.points.len() / 2].0;
            for frac in [0.35, 0.5, 0.65, 0.8] {
                let idx = ((river.points.len() - 1) as f32 * frac) as usize;
                let pos = river.points[idx].0;
                let w = ctx.weights_with_slope(pos.x, pos.y, field.height_at(pos.x, pos.y), 0.0);
                if w.mud() >= best_mud {
                    best_mud = w.mud();
                    best_grass = w.grass;
                    sample = pos;
                }
            }
            assert!(
                best_mud > 160,
                "seed {seed}: el cauce debe ser lodo (mud {best_mud} en {sample:?})"
            );
            assert!(
                best_grass < 40,
                "seed {seed}: el cauce no debe verse como pradera (hierba {best_grass})"
            );
        }
    }
    assert!(rivers_checked > 0, "los mundos de prueba deben tener ríos");
}

#[test]
fn river_bank_blends_mud_with_surroundings() {
    let mut banks = 0u32;
    for seed in [7, 42, 555, 91234, 752306577] {
        let mut config = config_for_biomes();
        config.seed = seed;
        config.biomes.desert = false;
        config.rivers.enabled = true;
        config.rivers.max_count = 5;
        let field = DensityField::new(&config);
        let biomes = BiomeMap::new(&config);
        let ctx = SplatContext::new(
            &field,
            &biomes,
            config.bounds.height,
            config.resolution.voxel_size,
        );
        for river in &field.plan().rivers {
            if river.points.len() < 8 {
                continue;
            }
            let mid_i = river.points.len() / 2;
            let a = river.points[mid_i - 1].0;
            let b = river.points[mid_i].0;
            let along = b - a;
            if along.length_squared() <= f32::EPSILON {
                continue;
            }
            let normal = glam::Vec2::new(-along.y, along.x).normalize() * river.width;
            let shore = b + normal;
            let w =
                ctx.weights_with_slope(shore.x, shore.y, field.height_at(shore.x, shore.y), 0.0);
            let around = w.grass as u32 + w.rock as u32 + w.snow as u32;
            if w.mud() > 20 && around > 20 && w.sand < 48 {
                banks += 1;
            }
        }
    }
    assert!(
        banks >= 3,
        "las orillas de los ríos deben mezclar lodo con el suelo ({banks})"
    );
}

#[test]
fn cave_mouth_keeps_surface_biome() {
    let mut caves_checked = 0;
    for seed in [7, 42, 555, 1234, 91234] {
        let mut config = config_for_biomes();
        config.seed = seed;
        config.caves.enabled = true;
        config.caves.max_count = 6;
        config.mountains.base.enabled = true;
        config.mountains.base.max_count = 8;
        let field = DensityField::new(&config);
        let biomes = BiomeMap::new(&config);
        let ctx = SplatContext::new(
            &field,
            &biomes,
            config.bounds.height,
            config.resolution.voxel_size,
        );
        for cave in &field.plan().caves {
            if cave.segments.len() < 4 {
                continue;
            }
            caves_checked += 1;
            let first = cave.segments[0].a.lerp(cave.segments[0].b, 0.4);
            let y = field.height_at(first.x, first.z);
            let w = ctx.weights_with_slope(first.x, first.z, y, 0.0);
            let rock_alt = {
                let t = ((y - config.bounds.height * 0.60)
                    / (config.bounds.height * 0.12).max(1.0e-3))
                .clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            };
            assert!(
                w.rock as f32 <= rock_alt * 255.0 + 40.0,
                "seed {seed}: la boca no debe pintar piedra de cueva en superficie (rock {} alt {rock_alt} en {first:?})",
                w.rock
            );
        }
    }
    assert!(caves_checked > 0, "los mundos de prueba deben tener cuevas");
}

#[test]
fn chunk_splat_outputs_consistent_buffers() {
    let config = config_for_biomes();
    let field = DensityField::new(&config);
    let biomes = BiomeMap::new(&config);
    let ctx = SplatContext::new(
        &field,
        &biomes,
        config.bounds.height,
        config.resolution.voxel_size,
    );

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

fn config_with_shares(shares: BiomeShares) -> WorldGenConfig {
    let mut config = config_for_biomes();
    config.biomes.shares = Some(shares);
    config
}

/// Fracción del disco que ocupa cada bioma (índice canónico) sobre el relieve
/// `height_at`, con la nieve ajustada a ese relieve.
fn disk_fractions_on(config: &WorldGenConfig, height_at: impl Fn(f32, f32) -> f32) -> [f32; 5] {
    let mut map = BiomeMap::new(config);
    map.fit_snow_to_terrain(&height_at);
    let radius = config.bounds.width.min(config.bounds.length) * 0.5;
    let (cx, cz) = (config.bounds.width * 0.5, config.bounds.length * 0.5);
    let mut counts = [0u32; 5];
    let mut total = 0u32;
    for iz in 0..150 {
        for ix in 0..150 {
            let x = cx - radius + (ix as f32 + 0.37) * radius * 2.0 / 150.0;
            let z = cz - radius + (iz as f32 + 0.61) * radius * 2.0 / 150.0;
            if (x - cx).hypot(z - cz) > radius {
                continue;
            }
            counts[map.biome_at(x, z, height_at(x, z)).index()] += 1;
            total += 1;
        }
    }
    counts.map(|count| count as f32 / total as f32)
}

fn disk_fractions(config: &WorldGenConfig) -> [f32; 5] {
    disk_fractions_on(config, |_, _| 0.0)
}

/// Relieve sintético: llanura en el borde y cumbre de 48 m en el centro.
fn central_peak(config: &WorldGenConfig) -> impl Fn(f32, f32) -> f32 {
    let radius = config.bounds.width.min(config.bounds.length) * 0.5;
    let (cx, cz) = (config.bounds.width * 0.5, config.bounds.length * 0.5);
    move |x, z| 8.0 + 40.0 * (1.0 - (x - cx).hypot(z - cz) / radius).max(0.0)
}

#[test]
fn biome_shares_put_snow_on_the_highest_ground() {
    let config = config_with_shares(BiomeShares {
        grassland: 85,
        forest: 0,
        desert: 0,
        snow: 15,
        volcanic: 0,
    });
    let peak = central_peak(&config);
    let fractions = disk_fractions_on(&config, &peak);
    let snow = fractions[Biome::Snow.index()];
    assert!(
        snow < 0.08,
        "nieve no debe bajar de las cumbres ({:.1} % del disco)",
        snow * 100.0
    );

    let mut map = BiomeMap::new(&config);
    map.fit_snow_to_terrain(&peak);
    let (cx, cz) = (config.bounds.width * 0.5, config.bounds.length * 0.5);
    let mid = peak(cx + 40.0, cz);
    assert_eq!(map.biome_at(cx, cz, peak(cx, cz)), Biome::Snow);
    assert_eq!(map.biome_at(cx + 40.0, cz, mid), Biome::Grassland);
    assert!(
        mid > 8.0 && mid < peak(cx, cz) - 4.0,
        "el control de ladera debe ser más bajo que la cumbre"
    );
}

#[test]
fn biome_shares_keep_snow_off_flat_ground() {
    let config = config_with_shares(BiomeShares {
        grassland: 50,
        forest: 0,
        desert: 0,
        snow: 50,
        volcanic: 0,
    });
    let fractions = disk_fractions_on(&config, |_, _| 8.0);
    assert_eq!(fractions[Biome::Snow.index()], 0.0);
}

#[test]
fn biome_shares_set_the_area_of_each_biome() {
    let config = config_with_shares(BiomeShares {
        grassland: 45,
        forest: 30,
        desert: 25,
        snow: 0,
        volcanic: 0,
    });
    let fractions = disk_fractions(&config);
    let expected = [0.45, 0.30, 0.25, 0.0, 0.0];
    for biome in Biome::ALL {
        let got = fractions[biome.index()];
        let want = expected[biome.index()];
        assert!(
            (got - want).abs() < 0.08,
            "{biome:?}: {:.1} % del disco, pedido {:.0} %",
            got * 100.0,
            want * 100.0
        );
    }
}

#[test]
fn volcanic_share_stays_around_mountains() {
    let config = config_with_shares(BiomeShares {
        grassland: 50,
        forest: 15,
        desert: 15,
        snow: 5,
        volcanic: 15,
    });
    let field = DensityField::new(&config);
    let map = BiomeMap::for_field(&config, &field);
    let mountains: Vec<_> = field
        .plan()
        .sites
        .iter()
        .filter(|s| s.kind == rer_worldgen::planner::FeatureKind::Mountain)
        .copied()
        .collect();
    assert!(
        !mountains.is_empty(),
        "el mundo de prueba debe tener montañas"
    );
    let radius = config.bounds.width.min(config.bounds.length) * 0.5;
    let (cx, cz) = (config.bounds.width * 0.5, config.bounds.length * 0.5);
    let mut volcanic = 0u32;
    let mut far = 0u32;
    for iz in 0..80 {
        for ix in 0..80 {
            let x = cx - radius + (ix as f32 + 0.5) * radius * 2.0 / 80.0;
            let z = cz - radius + (iz as f32 + 0.5) * radius * 2.0 / 80.0;
            if (x - cx).hypot(z - cz) > radius {
                continue;
            }
            if map.biome_at(x, z, field.height_at(x, z)) != Biome::Volcanic {
                continue;
            }
            volcanic += 1;
            if mountains.iter().all(|m| m.norm_dist(x, z) >= 1.4) {
                far += 1;
            }
        }
    }
    assert!(volcanic > 8, "debe haber suelo volcánico ({volcanic})");
    assert!(
        far * 8 < volcanic,
        "el volcánico no debe irse lejos de las montañas ({far}/{volcanic})"
    );
}

#[test]
fn biome_shares_ignore_disabled_biomes_and_renormalize() {
    let mut config = config_with_shares(BiomeShares {
        grassland: 30,
        forest: 0,
        desert: 30,
        snow: 40,
        volcanic: 0,
    });
    config.biomes.snow = false;
    let fractions = disk_fractions(&config);
    assert_eq!(fractions[Biome::Snow.index()], 0.0);
    assert!((fractions[Biome::Grassland.index()] - 0.5).abs() < 0.05);
    assert!((fractions[Biome::Desert.index()] - 0.5).abs() < 0.05);
}

#[test]
fn biome_share_weights_blend_and_sum_to_one() {
    let config = config_with_shares(BiomeShares {
        grassland: 50,
        forest: 0,
        desert: 50,
        snow: 0,
        volcanic: 0,
    });
    let map = BiomeMap::new(&config);
    let mut blended = 0;
    for i in 0..4_000i32 {
        let x = (i * 37 % 509) as f32 + 0.5;
        let z = (i * 91 % 509) as f32 + 0.5;
        let weights = map.biome_weights(x, z, 0.0);
        let sum: f32 = weights.iter().sum();
        assert!((sum - 1.0).abs() < 1.0e-4, "los pesos suman {sum}");
        let grass = weights[Biome::Grassland.index()];
        if grass > 0.05 && grass < 0.95 {
            blended += 1;
        }
    }
    assert!(blended > 0, "la frontera entre biomas debe fundirse");
}

#[test]
fn desert_stays_away_from_lakes_and_rivers() {
    let mut config = config_with_shares(BiomeShares {
        grassland: 40,
        forest: 20,
        desert: 40,
        snow: 0,
        volcanic: 0,
    });
    config.lakes.base.enabled = true;
    config.lakes.base.max_count = 6;
    config.rivers.enabled = true;
    config.rivers.max_count = 5;
    let field = DensityField::new(&config);
    let map = BiomeMap::for_field(&config, &field);
    let plan = field.plan();
    let lakes: Vec<_> = plan
        .sites
        .iter()
        .filter(|site| site.kind == rer_worldgen::planner::FeatureKind::Lake)
        .copied()
        .collect();
    let rivers: Vec<Vec<_>> = plan
        .rivers
        .iter()
        .map(|river| river.points.iter().map(|(pos, _)| *pos).collect())
        .collect();
    assert!(
        !lakes.is_empty() || !rivers.is_empty(),
        "el mundo de prueba debe tener agua"
    );
    let radius = config.bounds.width.min(config.bounds.length) * 0.5;
    let (cx, cz) = (config.bounds.width * 0.5, config.bounds.length * 0.5);
    let mut desert = 0u32;
    for iz in 0..80 {
        for ix in 0..80 {
            let x = cx - radius + (ix as f32 + 0.5) * radius * 2.0 / 80.0;
            let z = cz - radius + (iz as f32 + 0.5) * radius * 2.0 / 80.0;
            if (x - cx).hypot(z - cz) > radius {
                continue;
            }
            if map.biome_at(x, z, field.height_at(x, z)) != Biome::Desert {
                continue;
            }
            desert += 1;
            let near_lake = lakes.iter().any(|lake| lake.norm_dist(x, z) < 1.22);
            let point = glam::Vec2::new(x, z);
            let near_river = rivers.iter().any(|path| {
                path.windows(2).any(|pair| {
                    let a = pair[0];
                    let b = pair[1];
                    let ab = b - a;
                    let len2 = ab.length_squared();
                    let t = if len2 <= f32::EPSILON {
                        0.0
                    } else {
                        ((point - a).dot(ab) / len2).clamp(0.0, 1.0)
                    };
                    (a + ab * t - point).length() < 18.0
                })
            });
            assert!(
                !near_lake && !near_river,
                "desierto junto al agua en ({x}, {z})"
            );
        }
    }
    assert!(desert > 8, "debe quedar desierto lejos del agua ({desert})");
}
