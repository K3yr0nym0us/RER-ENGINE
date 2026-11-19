//! Valores de referencia de la geometría generada por el mesher.
//!
//! Estos hashes detectan cualquier cambio accidental en la salida: orden de
//! vértices, normales, UVs o índices debe permanecer estable para un mismo
//! campo de densidad. Tras dejar la normalización al shader, los hashes de
//! normales cambiaron una sola vez y se registraron aquí como nueva referencia.

use rer_worldgen::chunks::{ChunkKey, sample_chunk_grid, sample_chunk_grid_lod};
use rer_worldgen::config::WorldGenConfig;
use rer_worldgen::field::DensityField;
use rer_worldgen::mesher::mesh_chunk;

#[test]
fn mesh_geometry_matches_reference() {
    let config = WorldGenConfig::default();
    let field = DensityField::new(&config);

    let cases = [
        (
            ChunkKey { x: 3, y: 0, z: 2 },
            0u32,
            0x32ed_fb79_0ebb_93c8u64,
            2170usize,
        ),
        (ChunkKey { x: 7, y: 1, z: 5 }, 0, 0xcbf2_9ce4_8422_2325, 0),
        (ChunkKey { x: 3, y: 0, z: 2 }, 2, 0x073c_bdb1_1a14_8c49, 192),
    ];

    for (chunk, lod, expected_hash, expected_tris) in cases {
        let mesh = mesh_chunk(sample_chunk_grid_lod(&field, &config, chunk, lod));
        assert_eq!(
            mesh.geometry_hash(),
            expected_hash,
            "hash de geometría inesperado en chunk ({},{},{}) lod={}",
            chunk.x,
            chunk.y,
            chunk.z,
            lod
        );
        assert_eq!(
            mesh.triangle_count(),
            expected_tris,
            "número de triángulos inesperado en chunk ({},{},{}) lod={}",
            chunk.x,
            chunk.y,
            chunk.z,
            lod
        );
    }
}

#[test]
fn normals_are_left_unnormalized() {
    let config = WorldGenConfig::default();
    let field = DensityField::new(&config);
    let mesh = mesh_chunk(sample_chunk_grid(
        &field,
        &config,
        ChunkKey { x: 3, y: 0, z: 2 },
    ));

    assert!(!mesh.normals.is_empty());
    assert!(
        mesh.normals.iter().all(|n| n.iter().all(|v| v.is_finite())),
        "ninguna normal debe contener NaN o infinitos"
    );
    assert!(
        mesh.normals.iter().any(|n| {
            let len2 = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
            (len2 - 1.0).abs() > 1.0e-6
        }),
        "las normales compartidas deben conservar la suma sin normalizar"
    );
}
