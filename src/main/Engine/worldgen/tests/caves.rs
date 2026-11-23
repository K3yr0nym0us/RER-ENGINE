//! F4: cuevas y sistemas subterráneos — bocas en superficie, túneles excavados,
//! gates de profundidad y salas.

use rer_worldgen::config::*;
use rer_worldgen::field::DensityField;
use rer_worldgen::planner::FeaturePlan;

fn near_entrance_corridor(system: &rer_worldgen::planner::CaveSystem, x: f32, z: f32) -> bool {
    system.segments.iter().any(|segment| {
        if !segment.entrance {
            return false;
        }
        let abx = segment.b.x - segment.a.x;
        let abz = segment.b.z - segment.a.z;
        let len2 = abx * abx + abz * abz;
        let t = if len2 <= f32::EPSILON {
            0.0
        } else {
            ((x - segment.a.x) * abx + (z - segment.a.z) * abz) / len2
        }
        .clamp(0.0, 1.0);
        let dx = x - (segment.a.x + abx * t);
        let dz = z - (segment.a.z + abz * t);
        (dx * dx + dz * dz).sqrt() < segment.radius + 2.5
    })
}

fn near_any_entrance(caves: &[rer_worldgen::planner::CaveSystem], x: f32, z: f32) -> bool {
    caves
        .iter()
        .any(|system| near_entrance_corridor(system, x, z))
}

fn cave_config() -> WorldGenConfig {
    let mut config = WorldGenConfig {
        seed: 1234,
        ..WorldGenConfig::default()
    };
    config.bounds.width = 512.0;
    config.bounds.length = 512.0;
    config.bounds.height = 128.0;
    config.mountains.base.enabled = true;
    config.mountains.base.max_count = 4;
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
        let segment = system.segments.last().expect("túnel con tramos");
        let mid = segment.a.lerp(segment.b, 0.5);
        let surface = field.height_at(mid.x, mid.z);
        let depth = surface - mid.y;
        if segment.entrance {
            continue;
        }
        assert!(
            depth > segment.radius + 0.4,
            "el tramo final debe quedar bajo tierra ({depth} m, radio {})",
            segment.radius
        );
        if system.segments.len() >= 10 {
            assert!(
                depth > config.caves.depth_min * 0.4,
                "tramo insuficientemente profundo ({depth} m)"
            );
        }
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
        for segment in system.segments.iter().skip(8).filter(|s| !s.entrance) {
            let mid = segment.a.lerp(segment.b, 0.5);
            if near_any_entrance(&plan.caves, mid.x, mid.z) {
                continue;
            }
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
        let end = system.segments.last().expect("túnel con tramos").b;
        assert!(
            end.y < first.a.y - 6.0,
            "el túnel debe descender ({end:?} desde {:?})",
            first.a
        );
    }
    assert!(entrances > 0);
}

#[test]
fn caves_descend_into_the_mass() {
    let config = cave_config();
    let field = DensityField::new(&config);
    let plan = FeaturePlan::generate(&config, field.base_heightfield());
    assert!(
        !plan.caves.is_empty(),
        "el mundo de prueba debe generar cuevas"
    );
    let high: Vec<_> = plan
        .sites
        .iter()
        .filter(|site| {
            matches!(
                site.kind,
                rer_worldgen::planner::FeatureKind::Mountain
                    | rer_worldgen::planner::FeatureKind::Hill
                    | rer_worldgen::planner::FeatureKind::Cliff
            )
        })
        .collect();
    for system in &plan.caves {
        for segment in &system.segments {
            assert!(
                segment.b.y <= segment.a.y + 0.2,
                "la cueva no puede subir ({:?} -> {:?})",
                segment.a,
                segment.b
            );
        }
        for segment in system.segments.iter().filter(|s| !s.entrance) {
            let mid = segment.a.lerp(segment.b, 0.5);
            if near_any_entrance(&plan.caves, mid.x, mid.z) {
                continue;
            }
            let surface = field.height_at(mid.x, mid.z);
            let cover = surface - mid.y;
            if cover < config.caves.radius + 0.6 {
                continue;
            }
            let d = field.density_at(mid.x, surface - 0.4, mid.z);
            assert!(
                d > 0.0,
                "la cueva no debe asomar por la ladera (densidad {d})"
            );
        }
        let mouth = glam::Vec2::new(system.segments[0].a.x, system.segments[0].a.z);
        let end = system.segments.last().unwrap().b;
        let end_xz = glam::Vec2::new(end.x, end.z);
        let drift = (end_xz - mouth).length();
        assert!(
            drift < 80.0,
            "la cueva debe bajar con poca deriva XZ ({drift} m, host {:?})",
            high.iter()
                .find(|site| (mouth - site.pos).length() <= site.radius * 1.2)
                .map(|site| site.radius)
        );
    }
}

