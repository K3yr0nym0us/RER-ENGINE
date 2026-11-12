//! F4: cuevas y sistemas subterráneos — bocas en superficie, túneles excavados,
//! gates de profundidad y salas.

use rer_worldgen::config::*;
use rer_worldgen::field::DensityField;
use rer_worldgen::planner::FeaturePlan;

fn cave_config() -> WorldGenConfig {
    let mut config = WorldGenConfig {
        seed: 1234,
        ..WorldGenConfig::default()
    };
    config.bounds.width = 512.0;
    config.bounds.length = 512.0;
    config.bounds.height = 128.0;
    config.mountains.base.enabled = false;
    config.hills.base.enabled = false;
    config.cliffs.base.enabled = true;
    config.cliffs.base.max_count = 6;
    config.lakes.base.enabled = false;
    config.rivers.enabled = false;
    config.caves.enabled = true;
    config.caves.max_count = 6;
    config.caves.depth_min = 14.0;
    config.caves.depth_max = 40.0;
    config.caves.radius = 3.0;
    config.caves.room_chance = 0.35;
    config.caves.segments = 18;
    config
}

#[test]
fn tunnel_interior_is_air_at_depth() {
    let config = cave_config();
    let field = DensityField::new(&config);
    let base = field.base_heightfield();
    let plan = FeaturePlan::generate(&config, base);
    assert!(
        !plan.caves.is_empty(),
        "el mundo de prueba debe generar cuevas"
    );

    for system in &plan.caves {
        // Punto medio de un tramo profundo (no de entrada).
        let segment = &system.segments[system.segments.len() / 2];
        let mid = segment.a.lerp(segment.b, 0.5);
        let surface = field.height_at(mid.x, mid.z);
        let depth = surface - mid.y;
        assert!(
            depth > config.caves.depth_min * 0.4,
            "tramo insuficientemente profundo"
        );
        let d = field.density_at(mid.x, mid.y, mid.z);
        assert!(
            d < 0.0,
            "el interior del túnel debe ser aire (densidad {d} a {depth:.1} m de profundidad)"
        );
    }
}

#[test]
fn entrance_opens_through_the_surface() {
    let config = cave_config();
    let field = DensityField::new(&config);
    let base = field.base_heightfield();
    let plan = FeaturePlan::generate(&config, base);

    for system in &plan.caves {
        // La boca: primer segmento, punto cercano a la superficie con plan.
        let first = &system.segments[0];
        let mouth = first.a.lerp(first.b, 0.25);
        let surface = field.height_at(mouth.x, mouth.z);
        // Justo bajo la superficie (0.5 m): el embudo debe haber perforado.
        let d = field.density_at(mouth.x, surface - 0.5, mouth.z);
        assert!(
            d < 0.0,
            "la boca de la cueva debe abrir la superficie (densidad {d})"
        );
    }
}

#[test]
fn deep_tunnels_do_not_perforate_surface() {
    let config = cave_config();
    let field = DensityField::new(&config);
    let base = field.base_heightfield();
    let plan = FeaturePlan::generate(&config, base);

    for system in &plan.caves {
        // Tramos no-entrada: la superficie directamente encima debe seguir sólida
        // (el gate de profundidad impide perforaciones espurias).
        for segment in system.segments.iter().skip(8) {
            let mid = segment.a.lerp(segment.b, 0.5);
            let surface = field.height_at(mid.x, mid.z);
            let depth = surface - mid.y;
            if depth < config.caves.depth_min * 0.6 {
                continue;
            }
            // A 0.5 m bajo la superficie: sin carving (gate = smoothstep(2.5, 6.5, 0.5) = 0).
            let d = field.density_at(mid.x, surface - 0.5, mid.z);
            assert!(
                d > 0.0,
                "la superficie sobre un túnel profundo debe seguir sólida (densidad {d})"
            );
        }
    }
}

