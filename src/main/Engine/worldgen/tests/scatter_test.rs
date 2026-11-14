//! F7: distribución de blueprints (scatter) — reglas nunca violadas,
//! máximos respetados y determinismo.

use glam::Vec2;

use rer_worldgen::config::WorldGenConfig;
use rer_worldgen::field::DensityField;
use rer_worldgen::scatter::{ScatterCluster, ScatterRules, biome_name, blueprint_hash, scatter};

fn scatter_config() -> WorldGenConfig {
    let mut config = WorldGenConfig {
        seed: 31415,
        ..WorldGenConfig::default()
    };
    config.bounds.width = 512.0;
    config.bounds.length = 512.0;
    config.bounds.height = 128.0;
    config.biomes.grassland = true;
    config.biomes.forest = true;
    config.biomes.desert = true;
    config.biomes.snow = true;
    config.biomes.volcanic = false;
    config
}

fn base_rules() -> ScatterRules {
    ScatterRules {
        biomes: Vec::new(),
        altitude_min: 0.0,
        altitude_max: f32::MAX,
        max_slope_deg: 90.0,
        max_instances: 200,
        density: 1.0,
        scale_min: 1.0,
        scale_max: 1.0,
        random_rotation: true,
        min_distance: 6.0,
        cluster: ScatterCluster::default(),
    }
}

#[test]
fn scatter_is_deterministic_per_blueprint() {
    let config = scatter_config();
    let field = DensityField::new(&config);
    let rules = base_rules();
    let hash = blueprint_hash("model_tree_oak_01");
    let a = scatter(&field, &config, &rules, hash);
    let b = scatter(&field, &config, &rules, hash);
    assert_eq!(a, b, "mismo seed+reglas ⇒ mismas colocaciones");
    // Blueprints distintos ⇒ secuencias distintas (mayormente).
    let other = scatter(&field, &config, &rules, blueprint_hash("model_rock_02"));
    let same_positions = a
        .iter()
        .zip(&other)
        .filter(|(p, q)| p.position == q.position)
        .count();
    assert!(
        same_positions < a.len() / 2,
        "blueprints distintos deben descorrelacionarse ({same_positions} iguales)"
    );
}

#[test]
fn max_instances_is_a_hard_limit() {
    let config = scatter_config();
    let field = DensityField::new(&config);
    let mut rules = base_rules();
    rules.max_instances = 50;
    let out = scatter(&field, &config, &rules, blueprint_hash("bp"));
    assert!(out.len() <= 50, "el scatter devolvió {} (> 50)", out.len());
    rules.max_instances = 10_000;
    rules.density = 1.0;
    let out = scatter(&field, &config, &rules, blueprint_hash("bp"));
    assert!(out.len() <= 10_000);
}

#[test]
fn min_distance_is_respected() {
    let config = scatter_config();
    let field = DensityField::new(&config);
    let mut rules = base_rules();
    rules.min_distance = 10.0;
    rules.max_instances = 400;
    let out = scatter(&field, &config, &rules, blueprint_hash("bp"));
    assert!(
        out.len() >= 50,
        "el mundo debería admitir >50 objetos a 10 m"
    );
    for (i, a) in out.iter().enumerate() {
        for b in out.iter().skip(i + 1) {
            let d =
                Vec2::new(a.position[0] - b.position[0], a.position[2] - b.position[2]).length();
            assert!(
                d >= 10.0 - 0.01,
                "colocaciones a {d} m violan la distancia mínima de 10 m"
            );
        }
    }
}

#[test]
fn slope_constraint_is_respected() {
    let config = scatter_config();
    let field = DensityField::new(&config);
    let mut rules = base_rules();
    rules.max_slope_deg = 15.0; // Solo zonas llanas.
    rules.max_instances = 500;
    let out = scatter(&field, &config, &rules, blueprint_hash("bp"));
    assert!(!out.is_empty());
    for p in &out {
        let eps = 2.0;
        let gx = (field.height_at(p.position[0] + eps, p.position[2])
            - field.height_at(p.position[0] - eps, p.position[2]))
            / (2.0 * eps);
        let gz = (field.height_at(p.position[0], p.position[2] + eps)
            - field.height_at(p.position[0], p.position[2] - eps))
            / (2.0 * eps);
        let slope = gx.hypot(gz).atan().to_degrees();
        assert!(
            slope <= 15.0 + 0.5,
            "colocación en pendiente {slope:.1}° (> 15°)"
        );
    }
}