#[test]
fn caves_stay_open_along_the_path() {
    let config = cave_config();
    let field = DensityField::new(&config);
    let plan = FeaturePlan::generate(&config, field.base_heightfield());
    assert!(
        !plan.caves.is_empty(),
        "el mundo de prueba debe generar cuevas"
    );
    for system in &plan.caves {
        for (index, segment) in system.segments.iter().enumerate() {
            if segment.entrance {
                continue;
            }
            let mid = segment.a.lerp(segment.b, 0.5);
            let d = field.density_at(mid.x, mid.y, mid.z);
            assert!(
                d < 0.0,
                "el tramo {index} debe quedar hueco (densidad {d}, y={})",
                mid.y
            );
        }
    }
}

#[test]
fn caves_descend_steeply_and_end_in_a_cavern() {
    let config = cave_config();
    let field = DensityField::new(&config);
    let plan = FeaturePlan::generate(&config, field.base_heightfield());
    assert!(
        !plan.caves.is_empty(),
        "el mundo de prueba debe generar cuevas"
    );
    for system in &plan.caves {
        let first_r = system.segments[0].radius;
        let last_r = system.segments.last().expect("túnel con tramos").radius;
        assert!(
            last_r + 0.05 >= first_r,
            "el túnel debe ensancharse hacia dentro ({first_r} → {last_r})"
        );
        let mouth = system.segments[0].a;
        let end = system.segments.last().unwrap().b;
        let horiz = ((end.x - mouth.x).hypot(end.z - mouth.z)).max(0.01);
        let drop = (mouth.y - end.y).max(0.0);
        assert!(
            drop >= horiz * 0.4,
            "la cueva debe bajar más de lo que recorre en XZ ({drop} m vs {horiz} m)"
        );
        let cavern = system
            .rooms
            .iter()
            .max_by(|a, b| a.radius.total_cmp(&b.radius));
        let cavern = cavern.expect("cada cueva termina en una sala");
        assert!(
            (6.0..13.0).contains(&cavern.radius),
            "la sala terminal debe ser compacta ({})",
            cavern.radius
        );
        assert!(
            (cavern.center.y - cavern.radius) <= end.y + 1.0,
            "la sala gigante debe quedar al fondo del túnel"
        );
    }
}

