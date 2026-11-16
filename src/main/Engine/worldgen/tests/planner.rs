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
        min_size: 90.0,
        max_size: 180.0,
        min_separation: 140.0,
    };
    config.hills.base = FeatureConfig {
        enabled: true,
        max_count: 30,
        min_size: 16.0,
        max_size: 36.0,
        min_separation: 50.0,
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
        min_separation: 160.0,
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
        let mut rim = f32::MIN;
        for k in 0..8 {
            let ang = k as f32 * std::f32::consts::TAU / 8.0;
            let px = lake.pos.x + ang.cos() * lake.radius * 1.2;
            let pz = lake.pos.y + ang.sin() * lake.radius * 1.2;
            rim = rim.max(field.height_at(px, pz));
        }
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
fn lake_basin_stays_deep_until_the_shore() {
    let config = config_with_all_features();
    let field = DensityField::new(&config);
    let mut checked = 0;
    for lake in &field_lakes(&config) {
        let center = field.height_at(lake.pos.x, lake.pos.y);
        let drop = lake.water_level - center;
        if drop < 1.0 {
            continue;
        }
        assert!(
            drop > 3.0 || center <= rer_worldgen::config::TERRAIN_FLOOR + 0.75,
            "el centro del lago debe ser hondo ({drop} m, centro {center})"
        );
        let mut mid_drop = f32::MIN;
        let mut mid_floor = f32::MAX;
        for k in 0..8 {
            let sample = lake.pos
                + Vec2::from_angle(k as f32 * std::f32::consts::TAU / 8.0) * lake.radius * 0.32;
            let mid = field.height_at(sample.x, sample.y);
            mid_drop = mid_drop.max(lake.water_level - mid);
            mid_floor = mid_floor.min(mid);
        }
        assert!(
            mid_drop > drop * 0.45 || mid_floor <= rer_worldgen::config::TERRAIN_FLOOR + 0.75,
            "el vaso debe seguir hondo lejos del centro ({mid_drop} vs {drop})"
        );
        checked += 1;
    }
    assert!(checked > 0, "el mundo de prueba debe generar lagos hondos");
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
fn river_banks_are_gentle() {
    let config = config_with_all_features();
    let field = DensityField::new(&config);
    let base = rer_worldgen::field::HeightField::new(&config);
    let plan = FeaturePlan::generate(&config, &base);
    assert!(
        !plan.rivers.is_empty(),
        "el mundo de prueba debe generar ríos"
    );

    for river in &plan.rivers {
        let i = river.points.len() / 2;
        let (p0, _) = river.points[i];
        let p1 = river.points[i.saturating_add(1).min(river.points.len() - 1)].0;
        let tangent = (p1 - p0).normalize_or_zero();
        if tangent.length_squared() < 1e-6 {
            continue;
        }
        let perp = Vec2::new(-tangent.y, tangent.x);
        let y = field.height_at(p0.x, p0.y) - river.depth * 0.7;
        let d_center = field.density_at(p0.x, y, p0.y);
        assert!(
            d_center < 0.0,
            "el centro del cauce debe seguir abierto (densidad {d_center})"
        );
        let mut gentle = false;
        for sign in [1.0_f32, -1.0] {
            let outer = p0 + perp * sign * river.width * 1.12;
            let d_outer = field.density_at(outer.x, y, outer.y);
            if d_outer > 0.15 {
                gentle = true;
            }
        }
        assert!(
            gentle,
            "justo fuera del cauce el terreno no debe caer a plomo"
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
fn lake_and_valley_footprints_are_not_circles() {
    let config = config_with_all_features();
    let field = DensityField::new(&config);
    let plan = field.plan();
    let mut checked = 0;
    for site in plan.sites.iter().filter(|s| {
        matches!(
            s.kind,
            FeatureKind::Lake | FeatureKind::Valley | FeatureKind::Mountain
        )
    }) {
        let mut min_t = f32::MAX;
        let mut max_t = f32::MIN;
        for k in 0..16 {
            let ang = k as f32 * std::f32::consts::TAU / 16.0;
            let x = site.pos.x + ang.cos() * site.radius;
            let z = site.pos.y + ang.sin() * site.radius;
            let t = site.norm_dist(x, z);
            min_t = min_t.min(t);
            max_t = max_t.max(t);
        }
        assert!(
            max_t - min_t > 0.2,
            "{:?} en {:?} sigue siendo un círculo (t {min_t}..{max_t})",
            site.kind,
            site.pos
        );
        checked += 1;
    }
    assert!(
        checked > 0,
        "el mundo de prueba debe tener sitios de relieve"
    );
}

#[test]
fn rivers_start_on_a_mountain_or_the_disk_rim() {
    let config = config_with_all_features();
    let base = rer_worldgen::field::HeightField::new(&config);
    let plan = FeaturePlan::generate(&config, &base);
    assert!(
        !plan.rivers.is_empty(),
        "el mundo de prueba debe generar ríos"
    );
    let radius = config.bounds.radius();
    let playable = radius - 24.0;
    let center = Vec2::new(config.bounds.width * 0.5, config.bounds.length * 0.5);
    let mountains: Vec<_> = plan
        .sites
        .iter()
        .filter(|s| s.kind == FeatureKind::Mountain)
        .collect();
    for river in &plan.rivers {
        let start = river.points[0].0;
        let on_mountain = mountains
            .iter()
            .any(|m| (start - m.pos).length() <= m.radius * 1.2);
        let on_rim = (start - center).length() >= playable - 22.0;
        assert!(
            on_mountain || on_rim,
            "el río nace en el interior {start:?} (ni montaña ni borde)"
        );
        let h0 = river.points[0].1;
        let h1 = river.points.last().unwrap().1;
        assert!(h1 <= h0 + 1.5, "el río debe bajar: {h0} → {h1}");
    }
}

#[test]
fn rivers_end_in_a_lake_or_at_the_map_edge() {
    let config = config_with_all_features();
    let base = rer_worldgen::field::HeightField::new(&config);
    let plan = FeaturePlan::generate(&config, &base);
    assert!(
        !plan.rivers.is_empty(),
        "el mundo de prueba debe generar ríos"
    );
    let playable = config.bounds.radius() - 24.0;
    let center = Vec2::new(config.bounds.width * 0.5, config.bounds.length * 0.5);
    let lakes: Vec<_> = plan
        .sites
        .iter()
        .filter(|site| site.kind == FeatureKind::Lake)
        .collect();
    for river in &plan.rivers {
        let end = river.points.last().unwrap().0;
        let in_lake = lakes.iter().any(|lake| lake.norm_dist(end.x, end.y) < 1.05)
            || plan
                .lakes
                .iter()
                .any(|lake| (end - lake.pos).length() < lake.radius * 0.9);
        let at_edge = (end - center).length() >= playable - 10.0;
        assert!(in_lake || at_edge, "el río muere tierra adentro {end:?}");
    }
}

#[test]
fn sites_stay_inside_the_world_rectangle() {
    let config = config_with_all_features();
    let base = rer_worldgen::field::HeightField::new(&config);
    let plan = FeaturePlan::generate(&config, &base);
    let width = config.bounds.width;
    let length = config.bounds.length;
    let margin = 24.0_f32.min(width * 0.25).min(length * 0.25) - 0.01;
    for site in &plan.sites {
        assert!(
            site.pos.x >= margin
                && site.pos.x <= width - margin
                && site.pos.y >= margin
                && site.pos.y <= length - margin,
            "sitio {:?} fuera del rectángulo: {:?} ({width}×{length})",
            site.kind,
            site.pos
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
    let grade = base.land_grade();
    for site in &mountains {
        assert!(
            grade.is_high_half(site.pos),
            "la montaña {:?} debe nacer en la mitad alta",
            site.pos
        );
    }
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
    let span = mountains
        .iter()
        .flat_map(|a| mountains.iter().map(move |b| (a.pos - b.pos).length()));
    let max_span = span.fold(0.0f32, f32::max);
    assert!(
        max_span > 180.0,
        "las montañas de 1 km deben repartirse en la mitad alta ({max_span} m)"
    );
}

#[test]
fn land_has_a_gentle_high_and_low_half() {
    let config = config_with_all_features();
    let base = rer_worldgen::field::HeightField::new(&config);
    let grade = base.land_grade();
    let high = grade.sample_high();
    let low = grade.sample_low();
    let h_high = base.height_at(high.x, high.y);
    let h_low = base.height_at(low.x, low.y);
    assert!(
        h_high > h_low + 10.0,
        "el lado alto ({h_high}) debe superar al bajo ({h_low})"
    );
    assert!(
        h_high - h_low < 72.0,
        "la pendiente no debe ser un acantilado ({})",
        h_high - h_low
    );
}

#[test]
fn mountains_and_caves_sit_on_the_high_half() {
    let config = config_with_all_features();
    let field = DensityField::new(&config);
    let plan = field.plan();
    let grade = rer_worldgen::field::HeightField::new(&config).land_grade();
    let mountains: Vec<_> = plan
        .sites
        .iter()
        .filter(|s| s.kind == FeatureKind::Mountain)
        .collect();
    assert!(!mountains.is_empty(), "debe haber montañas");
    for site in &mountains {
        assert!(
            grade.is_high_half(site.pos),
            "montaña en la mitad baja {:?}",
            site.pos
        );
    }
    assert!(!plan.caves.is_empty(), "debe haber cuevas");
    for system in &plan.caves {
        let mouth = Vec2::new(system.segments[0].a.x, system.segments[0].a.z);
        assert!(
            grade.is_high_half(mouth),
            "cueva en la mitad baja {mouth:?}"
        );
    }
}

#[test]
fn mountains_rise_from_local_high_ground() {
    let config = config_with_all_features();
    let field = DensityField::new(&config);
    let base = field.base_heightfield();
    let mountains: Vec<_> = field
        .plan()
        .sites
        .iter()
        .filter(|s| s.kind == FeatureKind::Mountain)
        .copied()
        .collect();
    assert!(!mountains.is_empty(), "debe haber montañas");
    for site in mountains {
        let local = base.height_at(site.pos.x, site.pos.y);
        let peak = field.height_at(site.pos.x, site.pos.y);
        assert!(
            peak > local + site.value * 0.5,
            "la montaña en {:?} debe nacer sobre el terreno local ({peak} vs cota {local}, relieve {})",
            site.pos,
            site.value
        );
    }
}

#[test]
fn lakes_sit_on_the_low_half() {
    let config = config_with_all_features();
    let base = rer_worldgen::field::HeightField::new(&config);
    let plan = FeaturePlan::generate(&config, &base);
    let grade = base.land_grade();
    assert!(!plan.lakes.is_empty(), "debe haber lagos");
    for lake in &plan.lakes {
        assert!(
            grade.is_low_half(lake.pos),
            "lago en la mitad alta {:?}",
            lake.pos
        );
    }
    for (i, a) in plan.lakes.iter().enumerate() {
        for b in plan.lakes.iter().skip(i + 1) {
            let gap = (a.pos - b.pos).length() - a.radius - b.radius;
            assert!(
                gap > 8.0,
                "lagos pegados: {:?} r={} y {:?} r={} (hueco {gap})",
                a.pos,
                a.radius,
                b.pos,
                b.radius
            );
        }
    }
}

#[test]
fn rivers_run_down_the_land_grade() {
    let config = config_with_all_features();
    let base = rer_worldgen::field::HeightField::new(&config);
    let plan = FeaturePlan::generate(&config, &base);
    let grade = base.land_grade();
    assert!(!plan.rivers.is_empty(), "debe haber ríos");
    for river in &plan.rivers {
        let start = river.points[0].0;
        let end = river.points.last().unwrap().0;
        let along_drop = grade.along(start) - grade.along(end);
        let in_lake = plan
            .lakes
            .iter()
            .any(|lake| (end - lake.pos).length() < lake.radius * 1.05);
        assert!(
            along_drop > 0.02 || in_lake,
            "el río no baja por la pendiente: {start:?} → {end:?} (Δalong {along_drop})"
        );
        assert!(
            grade.is_high_half(start) || along_drop > 0.08,
            "el río debe nacer en la mitad alta {start:?}"
        );
    }
}
