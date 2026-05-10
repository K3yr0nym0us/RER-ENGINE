// ---------------------------------------------------------------------------
// Spatial Partitioning — Grid-based spatial index for fast picking/queries
//
// Divide el espacio en una cuadrícula de celdas. Cada entidad se añade a las
// celdas que ocupa (AABB). Las queries devuelven solo las entidades en las
// celdas relevantes.
//
// - O(1) to O(k) lookup para k = número de entidades en la celda (típicamente << n)
// - O(n) rebuild por frame (pero trivial en CPU comparado con render)
// ---------------------------------------------------------------------------

use std::collections::HashMap;

/// Tamaño de celda: unidades de mundo por celda.
/// Para un juego 2D típico con vista de 40×30, 5.0 → 8×6 celdas.
const CELL_SIZE: f32 = 5.0;

/// Grid 2D para spatial partitioning.
/// Cada celda contiene la lista de EntityIds que ocupan esa celda.
pub struct SpatialGrid {
    cells: HashMap<(i32, i32), Vec<u32>>,  // (grid_x, grid_y) → [EntityId, ...]
}

impl SpatialGrid {
    pub fn new() -> Self {
        Self { cells: HashMap::new() }
    }

    /// Limpia el grid para rebuild.
    pub fn clear(&mut self) {
        self.cells.clear();
    }

    /// Calcula la celda para una coordenada del mundo.
    #[inline]
    fn world_to_grid(wx: f32) -> i32 {
        (wx / CELL_SIZE).floor() as i32
    }

    /// Inserta una entidad en las celdas que ocupa su AABB.
    /// 
    /// AABB: [min_x, min_y, max_x, max_y]
    pub fn insert_entity(&mut self, entity_id: u32, aabb: [f32; 4]) {
        let gx_min = Self::world_to_grid(aabb[0]);
        let gy_min = Self::world_to_grid(aabb[1]);
        let gx_max = Self::world_to_grid(aabb[2]);
        let gy_max = Self::world_to_grid(aabb[3]);

        for gx in gx_min..=gx_max {
            for gy in gy_min..=gy_max {
                self.cells
                    .entry((gx, gy))
                    .or_insert_with(Vec::new)
                    .push(entity_id);
            }
        }
    }

    /// Query: retorna todas las entidades en un rango AABB (para queries más amplias).
    #[allow(dead_code)]
    pub fn query_aabb(&self, aabb: [f32; 4]) -> Vec<u32> {
        let gx_min = Self::world_to_grid(aabb[0]);
        let gy_min = Self::world_to_grid(aabb[1]);
        let gx_max = Self::world_to_grid(aabb[2]);
        let gy_max = Self::world_to_grid(aabb[3]);

        let mut result = Vec::new();
        for gx in gx_min..=gx_max {
            for gy in gy_min..=gy_max {
                if let Some(entities) = self.cells.get(&(gx, gy)) {
                    result.extend(entities.iter().copied());
                }
            }
        }
        result
    }
}

impl Default for SpatialGrid {
    fn default() -> Self {
        Self::new()
    }
}
