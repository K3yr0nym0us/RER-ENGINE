//! Cubo de líquidos: autómata celular en CPU con volumen conservado y
//! superficie continua.
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
pub(crate) const LIQUID_WIRE: &str = "liquid_cube";
/// Tamaño de celda por defecto (m).
pub(crate) const DEFAULT_CELL_SIZE: f32 = 0.5;
/// Convierte un color RGB lineal del líquido a bytes para la textura; `boost`
/// aclara los canales (lo usa la prueba de saturación).
fn liquid_rgba(color: [f32; 3], boost: u8) -> [u8; 4] {
    let channel = |value: f32| (value * 255.0 + boost as f32).round().clamp(0.0, 255.0) as u8;
    [channel(color[0]), channel(color[1]), channel(color[2]), 255]
}

/// Albedo de la superficie: mismo tinte que el preset `water` de la demo de
/// reflejos (`water_rgba` en `pbr_presets.rs`), aplicado al color configurable.
fn liquid_surface_rgba(color: [f32; 3]) -> [u8; 4] {
    liquid_rgba([color[0], color[1] + 0.02, color[2] + 0.05], 0)
}

/// Propiedades por defecto del líquido común y corriente (agua).
/// Coincide con el `base_color` del preset `water` de la demo de reflejos.
pub(crate) const DEFAULT_LIQUID_COLOR: [f32; 3] = [0.08, 0.28, 0.42];
pub(crate) const DEFAULT_VISCOSITY: f32 = 0.15;
pub(crate) const DEFAULT_COHESION: f32 = 0.35;
/// Nivel máximo de una celda: 7 = altura `cs` (un cubo lleno). El agua se mide
/// en unidades de 1/8 de celda, así que una celda llena tiene 8 unidades.
const SOURCE_LEVEL: u8 = 7;
/// Volumen total que suelta una fuente, en unidades de 1/8 de celda (8 = un
/// cubo). Se reparte entre las celdas que alcanza: la altura inicial baja a
/// medida que crece el charco, hasta quedar en la mínima (1 unidad) en todas.
/// Ajusta este valor para charcos más grandes o más pequeños.
const SOURCE_VOLUME: u32 = 96;
/// Tick del autómata (5 Hz).
pub(crate) const TICK_INTERVAL: f32 = 0.2;
const MIN_CELL: f32 = 0.1;
const MAX_CELL: f32 = 4.0;
/// Tope de seguridad: un charco en una llanura infinita no puede crecer sin fin.
const MAX_CELLS: usize = 24_000;
/// Origen del raycast que busca el suelo de cada columna.
const RAY_TOP: f32 = 1000.0;
const RAY_MAX: f32 = 2000.0;
/// Margen con el que la fila final se apoya sobre el terreno. Si coincide con
/// la altura del suelo, la malla y el terreno hacen z-fighting y parpadean al
/// mover la cámara.
const EDGE_LIFT: f32 = 0.02;
/// Radio (en celdas) del redondeo de la unión entre fuentes cercanas. Con una
/// unión dura los dos círculos se cortan en un pico cóncavo; `smooth_min` lo
/// rellena hacia afuera, como si la atracción superficial fusionara los charcos.
const SOURCE_BLEND: f32 = 2.5;
/// B1 — margen sobre la media de los 8 vecinos para aplicar suavizado de
/// curvatura. Solo corrige bultos reales; no aplana un charco ya nivelado.
const CURVATURE_MARGIN: f32 = 1.5;
/// B2 — parámetros del flujo con inercia (shallow-water simplificado). La
/// velocidad no mueve fracciones de unidad (eso rompería la conservación
/// entera): sesga qué cesiones se permiten y se transporta con el agua.
const SW_GRAVITY: f32 = 0.16;
const SW_MAX_SPEED: f32 = 1.4;
const SW_MIN_PUSH: f32 = 0.12;
const SW_VELOCITY_SHARE: f32 = 0.35;

/// Celda de la rejilla (índices X, Y, Z).
type Cell = (i32, i32, i32);

/// Fuente colocada desde la UI. Guarda su posición para poder reconstruir la
/// rejilla al cargar una escena o al cambiar el tamaño de celda.
#[derive(Debug, Clone)]
pub(crate) struct WaterSource {
    pub id: u64,
    pub position: [f32; 3],
    pub cell_size: f32,
    /// Propiedades del líquido de esta fuente.
    pub color: [f32; 3],
    pub viscosity: f32,
    pub cohesion: f32,
    /// Celda snapped (puede moverse hacia arriba si el terreno la tapa).
    cell: Cell,
}

