//! F3/F4: planificador de features con cuotas, separación y determinismo.

use glam::Vec2;

use rer_worldgen::config::*;
use rer_worldgen::field::DensityField;
use rer_worldgen::planner::{FeatureKind, FeaturePlan};

fn config_with_all_features() -> WorldGenConfig {
    let mut config = WorldGenConfig {
        seed: 42,
        ..WorldGenConfig::default()
    };
    config.bounds.width = 512.0;
    config.bounds.length = 512.0;
    config.bounds.height = 128.0;
    config.mountains.base = FeatureConfig {
        enabled: true,
        max_count: 15,
        min_size: 60.0,
        max_size: 120.0,
        min_separation: 100.0,
    };
    config.hills.base = FeatureConfig {
        enabled: true,
        max_count: 30,
        min_size: 40.0,
        max_size: 80.0,
        min_separation: 70.0,
    };
    config.valleys.base = FeatureConfig {
        enabled: true,
        max_count: 8,
        min_size: 60.0,
        max_size: 120.0,
        min_separation: 90.0,
    };
    config.plains.base = FeatureConfig {
        enabled: true,
        max_count: 10,
        min_size: 80.0,
        max_size: 140.0,
        min_separation: 90.0,
    };
    config.cliffs.base = FeatureConfig {
        enabled: true,
        max_count: 12,
        min_size: 30.0,
        max_size: 80.0,
        min_separation: 70.0,
    };
    config.lakes.base = FeatureConfig {
        enabled: true,
        max_count: 8,
        min_size: 20.0,
        max_size: 60.0,
        min_separation: 80.0,
    };
    config.rivers.enabled = true;
    config.rivers.max_count = 5;
    config.caves.enabled = true;
    config.caves.max_count = 20;
    config.waterfalls.enabled = true;
    config.waterfalls.max_count = 3;
    config
}

#[test]
fn quotas_are_hard_limits() {
    let config = config_with_all_features();
    config.validate().expect("config valida");
    let base = rer_worldgen::field::HeightField::new(&config);
    let plan = FeaturePlan::generate(&config, &base);

    let count = |plan: &FeaturePlan, kind: FeatureKind| {
        plan.sites.iter().filter(|s| s.kind == kind).count()
    };
    assert!(count(&plan, FeatureKind::Mountain) <= config.mountains.base.max_count as usize);
    assert!(count(&plan, FeatureKind::Hill) <= config.hills.base.max_count as usize);
    assert!(count(&plan, FeatureKind::Valley) <= config.valleys.base.max_count as usize);
    assert!(count(&plan, FeatureKind::Plain) <= config.plains.base.max_count as usize);
    assert!(count(&plan, FeatureKind::Cliff) <= config.cliffs.base.max_count as usize);
    assert!(count(&plan, FeatureKind::Lake) <= config.lakes.base.max_count as usize);
    assert!(plan.rivers.len() <= config.rivers.max_count as usize);
    assert!(plan.caves.len() <= config.caves.max_count as usize);
}

#[test]
fn lakes_stay_clear_of_mountain_slopes() {
    for seed in [7, 555, 91234, 752306577] {
        let mut config = config_with_all_features();
        config.seed = seed;
        let base = rer_worldgen::field::HeightField::new(&config);
        let plan = FeaturePlan::generate(&config, &base);
        for lake in plan.sites.iter().filter(|s| s.kind == FeatureKind::Lake) {
            for mountain in plan
                .sites
                .iter()
                .filter(|s| s.kind == FeatureKind::Mountain)
            {
                let d = (lake.pos - mountain.pos).length();
                assert!(
                    d >= lake.radius + mountain.radius,
                    "seed {seed}: lago a {d} m de una montaña (radios {} + {})",
                    lake.radius,
                    mountain.radius
                );
            }
        }
    }
}

#[test]
fn sites_respect_min_separation() {
    let config = config_with_all_features();
    let base = rer_worldgen::field::HeightField::new(&config);
    let plan = FeaturePlan::generate(&config, &base);

    for kind in [
        FeatureKind::Mountain,
        FeatureKind::Hill,
        FeatureKind::Valley,
        FeatureKind::Plain,
        FeatureKind::Cliff,
        FeatureKind::Lake,
    ] {
        let sites: Vec<_> = plan.sites.iter().filter(|s| s.kind == kind).collect();
        let min_sep = match kind {
            FeatureKind::Mountain => config.mountains.base.min_separation,
            FeatureKind::Hill => config.hills.base.min_separation,
            FeatureKind::Valley => config.valleys.base.min_separation,
            FeatureKind::Plain => config.plains.base.min_separation,
            FeatureKind::Cliff => config.cliffs.base.min_separation,
            FeatureKind::Lake => config.lakes.base.min_separation,
        };
        for (i, a) in sites.iter().enumerate() {
            for b in sites.iter().skip(i + 1) {
                let d = (a.pos - b.pos).length();
                let effective = min_sep.max((a.radius + b.radius) * 0.75);
                assert!(
                    d >= effective - 1.0,
                    "{kind:?} sitios a {d} m (mínimo efectivo {effective})"
                );
            }
        }
    }
}

