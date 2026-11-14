//! F1: validación de la configuración (invariantes de dimensiones, máximos y rangos).

use rer_worldgen::chunks::CHUNK_SIZE;
use rer_worldgen::config::*;

fn valid_base() -> WorldGenConfig {
    WorldGenConfig {
        seed: 777,
        ..WorldGenConfig::default()
    }
}

#[test]
fn default_config_is_valid() {
    assert!(valid_base().validate().is_ok());
}

#[test]
fn bounds_out_of_range_are_rejected() {
    let mut config = valid_base();
    config.bounds.width = 16.0; // < MIN_DIM
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidField { .. })
    ));

    let mut config = valid_base();
    config.bounds.height = 20_000.0;
    assert!(config.validate().is_err());
}

#[test]
fn max_count_is_a_hard_limit() {
    let mut config = valid_base();
    config.lakes.base.max_count = MAX_FEATURE_COUNT + 1;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidField { .. })
    ));

    let mut config = valid_base();
    config.lakes.base.max_count = MAX_FEATURE_COUNT;
    assert!(config.validate().is_ok());
}

#[test]
fn size_invariants_are_checked() {
    let mut config = valid_base();
    config.hills.base.min_size = 150.0;
    config.hills.base.max_size = 100.0; // min > max
    assert!(config.validate().is_err());

    let mut config = valid_base();
    config.rivers.length_min = 50.0;
    config.rivers.length_max = 20.0;
    assert!(config.validate().is_err());

    let mut config = valid_base();
    config.caves.room_chance = 1.5;
    assert!(config.validate().is_err());

    let mut config = valid_base();
    config.caves.segments = 0;
    assert!(config.validate().is_err());
}

#[test]
fn voxel_size_is_bounded() {
    let mut config = valid_base();
    config.resolution.voxel_size = 8.0;
    assert!(config.validate().is_err());
    config.resolution.voxel_size = 0.5;
    assert!(config.validate().is_ok());
}

#[test]
fn disabled_features_skip_their_invariants() {
    let mut config = valid_base();
    config.mountains.base.enabled = false;
    config.mountains.max_height = -5.0; // inválido, pero deshabilitado => no importa
    assert!(config.validate().is_ok());
}

#[test]
fn config_roundtrips_camel_case_json() {
    let json = serde_json::json!({
        "seed": 42,
        "bounds": { "width": 256.0, "length": 256.0, "height": 96.0 },
        "resolution": { "voxelSize": 1.0, "lodEnabled": false },
        "mountains": {
            "enabled": true, "maxCount": 15, "minSize": 60.0, "maxSize": 180.0,
            "minSeparation": 120.0, "maxHeight": 70.0, "sharpness": 3.0
        },
        "lakes": {
            "enabled": true, "maxCount": 8, "minSize": 20.0, "maxSize": 90.0,
            "minSeparation": 100.0, "maxDepth": 10.0
        },
        "rivers": { "enabled": true, "maxCount": 5, "lengthMin": 150.0, "lengthMax": 600.0, "width": 4.0, "depth": 3.0 },
        "caves": {
            "enabled": true, "maxCount": 20, "depthMin": 10.0, "depthMax": 50.0,
            "radius": 3.0, "roomChance": 0.2, "segments": 18
        },
        "biomes": { "forest": true, "desert": false, "grassland": true, "snow": false, "volcanic": false }
    });
    let config: WorldGenConfig = serde_json::from_value(json).expect("JSON camelCase debe parsear");
    assert_eq!(config.seed, 42);
    assert_eq!(config.mountains.base.max_count, 15);
    assert_eq!(config.caves.room_chance, 0.2);
    assert!(config.validate().is_ok());
    // Defaults para lo no especificado: hills activada, plains/cliffs desactivados,
    // cascadas con su máximo por defecto.
    assert!(config.hills.base.enabled);
    assert!(!config.cliffs.base.enabled);
    assert_eq!(config.waterfalls.max_count, 3);
}

#[test]
fn chunk_accounting_matches_bounds() {
    let mut config = valid_base();
    config.bounds.width = 512.0;
    config.bounds.length = 384.0;
    config.bounds.height = 128.0;
    config.resolution.voxel_size = 1.0;
    let (cx, cz) = config.chunk_count_xz(CHUNK_SIZE);
    assert_eq!((cx, cz), (16, 12));
    assert_eq!(config.max_chunk_y(CHUNK_SIZE), 4);
}
