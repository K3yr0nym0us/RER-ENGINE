//! Agua de bloques estilo Minecraft: autómata celular en CPU, sin partículas.
//!
//! Cada celda de la rejilla guarda un nivel 0..=7 (7 = fuente infinita). En cada
//! tick el agua cae sin perder nivel si hay hueco debajo y, si no, se reparte a
//! los cuatro vecinos horizontales con nivel −1. El resultado es exactamente el
//! comportamiento buscado: al colocar un cubo, el agua se distribuye por su
//! espacio, baja por los desniveles del terreno y se detiene sola.
//!
//! El terreno NO se modifica: solo se consulta por raycast descendente para
//! saber dónde es sólida cada columna (los colisionadores ya existentes del
//! viewport). Esto es independiente del generador de mundos.
//!
//! La malla se reconstruye únicamente cuando la rejilla cambia, y se dibuja como
//! una entidad estática con material semitransparente. Nada de esto pasa por el
//! sistema de partículas GPU (el antiguo «cubo de agua» era un salpicador).

use std::collections::{HashMap, HashSet};

use glam::Vec3;

use crate::engine3d::ecs::{
    EntityId, MeshComponent, MeshLocalAabb, NonSelectable, SurfacePbr, Transform,
};
use crate::engine3d::ipc::SaveParticleEmitterSnapshot;
use crate::engine3d::mesh::Vertex;

/// Wire del elemento del sidebar (comparte snapshot con los emisores).
pub(crate) const WATER_WIRE: &str = "water_block";
/// Tamaño de celda por defecto (m).
pub(crate) const DEFAULT_CELL_SIZE: f32 = 0.5;
/// Nivel de una fuente: nunca se agota.
const SOURCE_LEVEL: u8 = 7;
/// Tick del autómata (5 Hz, como Minecraft).
pub(crate) const TICK_INTERVAL: f32 = 0.2;
const MIN_CELL: f32 = 0.1;
const MAX_CELL: f32 = 4.0;
/// Tope de seguridad: un charco en una llanura infinita no puede crecer sin fin.
const MAX_CELLS: usize = 24_000;
/// Origen del raycast que busca el suelo de cada columna.
const RAY_TOP: f32 = 1000.0;
const RAY_MAX: f32 = 2000.0;

/// Celda de la rejilla (índices X, Y, Z).
type Cell = (i32, i32, i32);

/// Fuente colocada desde la UI. Guarda su posición para poder reconstruir la
/// rejilla al cargar una escena o al cambiar el tamaño de celda.
#[derive(Debug, Clone)]
pub(crate) struct WaterSource {
    pub id: u64,
    pub position: [f32; 3],
    pub cell_size: f32,
    /// Celda snapped (puede moverse hacia arriba si el terreno la tapa).
    cell: Cell,
}

impl WaterSource {
    fn new(id: u64, position: [f32; 3], cell_size: f32) -> Self {
        let cell = snap(position, cell_size);
        Self {
            id,
            position,
            cell_size,
            cell,
        }
    }
}

fn snap(position: [f32; 3], cell_size: f32) -> Cell {
    (
        (position[0] / cell_size).floor() as i32,
        (position[1] / cell_size).floor() as i32,
        (position[2] / cell_size).floor() as i32,
    )
}

fn cell_bottom(cell: Cell, cell_size: f32) -> f32 {
    cell.1 as f32 * cell_size
}

/// ¿Hay terreno en esta celda? El suelo de cada columna viene del raycast.
fn is_solid(cell: Cell, cell_size: f32, floors: &HashMap<(i32, i32), f32>) -> bool {
    let floor = floors.get(&(cell.0, cell.2)).copied().unwrap_or(0.0);
    cell_bottom(cell, cell_size) + 1.0e-3 < floor
}

/// Rejilla de agua + estado de render. Toda la lógica del autómata es pura
/// (recibe las alturas de suelo ya consultadas por el motor).
#[derive(Default)]
pub(crate) struct WaterSystem {
    sources: Vec<WaterSource>,
    cells: HashMap<Cell, u8>,
    cell_size: f32,
    /// Suelo ya consultado por columna: el raycast contra el trimesh del terreno
    /// es caro y la altura de una columna no cambia mientras no se regenere.
    floor_cache: HashMap<(i32, i32), f32>,
    pub(crate) tick_accum: f32,
    /// `true` cuando la rejilla cambió y la malla hay que subirla de nuevo.
    pub(crate) dirty: bool,
    /// Entidad ECS con la malla de agua (una para todo el charco).
    entity: Option<EntityId>,
    mesh_idx: Option<usize>,
    tex_idx: Option<usize>,
}

