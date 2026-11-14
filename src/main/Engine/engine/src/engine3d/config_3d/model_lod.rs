//! LOD de mallas estáticas GLB por distancia (`set_model_lod`).
//!
//! Cada parte de modelo estático genera hasta dos variantes simplificadas
//! (LOD1/LOD2) por *vertex clustering*: se cuantiza la posición de cada vértice
//! a una rejilla proporcional a la extensión de la parte y los vértices que
//! caen en la misma celda (con UV/normales cuantizados) se colapsan a uno solo
//! promediando sus atributos. Las variantes se suben a GPU junto a la malla
//! original y el pase forward las selecciona por distancia a la cámara
//! (`select_lod_level`), repartiendo los lotes por `mesh_idx`.
//!
//! Fuera de alcance por diseño: modelos skinned (van por el camino de joints),
//! RT/BVH (coleccionan geometría aparte) y worldgen (tiene su propio LOD de
//! chunks). La selección usa el AABB de picking LOD0; el culling no cambia.

use std::collections::HashMap;

use crate::engine3d::mesh::Vertex;

/// Vértices mínimos de una parte para merecer variantes LOD.
pub(crate) const MODEL_LOD_MIN_VERTICES: usize = 2048;

/// Factor de celda para LOD1 (≈ mitad de triángulos en mallas densas).
pub(crate) const LOD1_CELL_FACTOR: f32 = 0.02;

/// Factor de celda para LOD2 (≈ silueta, ~1/4–1/8 de triángulos).
pub(crate) const LOD2_CELL_FACTOR: f32 = 0.05;

/// LOD activado por defecto (el objetivo de la card es optimizar el motor).
pub(crate) const DEFAULT_MODEL_LOD_ENABLED: bool = true;

/// Distancia (m) desde la que se usa LOD1 por defecto.
pub(crate) const DEFAULT_MODEL_LOD1_DISTANCE_M: f32 = 40.0;

/// Distancia (m) desde la que se usa LOD2 por defecto.
pub(crate) const DEFAULT_MODEL_LOD2_DISTANCE_M: f32 = 100.0;

/// Límites de las distancias configurables.
pub(crate) const MODEL_LOD_DISTANCE_MIN_M: f32 = 5.0;
pub(crate) const MODEL_LOD_DISTANCE_MAX_M: f32 = 1000.0;

/// Nivel de LOD (0 = malla original, 1..=2 = variantes simplificadas) para una
/// distancia a la cámara. `lod1_m <= lod2_m` (el setter lo garantiza).
pub(crate) fn select_lod_level(distance_m: f32, lod1_m: f32, lod2_m: f32) -> usize {
    if distance_m < lod1_m {
        0
    } else if distance_m < lod2_m {
        1
    } else {
        2
    }
}

/// Factor de celda por nivel de LOD (`1` y `2`).
pub(crate) fn cell_factor_for_level(level: usize) -> Option<f32> {
    match level {
        1 => Some(LOD1_CELL_FACTOR),
        2 => Some(LOD2_CELL_FACTOR),
        _ => None,
    }
}

