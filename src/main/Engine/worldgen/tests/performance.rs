//! F9: presupuesto de rendimiento por chunk (se verifica estrictamente en
//! release; en debug el presupuesto es holgado).

use std::time::Instant;

use rer_worldgen::chunks::{ChunkKey, sample_chunk_grid_lod};
use rer_worldgen::config::WorldGenConfig;
use rer_worldgen::field::DensityField;
use rer_worldgen::mesher::mesh_chunk;

fn perf_config() -> WorldGenConfig {
    let mut config = WorldGenConfig {
        seed: 8_765,
        ..WorldGenConfig::default()
    };
    config.bounds.width = 512.0;
    config.bounds.length = 512.0;
    config.bounds.height = 128.0;
    config
}

#[test]
fn full_chunk_generation_within_budget() {
    let config = perf_config();
    let field = DensityField::new(&config);

    // LOD 0 (peor caso): sampling 34³ + meshing completo.
    let start = Instant::now();
    let mesh = mesh_chunk(sample_chunk_grid_lod(
        &field,
        &config,
        ChunkKey { x: 3, y: 1, z: 2 },
        0,
    ));
    let elapsed = start.elapsed();
    assert!(mesh.triangle_count() > 0, "el chunk debe tener geometría");
    let budget = if cfg!(debug_assertions) {
        std::time::Duration::from_millis(2_500)
    } else {
        std::time::Duration::from_millis(60)
    };
    assert!(
        elapsed < budget,
        "chunk 32³ tardó {elapsed:?} (presupuesto {budget:?})"
    );

    // LOD 2: 8³ celdas, coste ~1/64 del sampling.
    let start = Instant::now();
    let mesh_lod2 = mesh_chunk(sample_chunk_grid_lod(
        &field,
        &config,
        ChunkKey { x: 3, y: 1, z: 2 },
        2,
    ));
    let elapsed_lod2 = start.elapsed();
    assert!(mesh_lod2.triangle_count() > 0);
    let budget_lod2 = if cfg!(debug_assertions) {
        std::time::Duration::from_millis(600)
    } else {
        std::time::Duration::from_millis(15)
    };
    assert!(
        elapsed_lod2 < budget_lod2,
        "chunk LOD2 tardó {elapsed_lod2:?} (presupuesto {budget_lod2:?})"
    );
}

#[test]
fn world_create_with_all_features_within_budget() {
    use rer_worldgen::planner::FeaturePlan;

    let config = perf_config();
    let start = Instant::now();
    let base = rer_worldgen::field::HeightField::new(&config);
    let _plan = FeaturePlan::generate(&config, &base);
    let _field = DensityField::new(&config);
    let elapsed = start.elapsed();
    let budget = if cfg!(debug_assertions) {
        std::time::Duration::from_millis(1_500)
    } else {
        std::time::Duration::from_millis(150)
    };
    assert!(
        elapsed < budget,
        "world_create (plan + índices) tardó {elapsed:?} (presupuesto {budget:?})"
    );
}