impl WaterSource {
    fn with_liquid(
        id: u64,
        position: [f32; 3],
        cell_size: f32,
        color: [f32; 3],
        viscosity: f32,
        cohesion: f32,
    ) -> Self {
        let cell = snap(position, cell_size);
        Self {
            id,
            position,
            cell_size,
            color,
            viscosity,
            cohesion,
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

/// B9 — contadores ligeros del sistema de agua, para medir coste sin tocar el
/// resultado de la simulación.
#[derive(Default, Debug, Clone, Copy)]
pub(crate) struct WaterStats {
    pub steps: u64,
    pub cells: usize,
    pub mesh_vertices: usize,
    pub mesh_indices: usize,
    pub terrain_points: usize,
}

/// Altura del terreno en un punto arbitrario, interpolada bilinealmente desde
/// el centro de las columnas ya consultadas por raycast.
fn terrain_height_at(x: f32, z: f32, floors: &HashMap<(i32, i32), f32>, cell_size: f32) -> f32 {
    let fx = x / cell_size - 0.5;
    let fz = z / cell_size - 0.5;
    let ix = fx.floor() as i32;
    let iz = fz.floor() as i32;
    let tx = fx - ix as f32;
    let tz = fz - iz as f32;
    let h00 = floors.get(&(ix, iz)).copied().unwrap_or(0.0);
    let h10 = floors.get(&(ix + 1, iz)).copied().unwrap_or(h00);
    let h01 = floors.get(&(ix, iz + 1)).copied().unwrap_or(h00);
    let h11 = floors.get(&(ix + 1, iz + 1)).copied().unwrap_or(h00);
    (h00 * (1.0 - tx) + h10 * tx) * (1.0 - tz) + (h01 * (1.0 - tx) + h11 * tx) * tz
}

/// B5 — Nube de puntos que muestrea la superficie del terreno. Se construye
/// una sola vez y se reutiliza como frontera del solver de partículas, de modo
/// que el agua choque con el heightfield real y no con una caja genérica.
#[derive(Default)]
pub(crate) struct TerrainBoundary {
    points: Vec<[f32; 3]>,
    volumes: Vec<f32>,
    dirty: bool,
}

impl TerrainBoundary {
    fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Vuelve a muestrear la región cubierta por `floors` con separación de
    /// dos radios de partícula (o media celda, la que sea mayor).
    fn rebuild(&mut self, floors: &HashMap<(i32, i32), f32>, cell_size: f32, particle_radius: f32) {
        self.points.clear();
        self.volumes.clear();
        self.dirty = false;
        let Some(&(min_ix, min_iz)) = floors.keys().min_by_key(|column| (column.0, column.1))
        else {
            return;
        };
        let Some(&(max_ix, max_iz)) = floors.keys().max_by_key(|column| (column.0, column.1))
        else {
            return;
        };
        let spacing = (particle_radius * 2.0).max(cell_size * 0.5).max(0.01);
        let x0 = min_ix as f32 * cell_size - spacing;
        let x1 = (max_ix as f32 + 1.0) * cell_size + spacing;
        let z0 = min_iz as f32 * cell_size - spacing;
        let z1 = (max_iz as f32 + 1.0) * cell_size + spacing;
        let volume = spacing * spacing * spacing;
        let mut x = x0;
        while x <= x1 + 1.0e-4 {
            let mut z = z0;
            while z <= z1 + 1.0e-4 {
                let y = terrain_height_at(x, z, floors, cell_size);
                self.points.push([x, y, z]);
                self.volumes.push(volume);
                z += spacing;
            }
            x += spacing;
        }
    }

    pub(crate) fn points(&self) -> &[[f32; 3]] {
        &self.points
    }

    pub(crate) fn volumes(&self) -> &[f32] {
        &self.volumes
    }
}

/// Rejilla de agua + estado de render. Toda la lógica del autómata es pura
/// (recibe las alturas de suelo ya consultadas por el motor).
#[derive(Default)]
pub(crate) struct WaterSystem {
    sources: Vec<WaterSource>,
    cells: HashMap<Cell, u8>,
    /// Unidades (1..=8) justo antes del último tick. La malla interpola entre
    /// este estado y `cells` para que el agua salga suavemente de la celda
    /// anterior en vez de aparecer de golpe.
    prev_units: HashMap<Cell, u32>,
    /// Volumen aún no vertido por cada fuente (unidades de 1/8 de celda).
    reservoir: HashMap<u64, u32>,
    /// B2 — velocidad horizontal por celda (unidades de celda/tick). Da inercia
    /// al reparto: el agua puede seguir avanzando poco después de nivelarse.
    velocity: HashMap<Cell, [f32; 2]>,
    cell_size: f32,
    /// Propiedades del líquido compartidas por el charco (las pone la última
    /// fuente alta/editada; una sola malla implica un solo color por charco).
    color: [f32; 3],
    viscosity: f32,
    cohesion: f32,
    /// Suelo ya consultado por columna: el raycast contra el trimesh del terreno
    /// es caro y la altura de una columna no cambia mientras no se regenere.
    floor_cache: HashMap<(i32, i32), f32>,
    /// B5 — frontera muestreada del terreno para el solver de partículas.
    terrain: TerrainBoundary,
    /// Columnas que hay que consultar, recalculadas solo cuando cambia la
    /// rejilla. Evita reconstruir un `HashSet` nuevo en cada tick (5 Hz).
    query_cache: HashSet<(i32, i32)>,
    /// B9 — contadores del último estado conocido.
    stats: WaterStats,
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

    /// B9 — instantánea de contadores (sin cronómetros, coste despreciable).
    /// La consume el futuro panel de perfilado; se mantiene disponible mientras
    /// no haya interfaz que la muestre.
    #[allow(dead_code)]
    pub(crate) fn stats(&self) -> WaterStats {
        self.stats
    }

    /// ¿Queda una transición entre ticks por dibujar? Mientras sea `true` la
    /// malla se reinterpola cada frame con el alpha del acumulador.
    pub(crate) fn is_animating(&self) -> bool {
        self.prev_units.len() != self.cells.len()
            || self
                .cells
                .iter()
                .any(|(&cell, &level)| self.prev_units.get(&cell) != Some(&(level as u32 + 1)))
    }

    /// Fuentes pickables por el viewport: comparte el cableado de hover y click
    /// derecho de los emisores (F5).
    pub(crate) fn pickable(&self) -> Vec<(u64, [f32; 3])> {
        self.sources
            .iter()
            .map(|source| (source.id, source.position))
            .collect()
    }

    /// Alta o reemplazo de una fuente con propiedades de líquido por defecto.
    pub(crate) fn add_source(&mut self, id: u64, position: [f32; 3], cell_size: f32) {
        self.add_liquid_source(
            id,
            position,
            cell_size,
            DEFAULT_LIQUID_COLOR,
            DEFAULT_VISCOSITY,
            DEFAULT_COHESION,
        );
    }

    /// Alta o reemplazo con propiedades configurables. Todas las fuentes
    /// comparten el tamaño de celda y el color de la más reciente.
    pub(crate) fn add_liquid_source(
        &mut self,
        id: u64,
        position: [f32; 3],
        cell_size: f32,
        color: [f32; 3],
        viscosity: f32,
        cohesion: f32,
    ) {
        let cell_size = cell_size.clamp(MIN_CELL, MAX_CELL);
        self.color = [
            color[0].clamp(0.0, 1.0),
            color[1].clamp(0.0, 1.0),
            color[2].clamp(0.0, 1.0),
        ];
        self.viscosity = viscosity.clamp(0.0, 1.0);
        self.cohesion = cohesion.clamp(0.0, 1.0);
        if self.cell_size != cell_size {
            self.cell_size = cell_size;
            self.floor_cache.clear();
            self.terrain.invalidate();
            // Las celdas son índices de rejilla: con otro tamaño de celda los
            // mismos índices apuntan a otra parte del mundo (y sin nivelación
            // entre celdas jamás se drenarían). Se reconstruye desde cero.
            self.cells.clear();
            self.prev_units.clear();
            for source in &mut self.sources {
                source.cell_size = cell_size;
                source.cell = snap(source.position, cell_size);
            }
        }
        if self.cell_size == 0.0 {
            self.cell_size = cell_size;
        }
        match self.sources.iter_mut().find(|source| source.id == id) {
            Some(source) => {
                *source =
                    WaterSource::with_liquid(id, position, cell_size, color, viscosity, cohesion)
            }
            None => self.sources.push(WaterSource::with_liquid(
                id, position, cell_size, color, viscosity, cohesion,
            )),
        }
        // Una fuente nueva trae su propio cubo: se recarga el reservorio.
        self.reservoir.insert(id, SOURCE_VOLUME);
        self.refresh_query_cache();
        self.dirty = true;
    }

    pub(crate) fn remove_source(&mut self, id: u64) {
        let before = self.sources.len();
        self.sources.retain(|source| source.id != id);
        self.reservoir.remove(&id);
        if self.sources.len() != before && self.sources.is_empty() {
            self.cells.clear();
            self.prev_units.clear();
        }
        self.refresh_query_cache();
        self.dirty = true;
    }

    pub(crate) fn clear(&mut self) {
        self.sources.clear();
        self.cells.clear();
        self.prev_units.clear();
        self.reservoir.clear();
        self.velocity.clear();
        self.floor_cache.clear();
        self.terrain.invalidate();
        self.refresh_query_cache();
        self.dirty = true;
    }

    /// Columnas XZ que hay que consultar: celdas vivas, las 24 columnas que las
    /// rodean (el anillo en rampa usa su suelo) y las fuentes.
    /// Se recalcula solo cuando cambia la rejilla; reutiliza la capacidad del
    /// `HashSet` anterior en vez de asignar uno nuevo en cada tick.
    fn refresh_query_cache(&mut self) {
        let mut out = std::mem::take(&mut self.query_cache);
        out.clear();
        let mut add = |cell: Cell| {
            for dx in -2..=2 {
                for dz in -2..=2 {
                    out.insert((cell.0 + dx, cell.2 + dz));
                }
            }
        };
        for &cell in self.cells.keys() {
            add(cell);
        }
        for source in &self.sources {
            add(source.cell);
        }
        self.query_cache = out;
    }

    /// Cache de columnas del estado actual (ver `refresh_query_cache`).
    pub(crate) fn columns_to_query(&self) -> &HashSet<(i32, i32)> {
        &self.query_cache
    }

    /// Quita del caché de suelos las columnas que ya no están cerca del agua.
    /// Sin esto, mover fuentes acumularía raycasts viejos indefinidamente.
    fn prune_floor_cache(&mut self) {
        self.floor_cache
            .retain(|column, _| self.query_cache.contains(column));
    }

    /// Un tick del autómata. `floors` son las alturas de terreno por columna.
    ///
    /// El agua se CONSERVA: cada fuente suelta un volumen finito (`SOURCE_VOLUME`)
    /// que se reparte entre las celdas que alcanza. La altura de la fuente baja a
    /// medida que el charco crece y al final todas las celdas quedan en la altura
    /// mínima (1 unidad = 1/8 de celda), en vez de haber una fuente infinita con
    /// un cono alto en el centro.
    pub(crate) fn step(&mut self, floors: &HashMap<(i32, i32), f32>) {
        let cs = self.cell_size();
        // La viscosidad del líquido se traduce en amortiguamiento de la
        // inercia: 0.15 (agua) ≈ 0.86, 1.0 (jarabe) ≈ 0.35.
        let damping = 0.95 - 0.6 * self.viscosity;
        // Unidades de agua por celda (1..=8). El nivel guardado es unidades - 1.
        let mut units: HashMap<Cell, u32> = self
            .cells
            .iter()
            .map(|(&cell, &level)| (cell, level as u32 + 1))
            .collect();
        // Estado de partida para la interpolación. En el PRIMER llenado se
        // iguala al resultado: el cubo inicial aparece entero y son las celdas
        // vecinas las que salen de él en los ticks siguientes.
        let before = units.clone();
        let first_fill = before.is_empty();
        let mut injected: HashSet<Cell> = HashSet::new();

        // Fuentes enterradas: se recolocan hacia arriba. Sus celdas se resuelven
        // antes de mutarlas para no solapar préstamos con la rejilla.
        let targets: Vec<(u64, Cell)> = self
            .sources
            .iter()
            .map(|source| {
                let (ix, mut iy, iz) = source.cell;
                let mut guard = 0;
                while is_solid((ix, iy, iz), cs, floors) && guard < 16 {
                    iy += 1;
                    guard += 1;
                }
                (source.id, (ix, iy, iz))
            })
            .collect();

        // Cada fuente vierte lo que le queda de su cubo, hasta llenar su celda.
        for &(id, target) in &targets {
            let remaining = self.reservoir.get(&id).copied().unwrap_or(0);
            let current = units.get(&target).copied().unwrap_or(0);
            let added = remaining.min(8u32.saturating_sub(current));
            if added > 0 {
                units.insert(target, current + added);
                self.reservoir.insert(id, remaining - added);
                // La celda que acaba de recibir el cubo NO reparte en este tick:
                // primero se ve el cubo, después salen los vecinos de él.
                injected.insert(target);
            }
        }
        for source in &mut self.sources {
            if let Some((_, target)) = targets.iter().find(|(id, _)| *id == source.id) {
                source.cell = *target;
            }
        }

        // Caída: se recogen todas las transferencias desde un snapshot y se
        // aplican al final, para que el resultado no dependa del orden en que
        // el HashMap devuelva las celdas (mismo estado → mismo charco).
        let falling_from = units.clone();
        let mut fall_give: HashMap<Cell, u32> = HashMap::new();
        let mut fall_take: HashMap<Cell, u32> = HashMap::new();
        let mut falling_cells: Vec<Cell> = falling_from.keys().copied().collect();
        falling_cells.sort_unstable();
        for cell in falling_cells {
            let amount = falling_from[&cell];
            let given = fall_give.get(&cell).copied().unwrap_or(0);
            let current = amount.saturating_sub(given);
            if current == 0 {
                continue;
            }
            let (ix, iy, iz) = cell;
            let below = (ix, iy - 1, iz);
            if is_solid(below, cs, floors) {
                continue;
            }
            let occupied = falling_from.get(&below).copied().unwrap_or(0);
            let taken = fall_take.get(&below).copied().unwrap_or(0);
            let space = 8u32.saturating_sub(occupied + taken);
            if space == 0 {
                continue;
            }
            let moved = space.min(current);
            fall_give.insert(cell, given + moved);
            fall_take.insert(below, taken + moved);
        }
        let mut flowed = units;
        apply_transfers(&mut flowed, &fall_give, &fall_take);

        // B2 — velocidad horizontal desde el gradiente de altura con inercia y
        // amortiguamiento. No mueve fracciones de unidad: sesga qué cesiones se
        // permiten y se transporta con el agua cuando una cesión se acepta.
        let mut velocity = HashMap::with_capacity(self.velocity.len());
        let mut vel_cells: Vec<Cell> = flowed.keys().copied().collect();
        vel_cells.sort_unstable();
        for cell in vel_cells {
            let (ix, iy, iz) = cell;
            let h_xp = flowed.get(&(ix + 1, iy, iz)).copied().unwrap_or(0) as f32;
            let h_xm = flowed.get(&(ix - 1, iy, iz)).copied().unwrap_or(0) as f32;
            let h_zp = flowed.get(&(ix, iy, iz + 1)).copied().unwrap_or(0) as f32;
            let h_zm = flowed.get(&(ix, iy, iz - 1)).copied().unwrap_or(0) as f32;
            let v = self.velocity.get(&cell).copied().unwrap_or([0.0; 2]);
            let mut next = [
                (v[0] - SW_GRAVITY * (h_xp - h_xm) * 0.5) * damping,
                (v[1] - SW_GRAVITY * (h_zp - h_zm) * 0.5) * damping,
            ];
            let speed = next[0].hypot(next[1]);
            if speed > SW_MAX_SPEED {
                let scale = SW_MAX_SPEED / speed;
                next[0] *= scale;
                next[1] *= scale;
            }
            velocity.insert(cell, next);
        }

        // Reparto lateral: gravedad (diferencia ≥2 unidades) o inercia (el
        // vector de velocidad empuja hacia el vecino aunque ya esté nivelado).
        let spreading_from = flowed.clone();
        let mut proposals: Vec<(Cell, Cell)> = Vec::new();
        let mut spreading_cells: Vec<Cell> = spreading_from.keys().copied().collect();
        spreading_cells.sort_unstable();
        for cell in spreading_cells {
            if injected.contains(&cell) {
                continue;
            }
            let amount = spreading_from[&cell];
            if amount <= 1 {
                continue;
            }
            let (ix, iy, iz) = cell;
            let v = velocity.get(&cell).copied().unwrap_or([0.0; 2]);
            for (dx, dz, neighbor) in [
                (1i32, 0i32, (ix + 1, iy, iz)),
                (-1, 0, (ix - 1, iy, iz)),
                (0, 1, (ix, iy, iz + 1)),
                (0, -1, (ix, iy, iz - 1)),
            ] {
                if is_solid(neighbor, cs, floors) {
                    continue;
                }
                let neighbor_amount = spreading_from.get(&neighbor).copied().unwrap_or(0);
                if amount <= neighbor_amount {
                    continue;
                }
                let push = v[0] * dx as f32 + v[1] * dz as f32;
                if amount < neighbor_amount + 2 && push <= SW_MIN_PUSH {
                    continue;
                }
                proposals.push((cell, neighbor));
            }
        }
        let mut spread = flowed;
        let accepted = commit_flow(&mut spread, proposals);
        // La velocidad viaja con el agua cedida (snapshot → aplicar junto).
        let mut vel_give: HashMap<Cell, [f32; 2]> = HashMap::new();
        let mut vel_take: HashMap<Cell, [f32; 2]> = HashMap::new();
        for (cell, neighbor) in accepted {
            if let Some(&v) = velocity.get(&cell) {
                let share = [v[0] * SW_VELOCITY_SHARE, v[1] * SW_VELOCITY_SHARE];
                vel_give
                    .entry(cell)
                    .and_modify(|sum| {
                        sum[0] += share[0];
                        sum[1] += share[1];
                    })
                    .or_insert(share);
                vel_take
                    .entry(neighbor)
                    .and_modify(|sum| {
                        sum[0] += share[0];
                        sum[1] += share[1];
                    })
                    .or_insert(share);
            }
        }
        for (cell, delta) in vel_give {
            if let Some(v) = velocity.get_mut(&cell) {
                v[0] -= delta[0];
                v[1] -= delta[1];
            }
        }
        for (cell, delta) in vel_take {
            let entry = velocity.entry(cell).or_insert([0.0; 2]);
            entry[0] += delta[0];
            entry[1] += delta[1];
        }

        // B1 — suavizado por curvatura con estencil de 8 vecinos: una celda muy
        // por encima de la media cede una unidad al vecino más seco. Conserva
        // el volumen y borra bultos residuales sin aplanar el charco nivelado.
        let curvature_from = spread.clone();
        let mut curvature_proposals: Vec<(Cell, Cell)> = Vec::new();
        let mut curvature_cells: Vec<Cell> = curvature_from.keys().copied().collect();
        curvature_cells.sort_unstable();
        for cell in curvature_cells {
            if injected.contains(&cell) {
                continue;
            }
            let amount = curvature_from[&cell];
            if amount <= 2 {
                continue;
            }
            let (ix, iy, iz) = cell;
            let mut sum = 0u32;
            let mut count = 0u32;
            let mut driest: Option<(Cell, u32)> = None;
            for dx in -1..=1 {
                for dz in -1..=1 {
                    if dx == 0 && dz == 0 {
                        continue;
                    }
                    let neighbor = (ix + dx, iy, iz + dz);
                    if is_solid(neighbor, cs, floors) {
                        continue;
                    }
                    let other = curvature_from.get(&neighbor).copied().unwrap_or(0);
                    sum += other;
                    count += 1;
                    if driest.is_none_or(|(_, best)| other < best) {
                        driest = Some((neighbor, other));
                    }
                }
            }
            if count == 0 {
                continue;
            }
            let average = sum as f32 / count as f32;
            if amount as f32 >= average + CURVATURE_MARGIN
                && let Some((neighbor, other)) = driest
                && amount > other
            {
                curvature_proposals.push((cell, neighbor));
            }
        }
        commit_flow(&mut spread, curvature_proposals);

        // Un piso por debajo de la rejilla es sólido: evita caída infinita en
        // escenas sin colisionadores (el raycast devuelve 0.0 por defecto).
        let cs_min = cell_bottom((0, -1, 0), cs);
        spread.retain(|cell, _| cell_bottom(*cell, cs) > cs_min);

        velocity.retain(|cell, _| spread.contains_key(cell));
        self.velocity = velocity;

        let next: HashMap<Cell, u8> = spread
            .into_iter()
            .filter(|(_, amount)| *amount > 0)
            .map(|(cell, amount)| (cell, (amount - 1) as u8))
            .collect();
        self.prev_units = if first_fill {
            // El cubo inicial se muestra entero desde el primer frame; la
            // animación empieza en el tick siguiente, cuando sale hacia afuera.
            next.iter()
                .map(|(&cell, &level)| (cell, level as u32 + 1))
                .collect()
        } else {
            before
        };
        self.stats.steps += 1;
        self.stats.cells = next.len();
        if next != self.cells {
            self.cells = next;
            self.refresh_query_cache();
            self.dirty = true;
        }
    }

    /// Vértices, índices y AABB de la superficie de agua.
    ///
    /// La malla es un campo de alturas **continuo**: en cada vértice de la
    /// rejilla se promedia el techo de las columnas que lo comparten, así que el
    /// agua se nivela de punta a punta sin escalones entre celdas. El charco se
    /// cierra con una fila extra de celdas secas que bajan en rampa desde la
    /// última altura hasta el suelo, en vez de cortarse en una pared vertical.
    pub(crate) fn build_mesh(
        &self,
        floors: &HashMap<(i32, i32), f32>,
        alpha: f32,
    ) -> (Vec<Vertex>, Vec<u32>, [f32; 3], [f32; 3]) {
        let cs = self.cell_size();
        // La malla se reconstruye cada frame durante la animación: reservar de
        // entrada evita varias reasignaciones por frame.
        let cell_hint = self.cells.len().max(self.prev_units.len());
        let mut vertices = Vec::with_capacity(cell_hint * 24);
        let mut indices = Vec::with_capacity(cell_hint * 36);
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];

        // Unidades interpoladas entre el estado anterior y el actual. Una celda
        // nueva nace con 0 unidades y crece hasta la suya en un tick; la que la
        // alimenta baja al mismo tiempo. Así el agua parece salir de la celda
        // anterior en vez de aparecer de la nada.
        let mut animated: HashMap<Cell, f32> = HashMap::new();
        for (&cell, &units) in &self.prev_units {
            animated.insert(cell, units as f32 * (1.0 - alpha));
        }
        for (&cell, &level) in &self.cells {
            let entry = animated.entry(cell).or_insert(0.0);
            *entry += (level as f32 + 1.0) * alpha;
        }
        animated.retain(|_, units| *units > 1.0e-4);

        // Techo y suelo de cada columna XZ: el agua se apila en Y.
        let mut column_top: HashMap<(i32, i32), f32> = HashMap::new();
        let mut column_bottom: HashMap<(i32, i32), f32> = HashMap::new();
        for (&cell, &units) in &animated {
            let bottom = cell_bottom(cell, cs);
            let top = bottom + units / 8.0 * cs;
            let col = (cell.0, cell.2);
            column_top
                .entry(col)
                .and_modify(|v| *v = v.max(top))
                .or_insert(top);
            column_bottom
                .entry(col)
                .and_modify(|v| *v = v.min(bottom))
                .or_insert(bottom);
            let x0 = cell.0 as f32 * cs;
            let z0 = cell.2 as f32 * cs;
            min = [min[0].min(x0), min[1].min(bottom), min[2].min(z0)];
            max = [max[0].max(x0 + cs), max[1].max(top), max[2].max(z0 + cs)];
        }
        if column_top.is_empty() {
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

        // Suelo real (raycast) en cada vértice: el MÁXIMO de las columnas que lo
        // tocan, para que agua y fila final queden por encima del terreno y no
        // se superpongan con él.
        let mut floor_at_vertex: HashMap<(i32, i32), f32> = HashMap::new();
        for (&(cx, cz), &floor) in floors {
            for gx in cx..=cx + 1 {
                for gz in cz..=cz + 1 {
                    floor_at_vertex
                        .entry((gx, gz))
                        .and_modify(|v| *v = v.max(floor))
                        .or_insert(floor);
                }
            }
        }

        // Altura en cada VÉRTICE mojado: media de los techos de las columnas que
        // lo comparten. El suelo máximo de esas columnas es el límite inferior
        // para que la rampa no se hunda bajo un desnivel; el suelo del raycast
        // con margen evita que el agua muy fina se corte contra el terreno.
        let mut accum: HashMap<(i32, i32), (f32, u32, f32)> = HashMap::new();
        for (&(ix, iz), &top) in &column_top {
            let bottom = column_bottom[&(ix, iz)];
            for corner in [(ix, iz), (ix + 1, iz), (ix, iz + 1), (ix + 1, iz + 1)] {
                let entry = accum.entry(corner).or_insert((0.0, 0, f32::MIN));
                entry.0 += top;
                entry.1 += 1;
                entry.2 = entry.2.max(bottom);
            }
        }
        let surface: HashMap<(i32, i32), f32> = accum
            .into_iter()
            .map(|(corner, (sum, count, bottom))| {
                let floor = floor_at_vertex.get(&corner).copied().unwrap_or(f32::MIN);
                (
                    corner,
                    (sum / count as f32).max(bottom).max(floor + EDGE_LIFT),
                )
            })
            .collect();

        // Extiende el campo dos celdas hacia afuera. Donde no hay agua, la altura
        // es el suelo local de las columnas mojadas cercanas, levantado un margen
        // sobre el terreno: la superficie baja en rampa desde el borde del charco
        // hasta apoyarse justo por encima del suelo. Dos filas dan sitio a las
        // celdas intermedias que redondean el rombo en las diagonales.
        let mut ground: HashMap<(i32, i32), f32> = HashMap::new();
        for (&(ix, iz), &bottom) in &column_bottom {
            for gx in (ix - 2)..=(ix + 3) {
                for gz in (iz - 2)..=(iz + 3) {
                    let floor = floor_at_vertex.get(&(gx, gz)).copied().unwrap_or(f32::MIN);
                    // `min_bottom` evita rampas hacia el suelo lejano cuando el
                    // agua cae en el aire; `floor + EDGE_LIFT` evita el parpadeo.
                    let edge = bottom.max(floor + EDGE_LIFT);
                    ground
                        .entry((gx, gz))
                        .and_modify(|v| *v = (*v).min(edge))
                        .or_insert(edge);
                }
            }
        }
        let heightmap: HashMap<(i32, i32), f32> = ground
            .iter()
            .map(|(&corner, &floor)| (corner, surface.get(&corner).copied().unwrap_or(floor)))
            .collect();

        // El autómata reparte por distancia MANHATTAN (4 vecinos), lo que dibuja
        // un rombo. Para la malla usamos una distancia EUCLIDEA a las fuentes:
        // el radio sale del nivel más bajo alcanzado (7 - level pasos). El corte
        // de esta señal por cero da un contorno circular, no un rombo.
        // Se restan DOS celdas al alcance: el agua sigue propagándose igual,
        // pero el círculo dibujado crece una celda menos que antes (mismo
        // comportamiento, menos distancia máxima por cubo).
        let steps = self
            .cells
            .values()
            .fold(0u8, |max, &level| {
                max.max(SOURCE_LEVEL.saturating_sub(level))
            })
            .max(1);
        let radius = steps.saturating_sub(2).max(1) as f32;
        let sources_xy: Vec<(f32, f32)> = self
            .sources
            .iter()
            .map(|source| {
                (
                    (source.cell.0 as f32 + 0.5) * cs,
                    (source.cell.2 as f32 + 0.5) * cs,
                )
            })
            .collect();
        let radial: HashMap<(i32, i32), f32> = heightmap
            .keys()
            .map(|&(gx, gz)| {
                let x = gx as f32 * cs;
                let z = gz as f32 * cs;
                let distance = sources_xy
                    .iter()
                    .map(|&(sx, sz)| ((x - sx) * (x - sx) + (z - sz) * (z - sz)).sqrt() / cs)
                    .fold(f32::MAX, |nearest, d| smooth_min(nearest, d, SOURCE_BLEND));
                ((gx, gz), radius - distance)
            })
            .collect();

        let height = |gx: i32, gz: i32| -> f32 { heightmap.get(&(gx, gz)).copied().unwrap_or(0.0) };
        // Normal suave a partir del gradiente del campo de alturas, compartida
        // por la tapa y la fila final para que no haya costura de iluminación.
        let normal_at = |gx: i32, gz: i32| -> [f32; 3] {
            let here = height(gx, gz);
            let left = heightmap.get(&(gx - 1, gz)).copied().unwrap_or(here);
            let right = heightmap.get(&(gx + 1, gz)).copied().unwrap_or(here);
            let back = heightmap.get(&(gx, gz - 1)).copied().unwrap_or(here);
            let front = heightmap.get(&(gx, gz + 1)).copied().unwrap_or(here);
            let span_x =
                if heightmap.contains_key(&(gx - 1, gz)) && heightmap.contains_key(&(gx + 1, gz)) {
                    2.0 * cs
                } else {
                    cs
                };
            let span_z =
                if heightmap.contains_key(&(gx, gz - 1)) && heightmap.contains_key(&(gx, gz + 1)) {
                    2.0 * cs
                } else {
                    cs
                };
            let n = Vec3::new(-(right - left) / span_x, 1.0, -(front - back) / span_z).normalize();
            [n.x, n.y, n.z]
        };

        // Esquinas de una celda en orden 0,1,2,3 alrededor del cuadrado. `radial`
        // es la señal del contorno circular: positiva dentro del círculo, negativa
        // fuera. El recorte interpola dónde cruza cero en cada arista.
        let cell_corners = |ix: i32, iz: i32| -> [SurfaceCorner; 4] {
            let offsets = [(0, 0), (0, 1), (1, 1), (1, 0)];
            std::array::from_fn(|i| {
                let gx = ix + offsets[i].0;
                let gz = iz + offsets[i].1;
                let field = radial.get(&(gx, gz)).copied().unwrap_or(-1.0);
                SurfaceCorner {
                    x: gx as f32 * cs,
                    z: gz as f32 * cs,
                    height: height(gx, gz),
                    ground: ground.get(&(gx, gz)).copied().unwrap_or(0.0),
                    normal: normal_at(gx, gz),
                    radial: field,
                    inside: field > 0.0,
                }
            })
        };

        // Tapa del charco: una sola por columna (la de arriba), con la altura
        // interpolada del tick.
        for (&cell, &units) in &animated {
            let (ix, _, iz) = cell;
            let bottom = cell_bottom(cell, cs);
            let top = bottom + units / 8.0 * cs;
            if (top - column_top[&(ix, iz)]).abs() <= 1.0e-4 {
                emit_surface_cell(&mut vertices, &mut indices, cell_corners(ix, iz));
            }
        }

        // Filas finales: celdas secas hasta dos celdas del agua. Se recortan con
        // marching squares contra el círculo: las aristas se cortan donde la
        // señal radial cruza cero, dando una orilla redonda en vez de un rombo.
        let mut skirt: HashSet<(i32, i32)> = HashSet::new();
        // Techo máximo en el entorno de cada columna, calculado de una pasada.
        // Antes se reescaneaba TODA la rejilla por cada celda del anillo (O(n²)).
        let mut nearby_top: HashMap<(i32, i32), f32> = HashMap::new();
        for &(ix, iz) in column_top.keys() {
            let top = column_top[&(ix, iz)];
            for dx in -2..=2 {
                for dz in -2..=2 {
                    let col = (ix + dx, iz + dz);
                    nearby_top
                        .entry(col)
                        .and_modify(|value| *value = (*value).max(top))
                        .or_insert(top);
                    if !column_top.contains_key(&col) {
                        skirt.insert(col);
                    }
                }
            }
        }
        for (ix, iz) in skirt {
            let corners = cell_corners(ix, iz);
            // Solo se omite la celda si el terreno sobresale por encima de la
            // SUPERFICIE del charco (un pilar). Compararlo con el fondo dejaba
            // huecos en el anillo exterior por pequeñas variaciones del raycast,
            // que se veían como picos hacia adentro en el contorno.
            let water_top = nearby_top.get(&(ix, iz)).copied().unwrap_or(f32::MIN);
            if floors
                .get(&(ix, iz))
                .is_some_and(|&floor| floor > water_top + 1.0e-3)
            {
                continue;
            }
            for corner in &corners {
                grow_bounds(&mut min, &mut max, [corner.x, corner.height, corner.z]);
            }
            emit_surface_cell(&mut vertices, &mut indices, corners);
        }
        (vertices, indices, min, max)
    }
}

/// Aplica de una vez las unidades cedidas y recibidas calculadas desde un
/// snapshot. Resta primero y suma después: una celda puede dar y recibir en el
/// mismo tick (columnas que caen sobre agua existente).
fn apply_transfers(
    units: &mut HashMap<Cell, u32>,
    give: &HashMap<Cell, u32>,
    take: &HashMap<Cell, u32>,
) {
    for (&cell, &amount) in give {
        let current = units.get(&cell).copied().unwrap_or(0);
        if amount >= current {
            units.remove(&cell);
        } else {
            units.insert(cell, current - amount);
        }
    }
    for (&cell, &amount) in take {
        let current = units.get(&cell).copied().unwrap_or(0);
        units.insert(cell, (current + amount).min(8));
    }
}

/// Aplica una lista de cesiones de una unidad con los límites de capacidad y
/// `MAX_CELLS`. Devuelve las cesiones aceptadas (B2 las usa para transportar
/// velocidad). Todo se decide desde el estado de entrada y se aplica junto.
fn commit_flow(units: &mut HashMap<Cell, u32>, proposals: Vec<(Cell, Cell)>) -> Vec<(Cell, Cell)> {
    let mut give: HashMap<Cell, u32> = HashMap::new();
    let mut take: HashMap<Cell, u32> = HashMap::new();
    let mut accepted = Vec::new();
    let mut new_cells = 0usize;
    let base_len = units.len();
    for (cell, neighbor) in proposals {
        let amount = units.get(&cell).copied().unwrap_or(0);
        let given = give.get(&cell).copied().unwrap_or(0);
        if given >= amount.saturating_sub(1) {
            continue;
        }
        let other = units.get(&neighbor).copied().unwrap_or(0);
        let taken = take.get(&neighbor).copied().unwrap_or(0);
        if other + taken >= 8 {
            continue;
        }
        if other == 0 && !take.contains_key(&neighbor) {
            if base_len + new_cells >= MAX_CELLS {
                continue;
            }
            new_cells += 1;
        }
        give.insert(cell, given + 1);
        take.insert(neighbor, taken + 1);
        accepted.push((cell, neighbor));
    }
    apply_transfers(units, &give, &take);
    accepted
}

fn grow_bounds(min: &mut [f32; 3], max: &mut [f32; 3], point: [f32; 3]) {
    for axis in 0..3 {
        min[axis] = min[axis].min(point[axis]);
        max[axis] = max[axis].max(point[axis]);
    }
}

/// Esquina de una celda del campo de alturas. `radial` es la señal euclídea del
/// contorno (positiva dentro del círculo); `inside` es su signo. `ground` es la
/// altura de suelo con margen, usada para apoyar el borde de la rampa.
#[derive(Clone, Copy)]
struct SurfaceCorner {
    x: f32,
    z: f32,
    height: f32,
    ground: f32,
    normal: [f32; 3],
    radial: f32,
    inside: bool,
}

fn blend_normals(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    Vec3::lerp(Vec3::from(a), Vec3::from(b), t)
        .normalize()
        .to_array()
}

/// Mínimo suave (unión redondeada) con radio `k`, en unidades de celda. Donde
/// dos campos de distancia se parecen, devuelve un valor menor que el mínimo,
/// lo que empuja el contorno hacia afuera y funde los charcos sin el pico
/// cóncavo que deja `min`.
fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b * (1.0 - h) + a * h - k * h * (1.0 - h)
}

/// Emite un polígono (posición + normal por vértice) como abanico de triángulos.
fn emit_poly(vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>, poly: &[([f32; 3], [f32; 3])]) {
    if poly.len() < 3 {
        return;
    }
    let base = vertices.len() as u32;
    for (position, normal) in poly {
        vertices.push(Vertex {
            position: *position,
            normal: *normal,
            uv: [0.0, 0.0],
        });
    }
    for i in 1..poly.len() - 1 {
        indices.extend_from_slice(&[base, base + i as u32, base + (i + 1) as u32]);
    }
}

/// Recorte `marching squares` de una celda contra el contorno circular.
///
/// Cada arista se corta donde la señal radial cruza cero (interpolando entre
/// sus dos extremos), no a mitad de celda: el contorno sigue una circunferencia
/// en vez del rombo que produce la propagación Manhattan del autómata. Los
/// vértices compartidos cortan igual en ambas celdas, así que no hay huecos.
/// La altura del punto de corte es la del SUELO, para que la orilla apoye en el
/// terreno y no flote a media altura de la rampa.
fn emit_surface_cell(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    corners: [SurfaceCorner; 4],
) {
    let mask = corners
        .iter()
        .enumerate()
        .fold(0u8, |acc, (i, c)| acc | ((c.inside as u8) << i));
    if mask == 0 {
        return;
    }

    let corner_point = |i: usize| -> ([f32; 3], [f32; 3]) {
        let c = corners[i];
        ([c.x, c.height, c.z], c.normal)
    };
    let edge_point = |i: usize| -> ([f32; 3], [f32; 3]) {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let denominator = a.radial - b.radial;
        let t = if denominator.abs() < 1.0e-6 {
            0.5
        } else {
            (a.radial / denominator).clamp(0.0, 1.0)
        };
        (
            [
                a.x + (b.x - a.x) * t,
                a.ground + (b.ground - a.ground) * t,
                a.z + (b.z - a.z) * t,
            ],
            blend_normals(a.normal, b.normal, t),
        )
    };

    // Casos silla (dos esquinas diagonales mojadas): dos triángulos separados
    // en vez de un hexágono que se cortaría a sí mismo.
    if mask == 0b0101 {
        emit_poly(
            vertices,
            indices,
            &[corner_point(0), edge_point(0), edge_point(3)],
        );
        emit_poly(
            vertices,
            indices,
            &[edge_point(1), corner_point(2), edge_point(2)],
        );
        return;
    }
    if mask == 0b1010 {
        emit_poly(
            vertices,
            indices,
            &[edge_point(0), corner_point(1), edge_point(1)],
        );
        emit_poly(
            vertices,
            indices,
            &[edge_point(2), corner_point(3), edge_point(3)],
        );
        return;
    }

    // Recorrido 0→1→2→3: añade cada esquina dentro del círculo y el punto de
    // cruce por cero de las aristas que cambian de estado. Mantiene el devanado.
    let mut poly = Vec::with_capacity(6);
    for i in 0..4 {
        let j = (i + 1) % 4;
        if corners[i].inside {
            poly.push(corner_point(i));
        }
        if corners[i].inside != corners[j].inside {
            poly.push(edge_point(i));
        }
    }
    emit_poly(vertices, indices, &poly);
}

impl crate::engine3d::engine::State {
    /// Alta desde el ghost del viewport. Se snapped a la rejilla; el primer
    /// step ya consulta el suelo y recoloca la fuente si cae dentro del terreno.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn water_add_source(
        &mut self,
        id: u64,
        position: [f32; 3],
        cell_size: f32,
        color: [f32; 3],
        viscosity: f32,
        cohesion: f32,
    ) {
        self.water
            .add_liquid_source(id, position, cell_size, color, viscosity, cohesion);
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

    /// B5 — entrega al solver de partículas la nube de puntos que representa
    /// el terreno, para que sus paredes dejen de ser una caja genérica.
    #[allow(dead_code)]
    pub(crate) fn water_feed_boundary_to_solver(
        &mut self,
        solver: &mut crate::engine3d::particles::fluid::FluidSolver,
    ) {
        if self.water.terrain.dirty {
            let floors = std::mem::take(&mut self.water.floor_cache);
            let cell = self.water.cell_size();
            self.water.terrain.rebuild(&floors, cell, cell * 0.5);
            self.water.floor_cache = floors;
        }
        solver.set_terrain_boundary(self.water.terrain.points(), self.water.terrain.volumes());
    }

    /// Avance del autómata a 5 Hz, enganchado al update del motor.
    pub(crate) fn water_tick(&mut self, dt: f32) {
        let dt = dt.min(0.05);
        if !self.water.is_empty() {
            self.water.tick_accum += dt;
            if self.water.tick_accum < TICK_INTERVAL {
                // Entre ticks no se simula, pero sí se reinterpola la malla: la
                // celda nueva crece desde 0 mientras la anterior se vacía.
                if self.water.is_animating() {
                    let alpha = self.water.tick_accum / TICK_INTERVAL;
                    self.water_sync_mesh(alpha);
                }
            } else {
                self.water.tick_accum -= TICK_INTERVAL;
                self.water_step_once();
            }
        }
    }

    /// Consulta el suelo por raycast y ejecuta un tick (o regenera la malla si
    /// solo cambió la configuración). Público en el crate para la restauración
    /// de escenas desde `particles::restore_particles_from_value`.
    pub(crate) fn water_step_once(&mut self) {
        let cs = self.water.cell_size();
        // `step` solo lee los suelos: mover el mapa evita clonar cientos de
        // entradas en cada tick (5 Hz) solo por el borrow checker.
        let floors = std::mem::take(&mut self.water.floor_cache);
        self.water.step(&floors);
        self.water.floor_cache = floors;
        // La rejilla puede haber cambiado: consultar el suelo de las columnas
        // nuevas y soltar las que ya no están cerca del agua (si no, el caché
        // crecería sin límite al mover fuentes).
        let columns: Vec<_> = self.water.columns_to_query().iter().copied().collect();
        for (ix, iz) in columns {
            if self.water.floor_cache.contains_key(&(ix, iz)) {
                continue;
            }
            let x = (ix as f32 + 0.5) * cs;
            let z = (iz as f32 + 0.5) * cs;
            let origin = Vec3::new(x, RAY_TOP, z);
            let floor = self
                .physics
                .raycast_first_hit_distance(origin, Vec3::NEG_Y, RAY_MAX, &[])
                .map(|distance| RAY_TOP - distance)
                .unwrap_or(0.0);
            self.water.floor_cache.insert((ix, iz), floor);
        }
        self.water.prune_floor_cache();
        if self.water.terrain.dirty {
            // El muestreo del terreno es caro: solo se hace cuando cambia el
            // suelo, no en cada tick. Se mueve el mapa para no clonarlo.
            let floors = std::mem::take(&mut self.water.floor_cache);
            let cell = self.water.cell_size();
            self.water.terrain.rebuild(&floors, cell, cell * 0.5);
            self.water.floor_cache = floors;
        }
        // alpha=0: justo después del tick se dibuja el estado anterior, para
        // que la transición arranque desde donde estaba y no haya salto.
        self.water_sync_mesh(0.0);
        self.water.dirty = false;
        self.water.stats.terrain_points = self.water.terrain.points().len();
    }

    /// Sube la geometría resultante y la deja dibujándose como entidad estática.
    fn water_sync_mesh(&mut self, alpha: f32) {
        let (vertices, indices, min, max) = self.water.build_mesh(&self.water.floor_cache, alpha);
        self.water.stats.mesh_vertices = vertices.len();
        self.water.stats.mesh_indices = indices.len();
        let mesh_idx = if let Some(index) = self.water.mesh_idx {
            // Durante la animación esto ocurre cada frame: reutilizar los
            // buffers evita crear/destruir memoria Vulkan 60 veces por segundo.
            if !self.meshes[index].update_dynamic(&vertices, &indices) {
                self.meshes[index] = crate::engine3d::mesh::upload_dynamic(
                    &self.vulkan,
                    &vertices,
                    &indices,
                    "water",
                );
            }
            index
        } else {
            let index = self.meshes.len();
            self.meshes.push(crate::engine3d::mesh::upload_dynamic(
                &self.vulkan,
                &vertices,
                &indices,
                "water",
            ));
            self.water.mesh_idx = Some(index);
            index
        };
        // El color viene del líquido configurado con el tinte del preset `water`
        // de la demo de reflejos. La textura se cachea por RGB en el motor, así
        // que pedirla cada frame es solo una búsqueda en mapa.
        let rgba = liquid_surface_rgba(self.water.color);
        let tex_idx = self.ensure_plane_tool_colored_texture(rgba);
        self.water.tex_idx = Some(tex_idx);
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
                // Igual que el preset `water` de la demo de reflejos: espejo
                // liso, dieléctrico 1.33 y semitransparencia de 0.48.
                SurfacePbr {
                    roughness: 0.0,
                    metallic: 0.0,
                    ior: 1.33,
                    opacity: 0.48,
                },
            );
            self.water.entity = Some(entity);
        }
    }