/// Decima una malla por *vertex clustering* con celda `extent * cell_factor`.
///
/// Devuelve `None` si la malla es pequeña, el factor no aplica o la reducción
/// de triángulos no llega al 10 % (no merece un swap de malla).
pub(crate) fn decimate_mesh(
    vertices: &[Vertex],
    indices: &[u32],
    cell_factor: f32,
) -> Option<(Vec<Vertex>, Vec<u32>)> {
    if vertices.len() < MODEL_LOD_MIN_VERTICES
        || indices.len() < 3
        || !cell_factor.is_finite()
        || cell_factor <= 0.0
    {
        return None;
    }

    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in vertices {
        for a in 0..3 {
            min[a] = min[a].min(v.position[a]);
            max[a] = max[a].max(v.position[a]);
        }
    }
    let extent = (max[0] - min[0])
        .max((max[1] - min[1]).max(max[2] - min[2]))
        .max(0.0);
    if !extent.is_finite() || extent <= f32::EPSILON {
        return None;
    }
    let cell = (extent * cell_factor).max(1e-5);
    // Cuantización UV proporcional a la celda (mínimo para no fundir seams).
    let uv_cell = (cell * 16.0).max(0.01);

    // Clave de cúmulo: posición cuantizada + UV + normal cuantizadas.
    let key_of = |v: &Vertex| -> [i64; 8] {
        [
            (v.position[0] / cell).floor() as i64,
            (v.position[1] / cell).floor() as i64,
            (v.position[2] / cell).floor() as i64,
            (v.uv[0] / uv_cell).floor() as i64,
            (v.uv[1] / uv_cell).floor() as i64,
            (v.normal[0] * 2.0).round() as i64,
            (v.normal[1] * 2.0).round() as i64,
            (v.normal[2] * 2.0).round() as i64,
        ]
    };

    struct Group {
        pos: [f64; 3],
        normal: [f64; 3],
        uv: [f64; 2],
        count: u32,
    }

    let mut group_ids: HashMap<[i64; 8], u32> = HashMap::new();
    let mut groups: Vec<Group> = Vec::new();
    let mut remap = vec![0u32; vertices.len()];
    for (i, v) in vertices.iter().enumerate() {
        let key = key_of(v);
        let next = groups.len() as u32;
        let id = *group_ids.entry(key).or_insert_with(|| {
            groups.push(Group {
                pos: [0.0; 3],
                normal: [0.0; 3],
                uv: [0.0; 2],
                count: 0,
            });
            next
        });
        let g = &mut groups[id as usize];
        for a in 0..3 {
            g.pos[a] += v.position[a] as f64;
            g.normal[a] += v.normal[a] as f64;
        }
        g.uv[0] += v.uv[0] as f64;
        g.uv[1] += v.uv[1] as f64;
        g.count += 1;
        remap[i] = id;
    }

    let tri_before = indices.len() / 3;
    let mut out_indices = Vec::with_capacity(indices.len());
    for tri in indices.chunks_exact(3) {
        let a = remap[tri[0] as usize];
        let b = remap[tri[1] as usize];
        let c = remap[tri[2] as usize];
        if a != b && b != c && a != c {
            out_indices.extend_from_slice(&[a, b, c]);
        }
    }
    if out_indices.len() / 3 >= tri_before * 9 / 10 {
        return None;
    }

    let mut out_vertices = Vec::with_capacity(groups.len());
    for g in &groups {
        let n = g.count.max(1) as f64;
        let mut normal = [
            (g.normal[0] / n) as f32,
            (g.normal[1] / n) as f32,
            (g.normal[2] / n) as f32,
        ];
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if len > f32::EPSILON {
            normal = [normal[0] / len, normal[1] / len, normal[2] / len];
        } else {
            normal = [0.0, 1.0, 0.0];
        }
        out_vertices.push(Vertex {
            position: [
                (g.pos[0] / n) as f32,
                (g.pos[1] / n) as f32,
                (g.pos[2] / n) as f32,
            ],
            normal,
            uv: [(g.uv[0] / n) as f32, (g.uv[1] / n) as f32],
        });
    }
    Some((out_vertices, out_indices))
}

/// Decima una parte para el nivel pedido (`1`/`2`); `None` si no aplica.
pub(crate) fn decimate_part_for_level(
    vertices: &[Vertex],
    indices: &[u32],
    level: usize,
) -> Option<(Vec<Vertex>, Vec<u32>)> {
    let factor = cell_factor_for_level(level)?;
    decimate_mesh(vertices, indices, factor)
}

impl super::super::engine::State {
    /// Registra las variantes LOD de una malla LOD0 recién subida a GPU.
    pub(crate) fn register_mesh_lod_variants(
        &mut self,
        mesh_idx: usize,
        lod1: Option<usize>,
        lod2: Option<usize>,
    ) {
        if lod1.is_none() && lod2.is_none() {
            return;
        }
        self.mesh_lod_variants.insert(mesh_idx, [lod1, lod2]);
    }

    /// `mesh_idx` de la variante LOD para un nivel (`1`/`2`), si existe.
    pub(crate) fn lod_mesh_idx_for(&self, mesh_idx: usize, level: usize) -> Option<usize> {
        self.mesh_lod_variants
            .get(&mesh_idx)
            .and_then(|variants| variants[level.checked_sub(1)?])
    }

    /// Configuración LOD de modelos (`set_model_lod`); emite el evento el
    /// llamador (mismo contrato que `set_taa`).
    pub(crate) fn set_model_lod(&mut self, enabled: bool, lod1: Option<f32>, lod2: Option<f32>) {
        let lod1 = lod1
            .unwrap_or(self.model_lod1_distance_m)
            .clamp(MODEL_LOD_DISTANCE_MIN_M, MODEL_LOD_DISTANCE_MAX_M);
        let lod2 = lod2
            .unwrap_or(self.model_lod2_distance_m)
            .clamp(lod1, MODEL_LOD_DISTANCE_MAX_M);
        if self.model_lod_enabled == enabled
            && (self.model_lod1_distance_m - lod1).abs() < f32::EPSILON
            && (self.model_lod2_distance_m - lod2).abs() < f32::EPSILON
        {
            return;
        }
        self.model_lod_enabled = enabled;
        self.model_lod1_distance_m = lod1;
        self.model_lod2_distance_m = lod2;
        log::info!(
            "[lod] modelos: {} (LOD1 < {} m, LOD2 < {} m)",
            if enabled { "on" } else { "off" },
            lod1,
            lod2
        );
    }
}