#[test]
fn altitude_constraint_is_respected() {
    let config = scatter_config();
    let field = DensityField::new(&config);
    let mut rules = base_rules();
    rules.altitude_max = 50.0;
    rules.max_instances = 500;
    let out = scatter(&field, &config, &rules, blueprint_hash("bp"));
    assert!(!out.is_empty());
    for p in &out {
        assert!(
            p.position[1] <= 50.0,
            "colocación a altitud {} (> 50 m)",
            p.position[1]
        );
    }
}

#[test]
fn biome_constraint_is_respected() {
    let config = scatter_config();
    let field = DensityField::new(&config);
    let mut rules = base_rules();
    rules.biomes = vec![biome_name(rer_worldgen::biomes::Biome::Snow).to_string()];
    rules.max_instances = 500;
    let out = scatter(&field, &config, &rules, blueprint_hash("bp"));
    for p in &out {
        let biome = rer_worldgen::biomes::BiomeMap::new(&config).biome_at(
            p.position[0],
            p.position[2],
            p.position[1],
        );
        assert_eq!(
            biome,
            rer_worldgen::biomes::Biome::Snow,
            "colocación fuera del bioma permitido en ({}, {})",
            p.position[0],
            p.position[2]
        );
    }
}

#[test]
fn scale_and_rotation_follow_rules() {
    let config = scatter_config();
    let field = DensityField::new(&config);
    let mut rules = base_rules();
    rules.scale_min = 0.8;
    rules.scale_max = 1.25;
    rules.random_rotation = false;
    rules.max_instances = 100;
    let out = scatter(&field, &config, &rules, blueprint_hash("bp"));
    for p in &out {
        let s = p.scale[0];
        assert!((0.78..=1.27).contains(&s), "escala {s} fuera de rango");
        assert_eq!(p.scale[0], p.scale[1]);
        assert_eq!(p.scale[1], p.scale[2]);
        assert_eq!(p.yaw, 0.0, "random_rotation=false ⇒ yaw 0");
    }
}

#[test]
fn cluster_mode_places_grouped_instances() {
    let config = scatter_config();
    let field = DensityField::new(&config);
    let mut rules = base_rules();
    rules.max_instances = 120;
    rules.cluster = ScatterCluster {
        enabled: true,
        size: 12,
        radius: 30.0,
    };
    let out = scatter(&field, &config, &rules, blueprint_hash("bp"));
    assert!(!out.is_empty());
    // Con clústeres, las distancias medias son menores que en modo uniforme
    // (heurística robusta: hay pares muy cercanos).
    let mut min_pair = f32::MAX;
    for (i, a) in out.iter().enumerate() {
        for b in out.iter().skip(i + 1) {
            let d =
                Vec2::new(a.position[0] - b.position[0], a.position[2] - b.position[2]).length();
            min_pair = min_pair.min(d);
        }
    }
    let mut rules_uniform = rules.clone();
    rules_uniform.cluster.enabled = false;
    let out_uniform = scatter(&field, &config, &rules_uniform, blueprint_hash("bp"));
    let mut min_pair_uniform = f32::MAX;
    for (i, a) in out_uniform.iter().enumerate() {
        for b in out_uniform.iter().skip(i + 1) {
            let d =
                Vec2::new(a.position[0] - b.position[0], a.position[2] - b.position[2]).length();
            min_pair_uniform = min_pair_uniform.min(d);
        }
    }
    assert!(
        min_pair <= min_pair_uniform,
        "el modo clúster debe concentrar más los objetos ({min_pair} vs {min_pair_uniform})"
    );
}