impl WaterSystem {
    pub(crate) fn cell_size(&self) -> f32 {
        if self.cell_size > 0.0 {
            self.cell_size
        } else {
            DEFAULT_CELL_SIZE
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub(crate) fn sources(&self) -> &[WaterSource] {
        &self.sources
    }

    pub(crate) fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Fuentes pickables por el viewport: comparte el cableado de hover y click
    /// derecho de los emisores (F5).
    pub(crate) fn pickable(&self) -> Vec<(u64, [f32; 3])> {
        self.sources
            .iter()
            .map(|source| (source.id, source.position))
            .collect()
    }

    /// Alta o reemplazo de una fuente. Todas las fuentes comparten el tamaño de
    /// celda de la más reciente: cambiarlo reconstruye el snapped de las demás.
    pub(crate) fn add_source(&mut self, id: u64, position: [f32; 3], cell_size: f32) {
        let cell_size = cell_size.clamp(MIN_CELL, MAX_CELL);
        if self.cell_size != cell_size {
            self.cell_size = cell_size;
            self.floor_cache.clear();
            // Las celdas son índices de rejilla: con otro tamaño de celda los
            // mismos índices apuntan a otra parte del mundo (y sin nivelación
            // entre celdas jamás se drenarían). Se reconstruye desde cero.
            self.cells.clear();
            for source in &mut self.sources {
                source.cell_size = cell_size;
                source.cell = snap(source.position, cell_size);
            }
        }
        if self.cell_size == 0.0 {
            self.cell_size = cell_size;
        }
        match self.sources.iter_mut().find(|source| source.id == id) {
            Some(source) => *source = WaterSource::new(id, position, cell_size),
            None => self.sources.push(WaterSource::new(id, position, cell_size)),
        }
        self.dirty = true;
    }

    pub(crate) fn remove_source(&mut self, id: u64) {
        let before = self.sources.len();
        self.sources.retain(|source| source.id != id);
        if self.sources.len() != before && self.sources.is_empty() {
            self.cells.clear();
        }
        self.dirty = true;
    }

    pub(crate) fn clear(&mut self) {
        self.sources.clear();
        self.cells.clear();
        self.floor_cache.clear();
        self.dirty = true;
    }

    /// Columnas XZ que hay que consultar: celdas vivas, sus vecinas laterales y
    /// las fuentes. Con eso basta para decidir caída y expansión.
    pub(crate) fn columns_to_query(&self) -> HashSet<(i32, i32)> {
        let mut out = HashSet::new();
        let mut add = |cell: Cell| {
            out.insert((cell.0, cell.2));
            out.insert((cell.0 + 1, cell.2));
            out.insert((cell.0 - 1, cell.2));
            out.insert((cell.0, cell.2 + 1));
            out.insert((cell.0, cell.2 - 1));
        };
        for &cell in self.cells.keys() {
            add(cell);
        }
        for source in &self.sources {
            add(source.cell);
        }
        out
    }

    /// Un tick del autómata. `floors` son las alturas de terreno por columna.
    pub(crate) fn step(&mut self, floors: &HashMap<(i32, i32), f32>) {
        let cs = self.cell_size();
        let prev = self.cells.clone();
        let mut next: HashMap<Cell, u8> = HashMap::new();
        let mut source_cells: HashSet<Cell> = HashSet::new();

        // Resuelve primero las fuentes enterradas (lectura) para poder mutarlas
        // después sin solapar préstamos con el resto de la rejilla.
        let targets: Vec<Cell> = self
            .sources
            .iter()
            .map(|source| {
                let (ix, mut iy, iz) = source.cell;
                let mut guard = 0;
                while is_solid((ix, iy, iz), cs, floors) && guard < 16 {
                    iy += 1;
                    guard += 1;
                }
                (ix, iy, iz)
            })
            .collect();

        // Las fuentes primero: son infinitas y además alimentan la columna que
        // cae cuando no tienen suelo inmediato.
        for (source, target) in self.sources.iter_mut().zip(targets) {
            source.cell = target;
            let (ix, iy, iz) = target;
            source_cells.insert(target);
            let mut put = |cell: Cell, level: u8| {
                if !is_solid(cell, cs, floors) && next.len() < MAX_CELLS {
                    next.insert(cell, level);
                }
            };
            put(target, SOURCE_LEVEL);
            // Chorrito permanente hacia abajo: el agua de una fuente suspendida
            // cae hasta encontrar suelo y entonces se reparte.
            put((ix, iy - 1, iz), SOURCE_LEVEL);
            // La fuente también se reparte en este mismo tick: si solo se
            // mantuviera a sí misma, el agua nunca saldría de la celda inicial.
            for neighbor in [
                (ix + 1, iy, iz),
                (ix - 1, iy, iz),
                (ix, iy, iz + 1),
                (ix, iy, iz - 1),
            ] {
                if !is_solid(neighbor, cs, floors)
                    && !prev.contains_key(&neighbor)
                    && !source_cells.contains(&neighbor)
                    && next.get(&neighbor).map_or(true, |&l| l < SOURCE_LEVEL - 1)
                    && next.len() < MAX_CELLS
                {
                    next.insert(neighbor, SOURCE_LEVEL - 1);
                }
            }
        }

        for (cell, level) in &prev {
            if source_cells.contains(cell) || is_solid(*cell, cs, floors) {
                continue;
            }
            let (ix, iy, iz) = *cell;
            let below = (ix, iy - 1, iz);
            let can_fall = !is_solid(below, cs, floors)
                && !prev.contains_key(&below)
                && !source_cells.contains(&below);
            if can_fall {
                if next.len() < MAX_CELLS {
                    next.insert(below, *level);
                }
                continue;
            }
            // Sin hueco debajo: repartirse a los lados con un nivel menos.
            if *level > 1 {
                for neighbor in [
                    (ix + 1, iy, iz),
                    (ix - 1, iy, iz),
                    (ix, iy, iz + 1),
                    (ix, iy, iz - 1),
                ] {
                    if is_solid(neighbor, cs, floors)
                        || prev.contains_key(&neighbor)
                        || source_cells.contains(&neighbor)
                    {
                        continue;
                    }
                    if next.get(&neighbor).map_or(true, |&l| l < level - 1)
                        && next.len() < MAX_CELLS
                    {
                        next.insert(neighbor, level - 1);
                    }
                }
            }
            // El agua que no pudo moverse permanece (o se refuerza si varios
            // vecinos la alimentan).
            next.entry(*cell)
                .and_modify(|l| *l = (*l).max(*level))
                .or_insert(*level);
        }

        // Un piso por debajo de la rejilla es sólido: evita caída infinita en
        // escenas sin colisionadores (el raycast devuelve 0.0 por defecto).
        let cs_min = cell_bottom((0, -1, 0), cs);
        next.retain(|cell, _| cell_bottom(*cell, cs) > cs_min);

        if next != self.cells {
            self.cells = next;
            self.dirty = true;
        }
    }

    /// Vértices, índices y AABB de la superficie de agua. Se omiten las caras
    /// cubiertas por otra celda de agua para no pagar triángulos invisibles.
    pub(crate) fn build_mesh(&self) -> (Vec<Vertex>, Vec<u32>, [f32; 3], [f32; 3]) {
        let cs = self.cell_size();
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];

        for (&cell, &level) in &self.cells {
            let x0 = cell.0 as f32 * cs;
            let x1 = x0 + cs;
            let z0 = cell.2 as f32 * cs;
            let z1 = z0 + cs;
            let y0 = cell_bottom(cell, cs);
            let y1 = y0 + (level as f32 + 1.0) / 8.0 * cs;
            min = [min[0].min(x0), min[1].min(y0), min[2].min(z0)];
            max = [max[0].max(x1), max[1].max(y1), max[2].max(z1)];

            let covered = |neighbor: Cell| self.cells.get(&neighbor).is_some_and(|&l| l >= level);
            // Cara superior (+Y).
            if !covered((cell.0, cell.1 + 1, cell.2)) {
                quad(
                    &mut vertices,
                    &mut indices,
                    [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
                    [0.0, 1.0, 0.0],
                );
            }
            // +X.
            if !covered((cell.0 + 1, cell.1, cell.2)) {
                quad(
                    &mut vertices,
                    &mut indices,
                    [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
                    [1.0, 0.0, 0.0],
                );
            }
            // -X.
            if !covered((cell.0 - 1, cell.1, cell.2)) {
                quad(
                    &mut vertices,
                    &mut indices,
                    [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
                    [-1.0, 0.0, 0.0],
                );
            }
            // +Z.
            if !covered((cell.0, cell.1, cell.2 + 1)) {
                quad(
                    &mut vertices,
                    &mut indices,
                    [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
                    [0.0, 0.0, 1.0],
                );
            }
            // -Z.
            if !covered((cell.0, cell.1, cell.2 - 1)) {
                quad(
                    &mut vertices,
                    &mut indices,
                    [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
                    [0.0, 0.0, -1.0],
                );
            }
        }
        if vertices.is_empty() {
            return (
                vec![Vertex {
                    position: [0.0; 3],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 0.0],
                }],
                vec![0, 0, 0],
                [0.0; 3],
                [0.0; 3],
            );
        }
        (vertices, indices, min, max)
    }
}

fn quad(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    corners: [[f32; 3]; 4],
    normal: [f32; 3],
) {
    let base = vertices.len() as u32;
    for position in corners {
        vertices.push(Vertex {
            position,
            normal,
            uv: [0.0, 0.0],
        });
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 3, base]);
}

impl crate::engine3d::engine::State {
    /// Alta desde el ghost del viewport. Se snapped a la rejilla; el primer
    /// step ya consulta el suelo y recoloca la fuente si cae dentro del terreno.
    pub(crate) fn water_add_source(&mut self, id: u64, position: [f32; 3], cell_size: f32) {
        self.water.add_source(id, position, cell_size);
        self.water_step_once();
    }

    pub(crate) fn water_remove_source(&mut self, id: u64) {
        self.water.remove_source(id);
        self.water_step_once();
    }

    pub(crate) fn water_clear(&mut self) {
        self.water.clear();
        self.water_step_once();
    }

    /// Avance del autómata a 5 Hz, enganchado al update del motor.
    pub(crate) fn water_tick(&mut self, dt: f32) {
        if self.water.is_empty() {
            return;
        }
        self.water.tick_accum += dt.min(0.05);
        if self.water.tick_accum < TICK_INTERVAL {
            return;
        }
        self.water.tick_accum -= TICK_INTERVAL;
        self.water_step_once();
    }

    /// Consulta el suelo por raycast y ejecuta un tick (o regenera la malla si
    /// solo cambió la configuración). Público en el crate para la restauración
    /// de escenas desde `particles::restore_particles_from_value`.
    pub(crate) fn water_step_once(&mut self) {
        let columns = self.water.columns_to_query();
        let cs = self.water.cell_size();
        for (ix, iz) in &columns {
            if self.water.floor_cache.contains_key(&(*ix, *iz)) {
                continue;
            }
            let x = (*ix as f32 + 0.5) * cs;
            let z = (*iz as f32 + 0.5) * cs;
            let origin = Vec3::new(x, RAY_TOP, z);
            let floor = self
                .physics
                .raycast_first_hit_distance(origin, Vec3::NEG_Y, RAY_MAX, &[])
                .map(|distance| RAY_TOP - distance)
                .unwrap_or(0.0);
            self.water.floor_cache.insert((*ix, *iz), floor);
        }
        let floors = self.water.floor_cache.clone();
        self.water.step(&floors);
        if self.water.dirty {
            self.water_sync_mesh();
            self.water.dirty = false;
        }
    }

    /// Sube la geometría resultante y la deja dibujándose como entidad estática.
    fn water_sync_mesh(&mut self) {
        let (vertices, indices, min, max) = self.water.build_mesh();
        let mesh = crate::engine3d::mesh::upload(&self.vulkan, &vertices, &indices, "water", None);
        let mesh_idx = if let Some(index) = self.water.mesh_idx {
            self.meshes[index] = mesh;
            index
        } else {
            let index = self.meshes.len();
            self.meshes.push(mesh);
            self.water.mesh_idx = Some(index);
            index
        };
        let tex_idx = if let Some(index) = self.water.tex_idx {
            index
        } else {
            let index = self.ensure_plane_tool_colored_texture([56, 140, 222, 255]);
            self.water.tex_idx = Some(index);
            index
        };
        if let Some(entity) = self.water.entity {
            self.world
                .insert(entity, MeshComponent { mesh_idx, tex_idx });
            if let Some(aabb) = self.world.get_mut::<MeshLocalAabb>(entity) {
                aabb.min = Vec3::from(min);
                aabb.max = Vec3::from(max);
            } else {
                self.world.insert(
                    entity,
                    MeshLocalAabb {
                        min: Vec3::from(min),
                        max: Vec3::from(max),
                    },
                );
            }
        } else {
            let entity = self.world.spawn(Some("WaterSurface"));
            self.world
                .insert(entity, MeshComponent { mesh_idx, tex_idx });
            self.world.insert(
                entity,
                Transform {
                    position: Vec3::ZERO,
                    ..Default::default()
                },
            );
            self.world.insert(
                entity,
                MeshLocalAabb {
                    min: Vec3::from(min),
                    max: Vec3::from(max),
                },
            );
            self.world.insert(entity, NonSelectable);
            self.world.insert(
                entity,
                SurfacePbr {
                    roughness: 0.12,
                    metallic: 0.0,
                    ior: 1.33,
                    opacity: 0.62,
                },
            );
            self.water.entity = Some(entity);
        }
    }

    /// Las fuentes van en el mismo snapshot que los emisores (la UI las lista
    /// juntas) con material `water_block` y su tamaño de celda.
    pub(crate) fn water_export_into(&self, emitters: &mut Vec<SaveParticleEmitterSnapshot>) {
        for source in self.water.sources() {
            emitters.push(SaveParticleEmitterSnapshot {
                id: source.id,
                material: WATER_WIRE.to_string(),
                position: source.position,
                cell_size: Some(source.cell_size),
                soft: true,
                ..Default::default()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_floor(cells: &WaterSystem) -> HashMap<(i32, i32), f32> {
        cells
            .columns_to_query()
            .into_iter()
            .map(|column| (column, 0.0))
            .collect()
    }

    #[test]
    fn source_spreads_and_a_finite_puddle_forms() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 0.25, 0.25], 0.5);
        for _ in 0..30 {
            let floors = flat_floor(&water);
            water.step(&floors);
        }
        assert_eq!(
            water.cells.get(&(0, 0, 0)),
            Some(&SOURCE_LEVEL),
            "la fuente no se agota"
        );
        assert_eq!(water.cells.get(&(1, 0, 0)), Some(&6), "el agua se reparte");
        assert!(water.cell_count() > 20, "el charco debe extenderse");
        assert!(water.cell_count() < MAX_CELLS, "el charco debe ser finito");
        assert!(water.cells.values().all(|&level| level >= 1 && level <= 7));
    }

    #[test]
    fn water_falls_until_it_hits_the_floor() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 4.25, 0.25], 0.5);
        for _ in 0..30 {
            let floors = flat_floor(&water);
            water.step(&floors);
        }
        assert!(
            water
                .cells
                .keys()
                .any(|&(ix, iy, iz)| ix == 0 && iz == 0 && iy == 0),
            "el agua debe llegar al suelo"
        );
        assert!(
            water
                .cells
                .keys()
                .any(|&(ix, iy, iz)| iy == 0 && (ix != 0 || iz != 0)),
            "al llegar al suelo debe extenderse"
        );
    }

    #[test]
    fn buried_source_moves_above_the_terrain() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 0.25, 0.25], 0.5);
        let floors = flat_floor(&water)
            .into_iter()
            .map(|(column, _)| (column, 1.0))
            .collect();
        water.step(&floors);
        let source_cell = water.sources[0].cell;
        assert!(
            source_cell.1 >= 2,
            "la fuente enterrada debe subir sobre el terreno: {source_cell:?}"
        );
        assert_eq!(water.cells.get(&source_cell), Some(&SOURCE_LEVEL));
    }

    #[test]
    fn removing_the_last_source_empties_the_pool() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 0.25, 0.25], 0.5);
        for _ in 0..10 {
            let floors = flat_floor(&water);
            water.step(&floors);
        }
        assert!(water.cell_count() > 0);
        water.remove_source(1);
        assert!(water.cell_count() == 0 && water.is_empty());
    }
}