#[test]
fn caves_start_on_high_ground() {
    let config = cave_config();
    let field = DensityField::new(&config);
    let plan = FeaturePlan::generate(&config, field.base_heightfield());
    assert!(
        !plan.caves.is_empty(),
        "el mundo de prueba debe generar cuevas"
    );
    let floor = rer_worldgen::config::plains_height() + 6.0;
    let high: Vec<_> = plan
        .sites
        .iter()
        .filter(|site| {
            matches!(
                site.kind,
                rer_worldgen::planner::FeatureKind::Mountain
                    | rer_worldgen::planner::FeatureKind::Hill
                    | rer_worldgen::planner::FeatureKind::Cliff
            )
        })
        .collect();
    for system in &plan.caves {
        let mouth = system.segments[0].a;
        let surface = field.height_at(mouth.x, mouth.z);
        let on_high_site = high.iter().any(|site| {
            (glam::Vec2::new(mouth.x, mouth.z) - site.pos).length() <= site.radius * 1.15
        });
        assert!(
            surface >= floor || on_high_site,
            "la boca debe nacer en zona alta ({surface} m, piso {floor})"
        );
    }
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

#[test]
fn y_pruning_reaches_below_the_surface_when_caves_are_deep() {
    use rer_worldgen::chunks::active_y_chunks;

    let mut config = WorldGenConfig::default();
    config.bounds.width = 256.0;
    config.bounds.length = 256.0;
    config.bounds.height = 256.0;
    config.resolution.voxel_size = 1.0;
    config.mountains.base.enabled = false;
    config.hills.base.enabled = false;
    config.caves.enabled = false;
    let field = DensityField::new(&config);
    let shallow = active_y_chunks(&field, &config, 1, 1);
    config.caves.enabled = true;
    config.caves.depth_max = 50.0;
    let deep = active_y_chunks(&field, &config, 1, 1);
    assert_eq!(
        shallow.first().copied(),
        Some(0),
        "el suelo a unos metros ya ocupa la primera capa ({shallow:?})"
    );
    let deep_first = deep.first().copied().expect("columna con cuevas");
    assert!(
        deep_first < 0,
        "una cueva profunda debe abrir capas bajo el piso ({deep:?})"
    );
}

fn river_clearance(pos: glam::Vec2, river: &rer_worldgen::planner::RiverPath) -> f32 {
    let mut best = f32::MAX;
    for pair in river.points.windows(2) {
        let a = pair[0].0;
        let b = pair[1].0;
        let ab = b - a;
        let len2 = ab.length_squared();
        let t = if len2 <= f32::EPSILON {
            0.0
        } else {
            ((pos - a).dot(ab) / len2).clamp(0.0, 1.0)
        };
        best = best.min((pos - (a + ab * t)).length());
    }
    best
}

#[test]
fn caves_do_not_overlap_rivers() {
    let mut config = cave_config();
    config.rivers.enabled = true;
    config.rivers.max_count = 4;
    let field = DensityField::new(&config);
    let plan = FeaturePlan::generate(&config, field.base_heightfield());
    if plan.rivers.is_empty() || plan.caves.is_empty() {
        return;
    }
    let gap = config.caves.radius + 4.0;
    for system in &plan.caves {
        for segment in &system.segments {
            let pos = glam::Vec2::new(segment.a.x, segment.a.z);
            for river in &plan.rivers {
                let d = river_clearance(pos, river);
                assert!(
                    d > river.width + gap,
                    "cueva a {d} m de un río (ancho {} + hueco {gap})",
                    river.width
                );
            }
        }
    }
}

#[test]
fn cave_interior_mesh_marks_rock_uv() {
    use rer_worldgen::chunks::{CHUNK_SIZE, ChunkKey, sample_chunk_grid};
    use rer_worldgen::mesher::{CAVE_INTERIOR_UV_OFFSET, mesh_chunk};

    let config = cave_config();
    let field = DensityField::new(&config);
    let plan = FeaturePlan::generate(&config, field.base_heightfield());
    let voxel = config.resolution.voxel_size;
    let cell = CHUNK_SIZE as f32 * voxel;
    let mut meshed = 0usize;
    let mut marked = 0usize;
    'caves: for system in &plan.caves {
        for segment in system.segments.iter().filter(|segment| !segment.entrance) {
            let point = segment.a.lerp(segment.b, 0.5);
            if near_any_entrance(&plan.caves, point.x, point.z) {
                continue;
            }
            let key = ChunkKey {
                x: (point.x / cell).floor() as i32,
                y: (point.y / cell).floor() as i32,
                z: (point.z / cell).floor() as i32,
            };
            let mesh = mesh_chunk(sample_chunk_grid(&field, &config, key));
            if mesh.uvs.is_empty() {
                continue;
            }
            meshed += 1;
            marked = mesh
                .uvs
                .iter()
                .filter(|uv| uv[0] >= CAVE_INTERIOR_UV_OFFSET - 0.01)
                .count();
            if marked > 0 {
                break 'caves;
            }
        }
    }
    assert!(meshed > 0, "debe haber malla en un tramo interior");
    assert!(marked > 0, "el interior de la cueva debe marcar UV de roca");
}