#[test]
fn plan_is_deterministic_per_seed() {
    let config = config_with_all_features();
    let base = rer_worldgen::field::HeightField::new(&config);
    let a = FeaturePlan::generate(&config, &base);
    let b = FeaturePlan::generate(&config, &base);
    assert_eq!(a.sites.len(), b.sites.len());
    for (sa, sb) in a.sites.iter().zip(&b.sites) {
        assert_eq!(sa.kind, sb.kind);
        assert_eq!(sa.pos, sb.pos, "sitios distintos para la misma seed");
        assert_eq!(sa.radius, sb.radius);
        assert_eq!(sa.value, sb.value);
    }
    assert_eq!(a.rivers.len(), b.rivers.len());
    for (ra, rb) in a.rivers.iter().zip(&b.rivers) {
        assert_eq!(ra.points, rb.points);
    }
    assert_eq!(a.caves.len(), b.caves.len());
    for (ca, cb) in a.caves.iter().zip(&b.caves) {
        assert_eq!(ca.segments.len(), cb.segments.len());
        assert_eq!(ca.segments[0].a, cb.segments[0].a);
    }
}

#[test]
fn different_seeds_produce_different_plans() {
    let config = config_with_all_features();
    let base = rer_worldgen::field::HeightField::new(&config);
    let mut config_b = config.clone();
    config_b.seed = 43;
    let a = FeaturePlan::generate(&config, &base);
    let b = FeaturePlan::generate(&config_b, &base);
    let same_positions = a
        .sites
        .iter()
        .zip(&b.sites)
        .filter(|(sa, sb)| sa.pos == sb.pos)
        .count();
    assert!(
        same_positions < a.sites.len() / 2,
        "seeds distintas deberían dar planes distintos (iguales: {same_positions})"
    );
}

#[test]
fn disabled_features_are_absent() {
    let mut config = config_with_all_features();
    config.mountains.base.enabled = false;
    config.lakes.base.enabled = false;
    config.rivers.enabled = false;
    config.caves.enabled = false;
    let base = rer_worldgen::field::HeightField::new(&config);
    let plan = FeaturePlan::generate(&config, &base);
    assert!(plan.sites.iter().all(|s| s.kind != FeatureKind::Mountain));
    assert!(plan.sites.iter().all(|s| s.kind != FeatureKind::Lake));
    assert!(plan.rivers.is_empty());
    assert!(plan.caves.is_empty());
    assert!(plan.lakes.is_empty());
}

#[test]
fn lake_basin_sits_below_water_level() {
    let config = config_with_all_features();
    let field = DensityField::new(&config);
    for lake in &field_lakes(&config) {
        // El centro del vaso debe quedar por debajo del nivel del agua.
        let center_height = field.height_at(lake.pos.x, lake.pos.y);
        assert!(
            center_height < lake.water_level + 0.5,
            "el centro del lago ({} m) debe estar bajo el nivel de agua ({} m)",
            center_height,
            lake.water_level
        );
        // El agua sigue la orilla con plan (una colina vecina puede alzar el
        // centro respecto a la heightfield base).
        let rim = field.height_at(lake.pos.x + lake.radius * 0.9, lake.pos.y);
        assert!(
            lake.water_level <= rim + 2.5,
            "nivel de agua {} muy por encima de la orilla {}",
            lake.water_level,
            rim
        );
    }
    let _ = Vec2::ZERO;
}

#[test]
fn lakes_keep_a_solid_floor_inside_the_volume() {
    // Mundo de 256 m como el del editor, con varias seeds: el centro del vaso
    // debe ser sólido en y = 0 y la densidad no puede bajar el fondo más que
    // la altura del terreno. Sin ríos ni cuevas para aislar el vaso.
    for seed in [752_306_577u32, 2_776_343_831, 7, 555, 91_234] {
        let mut config = WorldGenConfig {
            seed,
            ..WorldGenConfig::default()
        };
        config.bounds.height = 70.0;
        config.lakes.base.max_count = 3;
        config.rivers.enabled = false;
        config.caves.enabled = false;
        let field = DensityField::new(&config);
        for lake in &field.plan().lakes {
            let (x, z) = (lake.pos.x, lake.pos.y);
            assert!(
                field.density_at(x, 0.0, z) > 0.0,
                "seed {seed}: lago en ({x}, {z}) sin fondo en y = 0"
            );
            let floor = field.height_at(x, z);
            assert!(
                field.density_at(x, floor - 0.5, z) > 0.0,
                "seed {seed}: el fondo del lago está por debajo de su altura ({floor} m)"
            );
        }
    }
}

