//! Marching Cubes con normales por gradiente y deduplicación de vértices.
//!
//! Convención del campo: **densidad positiva = sólido**. La isosuperficie es
//! `density = 0`.
//!
//! Decisiones de diseño:
//! - La orientación de cada triángulo se corrige contra el **gradiente del campo**
//!   (la normal apunta hacia la densidad decreciente, es decir, hacia fuera del
//!   sólido). Esto hace el resultado inmune a los casos ambiguos de la tabla
//!   canónica, que en otras implementaciones se parchean invirtiendo el winding a mano.
//! - Las normales de vértice se obtienen por **gradiente central** sobre el grid
//!   con padding, no por promedio de caras con keys de string (mucho más barato
//!   y exacto).
//! - Los vértices compartidos entre celdas se deduplican por posición cuantizada,
//!   no por string.

pub mod tables;

use std::collections::HashSet;

use glam::{Vec3, Vec3A};

use self::tables::{CORNER_OFFSETS, EDGE_CORNERS, EDGE_TABLE, TRI_TABLE};

/// Isosuperficie del campo de densidad.
pub const ISO_LEVEL: f32 = 0.0;

/// Ranura todavía no asignada en la tabla de vértices compartidos.
const NULL_SLOT: u32 = u32::MAX;

/// Buffer de geometría de un chunk (posiciones/normales/UV separados + índices).
///
/// El engine ensambla sus `Vertex { position, normal, uv }` a partir de aquí.
#[derive(Debug, Default, Clone)]
pub struct MeshBuffer {
    /// Posiciones de vértice en coordenadas de mundo (metros).
    pub positions: Vec<[f32; 3]>,
    /// Normales de vértice (orientadas hacia fuera del sólido, sin normalizar;
    /// su longitud depende de cuántas celdas comparten el vértice).
    pub normals: Vec<[f32; 3]>,
    /// UVs. El terreno usa splat mapping por coordenadas de mundo, así que los UVs
    /// son un placeholder (0,0) hasta la fase de splat.
    pub uvs: Vec<[f32; 2]>,
    /// Índices de triángulo (3 por triángulo).
    pub indices: Vec<u32>,
}

impl MeshBuffer {
    /// Número de triángulos del buffer.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Vacía el buffer conservando la capacidad.
    pub fn clear(&mut self) {
        self.positions.clear();
        self.normals.clear();
        self.uvs.clear();
        self.indices.clear();
    }