#[test]
fn rooms_are_carved() {
    let config = cave_config();
    let field = DensityField::new(&config);
    let base = field.base_heightfield();
    let plan = FeaturePlan::generate(&config, base);

    let rooms: Vec<_> = plan.caves.iter().flat_map(|c| &c.rooms).collect();
    assert!(!rooms.is_empty(), "con room_chance=0.35 debe haber salas");
    for room in rooms {
        let surface = field.height_at(room.center.x, room.center.z);
        let depth = surface - room.center.y;
        if depth < 10.0 {
            continue; // gate de sala (smoothstep 5..10): solo salas profundas.
        }
        let d = field.density_at(room.center.x, room.center.y, room.center.z);
        assert!(
            d < 0.0,
            "el interior de la sala debe ser aire (densidad {d})"
        );
    }
}

#[test]
fn caves_respect_max_count_and_depth() {
    let config = cave_config();
    let field = DensityField::new(&config);
    let base = field.base_heightfield();
    let plan = FeaturePlan::generate(&config, base);
    assert!(plan.caves.len() <= config.caves.max_count as usize);
    let mut entrances = 0;
    for system in &plan.caves {
        let first = &system.segments[0];
        // Nace pegado a la superficie CON PLAN (sitios pueden rebajar el terreno).
        let plan_surface = field.height_at(first.a.x, first.a.z);
        assert!(
            (first.a.y - (plan_surface - 0.3)).abs() < 1.0,
            "el túnel debe nacer en la superficie con plan ({} vs {})",
            first.a.y,
            plan_surface
        );
        entrances += 1;
        let deepest = system
            .segments
            .iter()
            .map(|s| plan_surface - s.b.y.min(s.a.y))
            .fold(0.0f32, f32::max);
        assert!(
            deepest <= config.caves.depth_max * 1.15 + 2.0,
            "túnel más profundo que depth_max: {deepest}"
        );
    }
    assert!(entrances > 0);
}

#[test]
fn cliff_overhangs_add_solid_above_terrain() {
    let config = cave_config();
    let field = DensityField::new(&config);
    let base = field.base_heightfield();
    let plan = FeaturePlan::generate(&config, base);
    let cliffs: Vec<_> = plan
        .sites
        .iter()
        .filter(|s| s.kind == rer_worldgen::planner::FeatureKind::Cliff)
        .collect();
    assert!(
        !cliffs.is_empty(),
        "el mundo de prueba debe generar acantilados"
    );

    // En la zona del acantilado, la roca aditiva debe crear sólido por encima
    // de la superficie natural (overhang) cerca de algún punto.
    let mut solid_above = 0;
    for site in &cliffs {
        for k in 0..32u32 {
            let angle = k as f32 * 0.196;
            let r = site.radius * 0.5 * (1.0 + (k % 3) as f32 * 0.25);
            let x = site.pos.x + angle.cos() * r;
            let z = site.pos.y + angle.sin() * r;
            let surface = field.height_at(x, z);
            let d = field.density_at(x, surface + 0.7, z);
            if d > 0.0 {
                solid_above += 1;
                break;
            }
        }
    }
    assert!(
        solid_above > 0,
        "los acantilados deben generar roca aditiva sobre la superficie (overhangs)"
    );
    let _ = base;
}

#[test]
fn carving_preserves_determinism_of_meshes() {
    use rer_worldgen::chunks::{ChunkKey, sample_chunk_grid};
    use rer_worldgen::mesher::mesh_chunk;

    let config = cave_config();
    let field = DensityField::new(&config);
    let chunk = ChunkKey { x: 4, y: 0, z: 3 };
    let a = mesh_chunk(sample_chunk_grid(&field, &config, chunk));
    let field_b = DensityField::new(&config);
    let b = mesh_chunk(sample_chunk_grid(&field_b, &config, chunk));
    assert_eq!(
        a.geometry_hash(),
        b.geometry_hash(),
        "el carving debe ser determinista entre instancias del campo"
    );
}
