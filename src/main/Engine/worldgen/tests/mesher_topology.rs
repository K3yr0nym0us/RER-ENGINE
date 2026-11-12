//! F2: topología de Marching Cubes (cierre de malla), determinismo y sellado de
//! costuras entre chunks vecinos.

use std::collections::HashMap;

use glam::Vec3;

use rer_worldgen::chunks::{CHUNK_SIZE, ChunkKey, sample_chunk_grid};
use rer_worldgen::config::WorldGenConfig;
use rer_worldgen::field::DensityField;
use rer_worldgen::mesher::{DensityGrid, MeshBuffer, mesh_chunk};

/// Campo de esfera centrado en el centro del chunk, con aire garantizado en el
/// borde del grid: produce una malla cerrada (superficie de esfera).
fn sphere_grid(radius: f32, center: Vec3, voxel: f32) -> DensityGrid {
    let dim = CHUNK_SIZE + 3; // CHUNK_POINTS + 2 (pad 1)
    let origin = center - Vec3::splat((dim as f32 - 1.0) * voxel / 2.0);
    let mut data = Vec::with_capacity((dim * dim * dim) as usize);
    for pz in 0..dim {
        for py in 0..dim {
            for px in 0..dim {
                let p = origin + Vec3::new(px as f32, py as f32, pz as f32) * voxel;
                data.push(p.distance(center) - radius);
            }
        }
    }
    DensityGrid {
        dim,
        pad: 1,
        data,
        origin,
        voxel_size: voxel,
    }
}

/// Cuenta las veces que se usa cada arista no dirigida de la malla.
/// En una malla cerrada y consistente, cada arista se usa exactamente 2 veces.
fn edge_usage(mesh: &MeshBuffer) -> HashMap<(u32, u32), usize> {
    let mut usage: HashMap<(u32, u32), usize> = HashMap::new();
    for tri in mesh.indices.chunks_exact(3) {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            // El índice (a,b) solo es compartido si las posiciones coinciden; en un
            // chunk deduplicado los índices compartidos ya son los mismos, así que
            // usamos el par no dirigido de índices directamente.
            let key = if a < b { (a, b) } else { (b, a) };
            *usage.entry(key).or_default() += 1;
        }
    }
    usage
}

#[test]
fn sphere_mesh_is_closed_manifold() {
    let grid = sphere_grid(10.0, Vec3::splat(16.0), 1.0);
    let mesh = mesh_chunk(grid);
    assert!(
        mesh.triangle_count() > 100,
        "la esfera debe tener triángulos, tiene {}",
        mesh.triangle_count()
    );

    let usage = edge_usage(&mesh);
    let bad: Vec<_> = usage.iter().filter(|&(_, &count)| count != 2).collect();
    assert!(
        bad.is_empty(),
        "malla no cerrada: {} aristas con uso != 2 (ejemplos: {:?})",
        bad.len(),
        &bad[..bad.len().min(8)]
    );
}

#[test]
fn mesh_is_deterministic_per_input() {
    let config = WorldGenConfig::default();
    let field = DensityField::new(&config);
    let chunk = ChunkKey { x: 3, y: 0, z: 2 };
    let a = mesh_chunk(sample_chunk_grid(&field, &config, chunk));
    let b = mesh_chunk(sample_chunk_grid(&field, &config, chunk));
    assert_eq!(
        a.geometry_hash(),
        b.geometry_hash(),
        "mismo input => mismo hash de geometría"
    );
}

#[test]
fn neighbor_chunks_share_boundary_seamlessly() {
    let config = WorldGenConfig::default();
    let field = DensityField::new(&config);

    // Dos chunks vecinos en X comparten el plano de puntos x = CHUNK_POINTS - 1.
    // y=1 es la capa que contiene la superficie (y=0 es totalmente sólida).
    let chunk_a = ChunkKey { x: 2, y: 1, z: 0 };
    let chunk_b = ChunkKey { x: 3, y: 1, z: 0 };
    let mesh_a = mesh_chunk(sample_chunk_grid(&field, &config, chunk_a));
    let mesh_b = mesh_chunk(sample_chunk_grid(&field, &config, chunk_b));

    let voxel = config.resolution.voxel_size;
    let plane_x = chunk_a.origin(voxel).x + CHUNK_SIZE as f32 * voxel;

    // Conjunto de posiciones cuantizadas de vértices en el plano compartido.
    let quant = 1024.0;
    let plane_vertices = |mesh: &MeshBuffer| -> std::collections::BTreeSet<[i64; 3]> {
        mesh.positions
            .iter()
            .filter(|p| (p[0] - plane_x).abs() < 1e-4)
            .map(|p| {
                [
                    (p[0] * quant) as i64,
                    (p[1] * quant) as i64,
                    (p[2] * quant) as i64,
                ]
            })
            .collect()
    };
    let va = plane_vertices(&mesh_a);
    let vb = plane_vertices(&mesh_b);
    assert!(
        !va.is_empty(),
        "el chunk A debe tener vértices en el plano compartido"
    );
    assert_eq!(
        va, vb,
        "los vértices del plano compartido deben coincidir exactamente"
    );
}

#[test]
fn triangle_normals_point_away_from_solid() {
    // La densidad positiva es sólido: la normal de cara debe tener componente
    // opuesta al gradiente (hacia densidad negativa). Verificamos con la esfera:
    // las normales de vértice deben apuntar hacia fuera (dot > 0 con radial).
    let center = Vec3::splat(16.0);
    let grid = sphere_grid(10.0, center, 1.0);
    let mesh = mesh_chunk(grid);
    let mut checked = 0;
    for (i, position) in mesh.positions.iter().enumerate() {
        let radial = Vec3::from_array(*position) - center;
        let normal = Vec3::from_array(mesh.normals[i]);
        let radial_len = radial.length();
        if radial_len < f32::EPSILON || normal.length_squared() < f32::EPSILON {
            continue;
        }
        assert!(
            normal.dot(radial) / (radial_len * normal.length()) > 0.5,
            "la normal debe apuntar hacia fuera del sólido en {position:?}"
        );
        checked += 1;
    }
    assert!(checked > 100);
}

#[test]
fn empty_chunk_produces_no_geometry() {
    // Un chunk muy por encima del terreno es 100% aire.
    let config = WorldGenConfig::default();
    let field = DensityField::new(&config);
    let chunk = ChunkKey { x: 0, y: 900, z: 0 };
    let mesh = mesh_chunk(sample_chunk_grid(&field, &config, chunk));
    assert!(mesh.positions.is_empty());
    assert!(mesh.indices.is_empty());
}