    /// Hash FNV-1a de la geometría para tests dorados de determinismo.
    #[must_use]
    pub fn geometry_hash(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for value in self
            .positions
            .iter()
            .flat_map(|p| p.iter())
            .chain(self.normals.iter().flat_map(|n| n.iter()))
        {
            for byte in value.to_bits().to_le_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        for value in &self.indices {
            for byte in value.to_le_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash
    }
}

/// Grid de densidad de un chunk, con padding de 1 punto por lado para gradientes
/// centrales sin costuras en los bordes.
#[derive(Debug, Clone)]
pub struct DensityGrid {
    /// Puntos por eje incluido padding (`CHUNK_POINTS + 2 * pad`).
    pub dim: u32,
    /// Puntos de padding por lado (siempre 1 en la práctica).
    pub pad: u32,
    /// Densidades en orden `x + y * dim + z * dim * dim` (x más rápido).
    pub data: Vec<f32>,
    /// Posición de mundo del punto de grid `(0, 0, 0)` (incluye el padding).
    pub origin: Vec3,
    /// Tamaño de voxel en metros.
    pub voxel_size: f32,
    /// Nivel de LOD con el que se muestreó el grid (`0` = resolución completa).
    pub lod: u32,
}

impl DensityGrid {
    /// Valor de densidad en el punto de grid con padding.
    #[inline]
    #[must_use]
    pub fn value(&self, x: u32, y: u32, z: u32) -> f32 {
        let dim = self.dim as usize;
        self.data[x as usize + y as usize * dim + z as usize * dim * dim]
    }

    /// Posición de mundo del punto de grid con padding.
    #[inline]
    #[must_use]
    pub fn point_world(&self, x: u32, y: u32, z: u32) -> Vec3 {
        self.origin + Vec3::new(x as f32, y as f32, z as f32) * self.voxel_size
    }

    /// Gradiente central de la densidad en el punto de grid con padding.
    ///
    /// Requiere `1 <= x,y,z < dim - 1` (siempre cierto para corners de celdas
    /// del interior del chunk, que es lo único que se meshea).
    #[inline]
    #[must_use]
    pub fn gradient(&self, x: u32, y: u32, z: u32) -> Vec3 {
        let inv = 0.5 / self.voxel_size;
        Vec3::new(
            (self.value(x + 1, y, z) - self.value(x - 1, y, z)) * inv,
            (self.value(x, y + 1, z) - self.value(x, y - 1, z)) * inv,
            (self.value(x, y, z + 1) - self.value(x, y, z - 1)) * inv,
        )
    }
}

/// Interpolación lineal en una arista cortada por la isosuperficie.
#[inline]
fn edge_point(pa: Vec3A, va: f32, pb: Vec3A, vb: f32) -> Vec3A {
    let delta = vb - va;
    if delta.abs() < f32::EPSILON {
        return pa;
    }
    let mu = ((ISO_LEVEL - va) / delta).clamp(0.0, 1.0);
    pa + (pb - pa) * mu
}

/// Ranura de la tabla de vértices correspondiente a una arista de la rejilla.
///
/// Las tres orientaciones se almacenan en bloques consecutivos de `dim³`; los
/// corners van en un cuarto bloque (ver [`corner_slot`]).
#[inline]
fn edge_slot(dim: u32, axis: usize, x: u32, y: u32, z: u32) -> usize {
    let d = dim as usize;
    axis * d * d * d + (z as usize) * d * d + (y as usize) * d + x as usize
}

/// Ranura de la tabla de vértices correspondiente a un corner de la rejilla.
///
/// Se usa cuando el cruce cae exactamente sobre el corner: varias aristas
/// convergentes deben compartir el mismo vértice para no romper la malla.
#[inline]
fn corner_slot(dim: u32, x: u32, y: u32, z: u32) -> usize {
    let d = dim as usize;
    3 * d * d * d + (z as usize) * d * d + (y as usize) * d + x as usize
}

/// Clave de posición cuantizada, con la misma resolución que usa el mesher
/// para considerar que dos puntos son el mismo vértice.
#[inline]
fn quantize_key(p: Vec3A, quant: f32) -> [i64; 3] {
    [
        (p.x * quant).round() as i64,
        (p.y * quant).round() as i64,
        (p.z * quant).round() as i64,
    ]
}

/// Meshea el chunk representado por `grid` (toma propiedad del grid).
///
/// Solo se procesan las celdas interiores al chunk (los `pad` puntos de padding
/// existen solo para gradientes en el borde). Los chunks vecinos que comparten
/// puntos de grid producen vértices idénticos bit a bit (misma densidad de entrada,
/// misma interpolación), por lo que las costuras son imposibles por construcción.
#[must_use]
pub fn mesh_chunk(grid: DensityGrid) -> MeshBuffer {
    let mut out = MeshBuffer::default();
    mesh_chunk_into(&grid, &mut out);
    out
}

/// Igual que [`mesh_chunk`] pero reutilizando el buffer de salida.
pub fn mesh_chunk_into(grid: &DensityGrid, out: &mut MeshBuffer) {
    out.clear();
    if grid.dim < 3 {
        return;
    }

    let pad = grid.pad;
    // Celdas a meshear por eje: los CHUNK_POINTS puntos interiores forman
    // CHUNK_SIZE = dim - 2*pad - 1 celdas (los 2*pad puntos de padding no meshean).
    let cells = grid.dim - 2 * pad - 1; // celdas por eje (== CHUNK_SIZE)
    if cells == 0 {
        return;
    }

    let dim = grid.dim;
    let d = dim as usize;
    // Tabla plana de vértices compartidos: acceso directo por arista o corner,
    // sin búsqueda por hash ni asignaciones por vértice.
    let mut vertex_slots = vec![NULL_SLOT; 4 * d * d * d];

    let origin = Vec3A::from(grid.origin);
    let voxel = grid.voxel_size;
    let quant = 1024.0 / voxel;

    let mut values = [0.0f32; 8];
    let mut corners = [[0u32; 3]; 8];
    let mut corner_gradients = [Vec3A::ZERO; 8];
    let mut edge_pos = [Vec3A::ZERO; 12];
    let mut edge_normal = [Vec3A::ZERO; 12];
    let mut edge_slots = [0usize; 12];

    for cz in 0..cells {
        for cy in 0..cells {
            for cx in 0..cells {
                // Corners de la celda en el grid con padding.
                let mut cube_index = 0u32;
                for (i, corner) in corners.iter_mut().enumerate() {
                    let [ox, oy, oz] = CORNER_OFFSETS[i].map(|o| o as u32);
                    let (x, y, z) = (cx + pad + ox, cy + pad + oy, cz + pad + oz);
                    *corner = [x, y, z];
                    let v = grid.value(x, y, z);
                    values[i] = v;
                    if v <= ISO_LEVEL {
                        cube_index |= 1 << i;
                    }
                }

                let edges = EDGE_TABLE[cube_index as usize];
                if edges == 0 {
                    continue;
                }

                // De los 8 corners solo se usan los que aparecen en alguna
                // arista cortada. Calcular el gradiente una vez por celda evita
                // re-leer el grid en cada arista y en cada celda vecina.
                let mut used = 0u8;
                for (edge, [a, b]) in EDGE_CORNERS.iter().enumerate() {
                    if edges & (1u16 << edge) != 0 {
                        used |= (1u8 << *a) | (1u8 << *b);
                    }
                }
                for i in 0..8usize {
                    if used & (1u8 << i) != 0 {
                        let [x, y, z] = corners[i];
                        // Densidad positiva = sólido: el gradiente apunta hacia
                        // dentro, la normal es su opuesto (hacia el aire).
                        corner_gradients[i] = -Vec3A::from(grid.gradient(x, y, z));
                    }
                }

                // Puntos, normales y ranuras compartidas de cada arista cortada.
                for edge in 0..12usize {
                    if edges & (1u16 << edge) == 0 {
                        continue;
                    }
                    let [a, b] = EDGE_CORNERS[edge];
                    let (a, b) = (a as usize, b as usize);
                    let ca = corners[a];
                    let cb = corners[b];
                    let pa = origin + Vec3A::new(ca[0] as f32, ca[1] as f32, ca[2] as f32) * voxel;
                    let pb = origin + Vec3A::new(cb[0] as f32, cb[1] as f32, cb[2] as f32) * voxel;
                    let p = edge_point(pa, values[a], pb, values[b]);
                    edge_pos[edge] = p;
                    edge_normal[edge] = edge_normal_at(
                        values[a],
                        values[b],
                        corner_gradients[a],
                        corner_gradients[b],
                    );

                    // Si el cruce cuantiza al mismo punto que un corner, todas
                    // las aristas que llegan a él comparten el mismo vértice.
                    let qp = quantize_key(p, quant);
                    edge_slots[edge] = if qp == quantize_key(pa, quant) {
                        corner_slot(dim, ca[0], ca[1], ca[2])
                    } else if qp == quantize_key(pb, quant) {
                        corner_slot(dim, cb[0], cb[1], cb[2])
                    } else {
                        let axis = if ca[0] != cb[0] {
                            0
                        } else if ca[1] != cb[1] {
                            1
                        } else {
                            2
                        };
                        edge_slot(
                            dim,
                            axis,
                            ca[0].min(cb[0]),
                            ca[1].min(cb[1]),
                            ca[2].min(cb[2]),
                        )
                    };
                }

                // Triángulos de la tabla.
                let tris = &TRI_TABLE[cube_index as usize];
                let mut i = 0;
                while i < 16 && tris[i] != -1 {
                    let (ea, eb, ec) = (tris[i], tris[i + 1], tris[i + 2]);
                    i += 3;
                    if ea < 0 || eb < 0 || ec < 0 {
                        break;
                    }
                    let (ea, eb, ec) = (ea as usize, eb as usize, ec as usize);

                    // La cara debe mirar hacia donde miran las normales de
                    // vértice (fuera del sólido).
                    let (mut vb, mut vc) = (edge_pos[eb], edge_pos[ec]);
                    let va = edge_pos[ea];
                    let mut nb = edge_normal[eb];
                    let mut nc = edge_normal[ec];
                    let na = edge_normal[ea];
                    let (mut sb, mut sc) = (edge_slots[eb], edge_slots[ec]);
                    let sa = edge_slots[ea];
                    let face_normal = (vb - va).cross(vc - va);
                    let outward = (na + nb + nc) * 0.333_333_34;
                    if face_normal.dot(outward) < 0.0 {
                        std::mem::swap(&mut vb, &mut vc);
                        std::mem::swap(&mut nb, &mut nc);
                        std::mem::swap(&mut sb, &mut sc);
                    }

                    let ia = vertex_index(
                        out,
                        &mut vertex_slots,
                        sa,
                        va,
                        na,
                        chunk_uv(grid, va.into()),
                    );
                    let ib = vertex_index(
                        out,
                        &mut vertex_slots,
                        sb,
                        vb,
                        nb,
                        chunk_uv(grid, vb.into()),
                    );
                    let ic = vertex_index(
                        out,
                        &mut vertex_slots,
                        sc,
                        vc,
                        nc,
                        chunk_uv(grid, vc.into()),
                    );
                    if ia != ib && ib != ic && ia != ic {
                        out.indices.push(ia);
                        out.indices.push(ib);
                        out.indices.push(ic);
                    }
                }
            }
        }
    }

    if grid.lod > 0 {
        add_lod_skirt(grid, out);
    }
}

/// Interpola el gradiente (normal) en el punto de la arista.
#[inline]
fn edge_normal_at(va: f32, vb: f32, ga: Vec3A, gb: Vec3A) -> Vec3A {
    let delta = vb - va;
    if delta.abs() < f32::EPSILON {
        return ga.normalize_or_zero();
    }
    let mu = ((ISO_LEVEL - va) / delta).clamp(0.0, 1.0);
    (ga * (1.0 - mu) + gb * mu).normalize_or_zero()
}

/// Busca o inserta el vértice de una ranura; acumula normales de los vértices
/// compartidos para suavizar juntas entre celdas.
#[inline]
fn vertex_index(
    out: &mut MeshBuffer,
    vertex_slots: &mut [u32],
    slot: usize,
    pos: Vec3A,
    normal: Vec3A,
    uv: [f32; 2],
) -> u32 {
    let existing = vertex_slots[slot];
    if existing != NULL_SLOT {
        let index = existing as usize;
        let summed = Vec3A::from_array(out.normals[index]) + normal;
        // Suma cruda: la normalización la aplica el shader al presentar la malla.
        out.normals[index] = summed.to_array();
        return existing;
    }
    let index = out.positions.len() as u32;
    vertex_slots[slot] = index;
    out.positions.push(pos.to_array());
    out.normals.push(normal.to_array());
    out.uvs.push(uv);
    index
}

/// UV de la columna XZ (sin el padding del grid), en `[0, 1]`.
fn chunk_uv(grid: &DensityGrid, pos: Vec3) -> [f32; 2] {
    let cells = grid.dim.saturating_sub(2 * grid.pad + 1).max(1) as f32;
    let origin = grid.origin + Vec3::splat(grid.pad as f32 * grid.voxel_size);
    let size = (cells * grid.voxel_size).max(1.0e-4);
    [
        ((pos.x - origin.x) / size).clamp(0.0, 1.0),
        ((pos.z - origin.z) / size).clamp(0.0, 1.0),
    ]
}

/// Falda vertical de un voxel en el perímetro XZ. Cierra el hueco entre un
/// chunk de LOD grueso y el vecino más fino sin cambiar la ABI.
fn add_lod_skirt(grid: &DensityGrid, out: &mut MeshBuffer) {
    if out.positions.is_empty() {
        return;
    }
    let cells = grid.dim.saturating_sub(2 * grid.pad + 1);
    if cells == 0 {
        return;
    }
    let min = grid.origin + Vec3::splat(grid.pad as f32 * grid.voxel_size);
    let extent = cells as f32 * grid.voxel_size;
    let max_x = min.x + extent;
    let max_z = min.z + extent;
    let eps = (grid.voxel_size * 0.2).max(1.0e-3);
    let drop = grid.voxel_size;
    let on_x = |x: f32| (x - min.x).abs() <= eps || (max_x - x).abs() <= eps;
    let on_z = |z: f32| (z - min.z).abs() <= eps || (max_z - z).abs() <= eps;

    let mut seen = HashSet::<(u32, u32)>::new();
    let mut quads: Vec<([f32; 3], [f32; 3], [f32; 3])> = Vec::new();
    #[allow(clippy::chunks_exact_to_as_chunks)]
    for tri in out.indices.chunks_exact(3) {
        for k in 0..3 {
            let ia = tri[k];
            let ib = tri[(k + 1) % 3];
            let key = if ia < ib { (ia, ib) } else { (ib, ia) };
            if !seen.insert(key) {
                continue;
            }
            let pa = out.positions[ia as usize];
            let pb = out.positions[ib as usize];
            let same_x = on_x(pa[0]) && on_x(pb[0]) && (pa[0] - pb[0]).abs() <= eps;
            let same_z = on_z(pa[2]) && on_z(pb[2]) && (pa[2] - pb[2]).abs() <= eps;
            if !same_x && !same_z {
                continue;
            }
            let outward = if same_x {
                if (pa[0] - min.x).abs() <= eps {
                    [-1.0, 0.0, 0.0]
                } else {
                    [1.0, 0.0, 0.0]
                }
            } else if (pa[2] - min.z).abs() <= eps {
                [0.0, 0.0, -1.0]
            } else {
                [0.0, 0.0, 1.0]
            };
            quads.push((pa, pb, outward));
        }
    }
    for (pa, pb, outward) in quads {
        let da = [pa[0], pa[1] - drop, pa[2]];
        let db = [pb[0], pb[1] - drop, pb[2]];
        let base = out.positions.len() as u32;
        let uva = chunk_uv(grid, Vec3::from_array(pa));
        let uvb = chunk_uv(grid, Vec3::from_array(pb));
        out.positions.extend([pa, pb, db, da]);
        out.normals.extend([outward, outward, outward, outward]);
        out.uvs.extend([uva, uvb, uvb, uva]);
        out.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}