/// Aplica la configuración del `.save` (`lodEnabled`/`lod1Distance`/`lod2Distance`).
pub(crate) fn apply_model_lod_from_world_wire(
    state: &mut super::super::engine::State,
    enabled: Option<bool>,
    lod1: Option<f32>,
    lod2: Option<f32>,
) {
    state.set_model_lod(enabled.unwrap_or(DEFAULT_MODEL_LOD_ENABLED), lod1, lod2);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine3d::mesh::Vertex;

    fn v(x: f32, y: f32, z: f32) -> Vertex {
        Vertex {
            position: [x, y, z],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
        }
    }

    /// Grid de (n+1)² vértices con 2 triángulos por celda.
    fn grid_mesh(n: u32) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        for y in 0..=n {
            for x in 0..=n {
                vertices.push(Vertex {
                    position: [x as f32, 0.0, y as f32],
                    normal: [0.0, 1.0, 0.0],
                    uv: [x as f32 / n as f32, y as f32 / n as f32],
                });
            }
        }
        let idx = |x: u32, y: u32| y * (n + 1) + x;
        let mut indices = Vec::new();
        for y in 0..n {
            for x in 0..n {
                indices.extend_from_slice(&[
                    idx(x, y),
                    idx(x, y + 1),
                    idx(x + 1, y + 1),
                    idx(x, y),
                    idx(x + 1, y + 1),
                    idx(x + 1, y),
                ]);
            }
        }
        (vertices, indices)
    }

    #[test]
    fn lod_levels_follow_distance_thresholds() {
        assert_eq!(select_lod_level(0.0, 40.0, 100.0), 0);
        assert_eq!(select_lod_level(39.9, 40.0, 100.0), 0);
        assert_eq!(select_lod_level(40.0, 40.0, 100.0), 1);
        assert_eq!(select_lod_level(99.9, 40.0, 100.0), 1);
        assert_eq!(select_lod_level(100.0, 40.0, 100.0), 2);
        assert_eq!(select_lod_level(9999.0, 40.0, 100.0), 2);
    }

    #[test]
    fn decimate_rejects_small_or_unreduced_meshes() {
        // Debajo del umbral de vértices.
        let small: Vec<Vertex> = (0..64).map(|i| v(i as f32, 0.0, 0.0)).collect();
        assert!(decimate_mesh(&small, &[0, 1, 2, 1, 2, 3], LOD1_CELL_FACTOR).is_none());

        // Grid muy denso pero factor de celda ridículo: sin reducción real.
        let (vertices, indices) = grid_mesh(64); // 4225 vértices
        assert!(decimate_mesh(&vertices, &indices, 0.0001).is_none());
    }

    #[test]
    fn decimate_collapses_grid_cells() {
        // Grid 64×64 de extensión 1 m: celda 0.5 m agrupa a ~3×3 vértices.
        let (vertices, indices) = grid_mesh(64);
        let tri_before = indices.len() / 3;
        let (lod_vertices, lod_indices) =
            decimate_mesh(&vertices, &indices, 0.5).expect("decimación del grid");
        assert!(lod_vertices.len() < vertices.len() / 4);
        assert!(lod_indices.len() / 3 < tri_before / 4);
        // Índices válidos y sin triángulos degenerados.
        for tri in lod_indices.chunks_exact(3) {
            assert!(tri[0] != tri[1] && tri[1] != tri[2] && tri[0] != tri[2]);
            for i in tri {
                assert!((*i as usize) < lod_vertices.len());
            }
        }
        // Atributos promediados dentro de rango.
        for vert in &lod_vertices {
            assert!(vert.uv[0] >= -0.01 && vert.uv[0] <= 1.01);
            assert!(vert.uv[1] >= -0.01 && vert.uv[1] <= 1.01);
            assert!((vert.normal[1] - 1.0).abs() < 0.01);
        }
    }

    #[test]
    fn decimate_part_for_level_maps_factors() {
        // Grid 128×128 (espaciado 1/128): celdas LOD1/LOD2 agrupan varios vértices.
        let (vertices, indices) = grid_mesh(128);
        assert!(decimate_part_for_level(&vertices, &indices, 0).is_none());
        assert!(decimate_part_for_level(&vertices, &indices, 1).is_some());
        assert!(decimate_part_for_level(&vertices, &indices, 2).is_some());
        assert!(decimate_part_for_level(&vertices, &indices, 3).is_none());
    }
}
