//! Chunks de terreno: grids de densidad por chunk, claves y Y-layer pruning.
//!
//! Cada chunk es una unidad de trabajo pura y paralelizable: su densidad se evalúa
//! en coordenadas de mundo, de modo que dos chunks vecinos que compartan punto de
//! grid producen valores idénticos y sus mallas sellan sin intercambio de datos
//! (patrón validado por la referencia).

use glam::Vec3;

use crate::config::WorldGenConfig;
use crate::field::DensityField;
use crate::mesher::DensityGrid;

/// Voxels de un chunk por eje (excluye padding de puntos de grid).
pub const CHUNK_SIZE: u32 = 32;
/// Puntos de grid por eje sin padding (`CHUNK_SIZE + 1`).
pub const CHUNK_POINTS: u32 = CHUNK_SIZE + 1;
/// Padding de puntos de grid por lado (para gradientes centrales en el borde).
pub const GRID_PAD: u32 = 1;
/// Puntos de grid por eje en el grid con padding.
pub const GRID_DIM: u32 = CHUNK_POINTS + 2 * GRID_PAD;

/// Clave de chunk en la rejilla del mundo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkKey {
    /// Índice de chunk en X (puede ser negativo fuera del origen; el mundo usa x >= 0).
    pub x: i32,
    /// Índice de chunk en Y (0 = bajo el suelo del mundo).
    pub y: i32,
    /// Índice de chunk en Z.
    pub z: i32,
}

impl ChunkKey {
    /// Posición de mundo de la esquina `(0, 0, 0)` del chunk (sin padding).
    #[must_use]
    pub fn origin(self, voxel_size: f32) -> Vec3 {
        Vec3::new(
            self.x as f32 * CHUNK_SIZE as f32 * voxel_size,
            self.y as f32 * CHUNK_SIZE as f32 * voxel_size,
            self.z as f32 * CHUNK_SIZE as f32 * voxel_size,
        )
    }
}

/// Resultado del pruning de columna: chunks Y activos para una columna XZ.
///
/// Replica la optimización de la referencia (`_get_surface_y_layers_for_xz_chunk`):
/// se samplea la altura en una rejilla 5×5 de la columna y solo se activan los
/// chunks Y cuyo rango pueda contener terreno.
#[must_use]
pub fn active_y_chunks(
    field: &DensityField,
    config: &WorldGenConfig,
    cx: i32,
    cz: i32,
) -> Vec<i32> {
    let voxel = config.resolution.voxel_size;
    let cell = CHUNK_SIZE as f32 * voxel;
    let base_x = cx as f32 * cell;
    let base_z = cz as f32 * cell;
    let samples = 5u32;
    let step = cell / (samples - 1) as f32;

    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for sy in 0..samples {
        for sx in 0..samples {
            // Altura con features planificados: montañas/valles/lagos cambian el
            // rango de la columna (el pruning base se quedaría corto).
            let h = field.height_at(base_x + sx as f32 * step, base_z + sy as f32 * step);
            min_y = min_y.min(h);
            max_y = max_y.max(h);
        }
    }

    let first = ((min_y / cell).floor() - 1.0).max(0.0) as i32;
    let last = ((max_y / cell).floor() + 1.0) as i32;
    let max_layer = config.max_chunk_y(CHUNK_SIZE) as i32;
    (first..=last.min(max_layer)).collect()
}

/// Muestrea el grid de densidad con padding de un chunk.
///
/// Coste: `GRID_DIM³` evaluaciones de densidad por chunk (34³ con el tamaño
/// por defecto); cada evaluación es una función pura de la posición de mundo.
#[must_use]
pub fn sample_chunk_grid(
    field: &DensityField,
    config: &WorldGenConfig,
    chunk: ChunkKey,
) -> DensityGrid {
    sample_chunk_grid_lod(field, config, chunk, 0)
}

/// Muestrea el grid de densidad de un chunk a un nivel de LOD.
///
/// `lod = 0`: voxels de la config (32 celdas por eje). Cada nivel duplica el
/// tamaño de voxel y reduce a la mitad las celdas por eje, cubriendo la misma
/// área de chunk: LOD para chunks lejanos (16 o 8 celdas).
///
/// Coste: `dim³` evaluaciones de densidad; cada evaluación es una función pura
/// de la posición de mundo.
#[must_use]
pub fn sample_chunk_grid_lod(
    field: &DensityField,
    config: &WorldGenConfig,
    chunk: ChunkKey,
    lod: u32,
) -> DensityGrid {
    let lod = lod.min(2);
    let base_voxel = config.resolution.voxel_size;
    let voxel = base_voxel * (1u32 << lod) as f32;
    let dim = (CHUNK_SIZE >> lod) + 1 + 2 * GRID_PAD;
    let mut data = Vec::with_capacity((dim * dim * dim) as usize);
    // El chunk cubre la misma área; con voxels más gruesos el padding crece.
    let origin = chunk.origin(base_voxel) - Vec3::splat(GRID_PAD as f32 * voxel);

    for pz in 0..dim {
        let z = origin.z + pz as f32 * voxel;
        for py in 0..dim {
            let y = origin.y + py as f32 * voxel;
            for px in 0..dim {
                let x = origin.x + px as f32 * voxel;
                data.push(field.density_at(x, y, z));
            }
        }
    }

    DensityGrid {
        dim,
        pad: GRID_PAD,
        data,
        origin,
        voxel_size: voxel,
    }
}

/// Rango de chunks XZ que cubren los límites del mundo (exclusivo en el límite
/// superior), respetando los límites configurados.
#[must_use]
pub fn chunk_range_xz(config: &WorldGenConfig) -> (std::ops::Range<i32>, std::ops::Range<i32>) {
    let (chunks_x, chunks_z) = config.chunk_count_xz(CHUNK_SIZE);
    (0..chunks_x as i32, 0..chunks_z as i32)
}