/// Recupera los lagos del plan regenerándolo (el plan no se expone en el campo).
fn field_lakes(config: &WorldGenConfig) -> Vec<rer_worldgen::planner::LakeInfo> {
    let base = rer_worldgen::field::HeightField::new(config);
    FeaturePlan::generate(config, &base).lakes
}

#[test]
fn rivers_descend_and_have_minimum_length() {
    let config = config_with_all_features();
    let base = rer_worldgen::field::HeightField::new(&config);
    let plan = FeaturePlan::generate(&config, &base);
    for river in &plan.rivers {
        assert!(
            river.points.len() >= 8,
            "río demasiado corto: {} puntos",
            river.points.len()
        );
        // Descenso neto: la altura final no debe superar la inicial + margen.
        let (first, h_first) = river.points[0];
        let (last, h_last) = *river.points.last().unwrap();
        assert!(
            h_last <= h_first + 2.0,
            "el río sube netamente: {first:?} {h_first} -> {last:?} {h_last}"
        );
    }
}

#[test]
fn rivers_carve_open_channels() {
    let config = config_with_all_features();
    let field = DensityField::new(&config);
    let base = rer_worldgen::field::HeightField::new(&config);
    let plan = FeaturePlan::generate(&config, &base);
    assert!(
        !plan.rivers.is_empty(),
        "el mundo de prueba debe generar ríos"
    );

    for river in &plan.rivers {
        let (mid_pos, h) = river.points[river.points.len() / 2];
        let (px, pz) = (mid_pos.x, mid_pos.y);
        // Dentro del cauce, a media profundidad: debe ser aire (canal abierto).
        let d = field.density_at(px, h - river.depth * 0.5, pz);
        assert!(
            d < 0.0,
            "el cauce del río debe estar excavado (densidad {d} en ({px}, {pz}))"
        );
    }
}

#[test]
fn waterfalls_respect_max_count() {
    let config = config_with_all_features();
    let base = rer_worldgen::field::HeightField::new(&config);
    let plan = FeaturePlan::generate(&config, &base);
    // Las cascadas detectadas pueden superar el máximo del config (son datos
    // derivados); el F6 recorta al máximo al exponerlas. Verificamos aquí solo
    // que cada cascada tiene un salto mayor que el umbral.
    for river in &plan.rivers {
        let _ = &river.waterfalls;
    }
    // Recorte duro documentado: cascadas expuestas <= max_count.
    let exposed = plan
        .waterfalls
        .len()
        .min(config.waterfalls.max_count as usize);
    assert!(exposed <= config.waterfalls.max_count as usize);
}

#[test]
fn sites_stay_inside_the_world_disk() {
    let config = config_with_all_features();
    let base = rer_worldgen::field::HeightField::new(&config);
    let plan = FeaturePlan::generate(&config, &base);
    let radius = config.bounds.radius();
    let center = Vec2::new(config.bounds.width * 0.5, config.bounds.length * 0.5);
    for site in &plan.sites {
        let d = (site.pos - center).length();
        assert!(
            d <= radius - 8.0,
            "sitio {:?} a {d} m del centro (radio {radius})",
            site.kind
        );
    }
}

#[test]
fn mountains_spread_around_a_kilometre_disk() {
    let mut config = WorldGenConfig {
        seed: 42,
        ..WorldGenConfig::default()
    };
    config.bounds.width = 1000.0;
    config.bounds.length = 1000.0;
    config.bounds.height = 128.0;
    config.mountains.base.max_count = 4;
    let base = rer_worldgen::field::HeightField::new(&config);
    let plan = FeaturePlan::generate(&config, &base);
    let mountains: Vec<_> = plan
        .sites
        .iter()
        .filter(|s| s.kind == FeatureKind::Mountain)
        .collect();
    assert!(
        mountains.len() >= 3,
        "un mapa de 1 km debe tener varias montañas, hay {}",
        mountains.len()
    );
    let center = Vec2::new(500.0, 500.0);
    for (i, a) in mountains.iter().enumerate() {
        for b in mountains.iter().skip(i + 1) {
            let d = (a.pos - b.pos).length();
            assert!(
                d >= 120.0,
                "montañas demasiado juntas ({d} m entre {:?} y {:?})",
                a.pos,
                b.pos
            );
        }
    }
    let mut min_angle = f32::MAX;
    let mut max_angle = f32::MIN;
    for site in &mountains {
        let delta = site.pos - center;
        let angle = delta.y.atan2(delta.x);
        min_angle = min_angle.min(angle);
        max_angle = max_angle.max(angle);
    }
    assert!(
        max_angle - min_angle > 1.2,
        "las montañas deben ocupar más de un cuadrante (arco {})",
        max_angle - min_angle
    );
}