    /// Las fuentes van en el mismo snapshot que los emisores (la UI las lista
    /// juntas) con material `liquid_cube` y su tamaño de celda.
    pub(crate) fn water_export_into(&self, emitters: &mut Vec<SaveParticleEmitterSnapshot>) {
        for source in self.water.sources() {
            emitters.push(SaveParticleEmitterSnapshot {
                id: source.id,
                material: LIQUID_WIRE.to_string(),
                position: source.position,
                cell_size: Some(source.cell_size),
                color: Some(source.color),
                viscosity: Some(source.viscosity),
                cohesion: source.cohesion,
                soft: true,
                ..Default::default()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_rgba_uses_the_configured_liquid_color() {
        let rgba = liquid_rgba(DEFAULT_LIQUID_COLOR, 0);
        assert_eq!(rgba, [20, 71, 107, 255]);
        // La superficie aplica además el tinte del preset `water` de la demo.
        assert_eq!(
            liquid_surface_rgba(DEFAULT_LIQUID_COLOR),
            [20, 77, 120, 255]
        );
        let rose = liquid_rgba([0.9, 0.2, 0.4], 0);
        assert_eq!(rose, [230, 51, 102, 255]);
        // El boost solo aclara las gotas; con 255 la superficie siempre salía blanca.
        assert_eq!(liquid_rgba(DEFAULT_LIQUID_COLOR, 255), [255, 255, 255, 255]);
    }

    fn flat_floor(cells: &WaterSystem) -> HashMap<(i32, i32), f32> {
        cells
            .columns_to_query()
            .iter()
            .copied()
            .map(|column| (column, 0.0))
            .collect()
    }

    #[test]
    fn source_spreads_and_a_finite_puddle_forms() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 0.25, 0.25], 0.5);
        for _ in 0..60 {
            let floors = flat_floor(&water);
            water.step(&floors);
        }
        // El cubo se reparte: el volumen total se conserva y la fuente se agota.
        let total: u32 = water.cells.values().map(|&level| level as u32 + 1).sum();
        assert_eq!(total, SOURCE_VOLUME, "el volumen debe conservarse: {total}");
        assert_eq!(water.reservoir[&1], 0, "la fuente debe agotarse");
        assert!(water.cell_count() > 20, "el charco debe extenderse");
        assert!(water.cell_count() < MAX_CELLS, "el charco debe ser finito");
        assert!(water.cells.values().all(|&level| level <= SOURCE_LEVEL));
        assert!(
            water
                .cells
                .get(&(0, 0, 0))
                .is_some_and(|&level| level < SOURCE_LEVEL),
            "la altura de la fuente debe bajar al repartirse"
        );
    }

    /// Entre ticks la malla interpola: la celda que cede agua baja y las
    /// nuevas crecen desde 0, en vez de aparecer completas de golpe.
    #[test]
    fn new_cells_grow_from_zero_between_ticks() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 0.25, 0.25], 0.5);
        let floors = flat_floor(&water);
        water.step(&floors); // cubo inicial, entero
        water.step(&floors); // las celdas vecinas salen de él
        assert!(water.is_animating());

        let (at_start, _, _, _) = water.build_mesh(&floors, 0.0);
        let (at_half, _, _, _) = water.build_mesh(&floors, 0.5);
        let (at_end, _, _, _) = water.build_mesh(&floors, 1.0);

        let highest = |vertices: &[Vertex]| {
            vertices
                .iter()
                .map(|v| v.position[1])
                .fold(f32::MIN, f32::max)
        };
        let start = highest(&at_start);
        let half = highest(&at_half);
        let end = highest(&at_end);
        assert!(
            start > half && half > end,
            "la altura debe bajar suavemente en la fuente: {start} > {half} > {end}"
        );
    }

    /// El cubo inicial aparece él solo; las celdas vecinas salen de él en el
    /// tick siguiente, no en el mismo en que se coloca la fuente.
    /// B1 + B2: el volumen sigue conservándose exactamente y se genera
    /// velocidad a partir del gradiente de altura, que se amortigua al asentarse.
    #[test]
    fn curvature_and_inertia_keep_conservation_and_velocity_bounded() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 0.25, 0.25], 0.5);
        let mut peak_speed = 0.0f32;
        for _ in 0..60 {
            let floors = flat_floor(&water);
            water.step(&floors);
            for &v in water.velocity.values() {
                peak_speed = peak_speed.max(v[0].hypot(v[1]));
            }
        }
        let total: u32 = water.cells.values().map(|&level| level as u32 + 1).sum();
        assert_eq!(total, SOURCE_VOLUME, "el volumen debe conservarse: {total}");
        assert!(
            peak_speed > 0.0,
            "debe generarse velocidad por el gradiente"
        );
        assert!(
            peak_speed <= SW_MAX_SPEED + 1.0e-5,
            "la velocidad debe estar acotada: {peak_speed}"
        );
        let settled_speed = water
            .velocity
            .values()
            .map(|&v| v[0].hypot(v[1]))
            .fold(0.0f32, f32::max);
        assert!(
            settled_speed < peak_speed,
            "la velocidad debe amortiguarse: {settled_speed} vs {peak_speed}"
        );
    }

    /// B5: el muestreador produce puntos sobre el heightfield con la altura
    /// interpolada y un espaciado regular, y se puede regenerar.
    /// B7: las gotas se generan, se simulan y se reciclan al caducar; la malla
    /// devuelve el placeholder cuando no queda ninguna.
    #[test]
    fn liquid_properties_are_applied_and_exported() {
        let mut water = WaterSystem::default();
        water.add_liquid_source(1, [0.25, 0.25, 0.25], 0.5, [0.9, 0.2, 0.4], 0.7, 0.25);
        assert_eq!(water.color, [0.9, 0.2, 0.4]);
        assert_eq!(water.viscosity, 0.7);
        assert_eq!(water.cohesion, 0.25);
        let source = &water.sources()[0];
        assert_eq!(source.color, [0.9, 0.2, 0.4]);
        assert_eq!(source.viscosity, 0.7);
        assert_eq!(source.cohesion, 0.25);
    }

    #[test]
    fn stats_track_simulation_state() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 0.25, 0.25], 0.5);
        for _ in 0..20 {
            let floors = flat_floor(&water);
            water.step(&floors);
        }
        let stats = water.stats();
        assert!(stats.steps >= 20);
        assert_eq!(stats.cells, water.cell_count());
    }

    #[test]
    fn terrain_boundary_samples_the_heightfield() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 1.25, 0.25], 0.5);
        let floors: HashMap<(i32, i32), f32> = flat_floor(&water)
            .into_keys()
            .map(|column| (column, 1.0))
            .collect();
        let cell = water.cell_size();
        water.terrain.rebuild(&floors, cell, cell * 0.5);

        assert!(water.terrain.points().len() > 4);
        assert_eq!(water.terrain.points().len(), water.terrain.volumes().len());
        assert!(
            water.terrain.points().iter().all(|point| {
                let column = (
                    (point[0] / cell - 0.5).floor() as i32,
                    (point[2] / cell - 0.5).floor() as i32,
                );
                // Fuera de la región consultada el suelo vale 0 por defecto.
                !floors.contains_key(&column) || (point[1] - 1.0).abs() < 1.0e-4
            }),
            "los puntos sobre columnas conocidas deben apoyarse en el terreno"
        );
        let first = water.terrain.points()[0];
        let second = water.terrain.points()[1];
        assert!(
            (second[2] - first[2]).abs() > 0.0,
            "debe haber espaciado en Z"
        );
        assert!(water.terrain.volumes().iter().all(|&volume| volume > 0.0));
    }

    #[test]
    fn floor_cache_discards_columns_away_from_the_water() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 0.25, 0.25], 0.5);
        water.floor_cache.insert((0, 0), 0.0);
        water.floor_cache.insert((50, 50), 0.0); // columna vieja de otra fuente
        water.prune_floor_cache();
        assert!(water.floor_cache.contains_key(&(0, 0)));
        assert!(!water.floor_cache.contains_key(&(50, 50)));
    }

    #[test]
    fn source_cell_precedes_its_neighbors() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 0.25, 0.25], 0.5);
        let floors = flat_floor(&water);

        water.step(&floors);
        assert_eq!(
            water.cells.len(),
            1,
            "el primer tick solo debe crear el cubo"
        );
        assert!(water.cells.contains_key(&(0, 0, 0)));
        assert!(
            !water.is_animating(),
            "el cubo inicial se muestra entero desde el primer frame"
        );

        water.step(&floors);
        assert!(
            water.cells.len() > 1,
            "las celdas vecinas deben salir en el tick siguiente"
        );
        assert!(water.is_animating());
    }

    #[test]
    fn water_height_flattens_as_it_spreads() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 0.25, 0.25], 0.5);
        assert_eq!(water.reservoir[&1], SOURCE_VOLUME, "el cubo nace lleno");
        for _ in 0..120 {
            let floors = flat_floor(&water);
            water.step(&floors);
        }
        let source_level = water.cells.get(&(0, 0, 0)).copied().unwrap();
        assert!(
            source_level <= 1,
            "la fuente debe perder altura al repartirse: {source_level}"
        );
        let max_level = water.cells.values().copied().max().unwrap();
        let average_units = water
            .cells
            .values()
            .map(|&level| level as u32 + 1)
            .sum::<u32>()
            / water.cell_count() as u32;
        assert!(
            max_level <= 4 && average_units <= 3,
            "todas las capas deben quedar cerca de la altura mínima: max {max_level}, media {average_units}"
        );
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
            .into_keys()
            .map(|column| (column, 1.0))
            .collect();
        water.step(&floors);
        let source_cell = water.sources[0].cell;
        assert!(
            source_cell.1 >= 2,
            "la fuente enterrada debe subir sobre el terreno: {source_cell:?}"
        );
        assert!(
            water.cells.contains_key(&source_cell),
            "la fuente debe verter agua en su celda: {source_cell:?}"
        );
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

    /// Itera los triángulos de la malla (tapa y fila final pueden tener
    /// polígonos de 3 a 6 vértices, no solo quads).
    fn triangles<'a>(
        vertices: &'a [Vertex],
        indices: &'a [u32],
    ) -> impl Iterator<Item = [&'a Vertex; 3]> + 'a {
        indices.chunks(3).map(move |tri| {
            [
                &vertices[tri[0] as usize],
                &vertices[tri[1] as usize],
                &vertices[tri[2] as usize],
            ]
        })
    }

    /// Toda la malla (tapa + fila final) sale de un único campo de alturas:
    /// un mismo punto de rejilla (incluidos los puntos medios del recorte) tiene
    /// una sola altura, compartida por las caras que lo tocan.
    #[test]
    fn surface_and_final_row_share_the_same_heightfield() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 0.25, 0.25], 0.5);
        for _ in 0..20 {
            let floors = flat_floor(&water);
            water.step(&floors);
        }
        let floors = flat_floor(&water);
        let (vertices, indices, _, _) = water.build_mesh(&floors, 1.0);
        let cs = water.cell_size();
        // Cuantización fina: el corte del círculo cae en posiciones arbitrarias
        // dentro de la celda, así que agrupar por media celda daría falsos
        // choques entre un vértice mojado y un punto de corte cercano.
        let key = |v: &Vertex| {
            (
                (v.position[0] / cs * 1000.0).round() as i32,
                (v.position[2] / cs * 1000.0).round() as i32,
            )
        };

        let mut heights: HashMap<(i32, i32), f32> = HashMap::new();
        let mut faces = 0;
        for tri in triangles(&vertices, &indices) {
            faces += 1;
            for vertex in tri {
                assert!(
                    vertex.normal[1] > 0.0,
                    "cara vertical en {:?}: normal {:?}",
                    key(vertex),
                    vertex.normal
                );
                match heights.get(&key(vertex)) {
                    Some(&previous) => assert!(
                        (previous - vertex.position[1]).abs() < 1.0e-5,
                        "altura distinta para el mismo vértice {:?}: {previous} vs {}",
                        key(vertex),
                        vertex.position[1]
                    ),
                    None => {
                        heights.insert(key(vertex), vertex.position[1]);
                    }
                }
            }
        }
        assert!(faces > 4, "el charco debe tener varias caras: {faces}");
    }

    /// El contorno es circular: los puntos de la orilla están a la misma
    /// distancia euclídea de la fuente en todos los sectores, sin huecos. El
    /// radio es una celda menor que el alcance del autómata (6 → 5). Con la
    /// propagación Manhattan pura el rombo daba ~4.2 en diagonales por 6 en
    /// los ejes; ahora todos los sectores rondan el mismo valor.
    #[test]
    fn border_follows_a_circular_contour() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 0.25, 0.25], 0.5);
        for _ in 0..20 {
            let floors = flat_floor(&water);
            water.step(&floors);
        }
        let floors = flat_floor(&water);
        let (vertices, _, _, _) = water.build_mesh(&floors, 1.0);
        let cs = water.cell_size();
        let source = &water.sources[0];
        let sx = (source.cell.0 as f32 + 0.5) * cs;
        let sz = (source.cell.2 as f32 + 0.5) * cs;

        const SECTORS: usize = 16;
        let mut sector_radius = [0.0f32; SECTORS];
        let mut contour_points = 0;
        for vertex in &vertices {
            if (vertex.position[1] - EDGE_LIFT).abs() > 1.0e-4 {
                continue;
            }
            contour_points += 1;
            let dx = (vertex.position[0] - sx) / cs;
            let dz = (vertex.position[2] - sz) / cs;
            let angle = dz.atan2(dx);
            let slice = 2.0 * std::f32::consts::PI / SECTORS as f32;
            let sector =
                (((angle + std::f32::consts::PI) / slice).floor() as usize).clamp(0, SECTORS - 1);
            sector_radius[sector] = sector_radius[sector].max(dx.hypot(dz));
        }
        assert!(
            contour_points > 16,
            "pocos puntos de contorno: {contour_points}"
        );
        for (sector, &radius) in sector_radius.iter().enumerate() {
            assert!(radius > 0.0, "hueco en el contorno, sector {sector}");
            assert!(
                (radius - 5.0).abs() < 5.0 * 0.12,
                "sector {sector}: radio {radius}, esperado ~5"
            );
        }
    }

    /// Dos fuentes cercanas se unen hacia AFUERA: en la bisectriz la orilla
    /// llega más lejos que con `min` puro. Con centros a 7 celdas y radio 5,
    /// la unión dura daría sqrt(5² − 3.5²) ≈ 3.57 celdas; el redondeo debe
    /// superar ese valor para borrar el pico cóncavo.
    #[test]
    fn nearby_sources_merge_with_an_outward_blend() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 0.25, 0.25], 0.5);
        water.add_source(2, [3.75, 0.25, 0.25], 0.5);
        for _ in 0..60 {
            let floors = flat_floor(&water);
            water.step(&floors);
        }
        let floors = flat_floor(&water);
        let (vertices, _, _, _) = water.build_mesh(&floors, 1.0);
        let cs = water.cell_size();
        let middle_x = 2.0f32;
        let middle_z = 0.25f32;

        let mut widest_at_seam = 0.0f32;
        let mut points_at_seam = 0;
        for vertex in &vertices {
            if (vertex.position[1] - EDGE_LIFT).abs() > 1.0e-4 {
                continue;
            }
            if (vertex.position[0] - middle_x).abs() > cs * 0.5 {
                continue;
            }
            points_at_seam += 1;
            widest_at_seam = widest_at_seam.max((vertex.position[2] - middle_z).abs() / cs);
        }
        assert!(points_at_seam > 0, "no hay orilla en la unión");
        assert!(
            widest_at_seam > 3.9,
            "la unión quedó cóncava: {widest_at_seam} celdas (min puro daría ~3.57)"
        );
    }

    /// El charco ya no termina en una línea vertical: la última fila de celdas
    /// secas baja en rampa desde la superficie hasta apoyarse justo por encima
    /// del suelo, con margen para no hacer z-fighting con el terreno.
    #[test]
    fn puddle_ends_with_a_sloped_row_instead_of_a_vertical_wall() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 0.25, 0.25], 0.5);
        for _ in 0..20 {
            let floors = flat_floor(&water);
            water.step(&floors);
        }
        let floors = flat_floor(&water);
        let (vertices, indices, _, _) = water.build_mesh(&floors, 1.0);
        assert_last_row_reaches(
            &water,
            &vertices,
            &indices,
            0.0 + EDGE_LIFT,
            "el charco debe cerrarse con una fila en rampa",
        );
    }

    /// El margen anti-z-fighting se mide sobre el suelo del raycast, no sobre un
    /// suelo imaginario en y=0: con el terreno a otra altura la fila final debe
    /// seguir quedando justo por encima de él.
    #[test]
    fn final_row_clears_a_nonzero_terrain_floor() {
        let mut water = WaterSystem::default();
        water.add_source(1, [0.25, 1.25, 0.25], 0.5);
        let mut floors: HashMap<(i32, i32), f32> = flat_floor(&water)
            .into_keys()
            .map(|column| (column, 1.0))
            .collect();
        for _ in 0..20 {
            water.step(&floors);
            floors = flat_floor(&water)
                .into_keys()
                .map(|column| (column, 1.0))
                .collect();
        }
        let (vertices, indices, _, _) = water.build_mesh(&floors, 1.0);
        assert_last_row_reaches(
            &water,
            &vertices,
            &indices,
            1.0 + EDGE_LIFT,
            "la fila final se superpone con el terreno",
        );
    }

    /// Comprueba que la malla tiene una orilla apoyada en `ground_y` (suelo +
    /// margen anti-z-fighting) y que desde ahí sube en rampa hasta el agua.
    fn assert_last_row_reaches(
        _water: &WaterSystem,
        vertices: &[Vertex],
        _indices: &[u32],
        ground_y: f32,
        message: &str,
    ) {
        let lowest = vertices
            .iter()
            .map(|v| v.position[1])
            .fold(f32::MAX, f32::min);
        let highest = vertices
            .iter()
            .map(|v| v.position[1])
            .fold(f32::MIN, f32::max);
        assert!(
            (lowest - ground_y).abs() < 1.0e-4,
            "{message}: borde en {lowest} (esperado {ground_y})"
        );
        assert!(
            highest > lowest + 1.0e-3,
            "{message}: no hay rampa (de {lowest} a {highest})"
        );
    }
}
