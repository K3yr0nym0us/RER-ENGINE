//! Solver de fluidos en CPU: PBF/XPBD con tensión superficial y cohesión.
//!
//! Lo que aquí se simula es SOLO el fluido: sin empuje, sin acoplamiento
//! bidireccional con actores — partículas de agua que caen, salpican y QUEDAN
//! contenidas dentro de una caja contenedora.
//!
//! Implementación derivada del núcleo físico de un solver de fluidos MIT de
//! terceros. Créditos, licencia y bibliografía en `THIRD_PARTY_NOTICES.md` en la
//! raíz del repositorio.
//!
//! Pipeline por substep:
//! 1. **Predict**: `v += (gravedad + cohesión) * dt`, viscosidad XSPH,
//!    `x* = x + v * dt`.
//! 2. **DensityPressure** (N iteraciones): densidad Poly6 (fluidos + frontera),
//!    constraint de DESIGUALDAD `C = ρ/ρ0 − 1` con λ XPBD (solo-compresión: la
//!    incompresibilidad es rígida, la atracción NO — ver `solve_constraints`),
//!    corrección `Δp = (1/ρ0)·Σ(λi+λj+s_corr)·∇W` con kernel Spiky acumulada
//!    por substep con tope 0.2·h, y clamp contra las paredes cada iteración.
//! 3. **PostStep**: `v = (x* − x)/dt` (PBF puro), clamp de velocidad, damping.
//!
//! La búsqueda de vecinos usa hash espacial 3D con cache por substep (la primera
//! iteración construye la lista; las demás la reutilizan con chequeo de distancia).
//!
//! Sin Vulkan ni dependencias: lógica pura y determinista. Vive dentro de una
//! caja contenedora cerrada; no tiene acoplamiento con actores ni con el
//! generador de mundos. El agua jugable del editor es el autómata celular de
//! `engine3d::water` (celdas que comparten un volumen conservado).
//!
//! Todavía no se llama desde el tick de partículas; `dead_code` / clippy se
//! retiran al enganchar `FluidSolver` a `ParticleSystem`.

#![allow(dead_code)]
#![allow(clippy::needless_range_loop)]

use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};

/// Margen anti-atascamiento en los kernels (evita división por cero en r→0).
const SMALL: f32 = 1e-4;

/// Caja contenedora alineada a ejes: el fluido queda DENTRO de estas paredes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FluidBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl FluidBounds {
    pub(crate) fn new(min: [f32; 3], max: [f32; 3]) -> Self {
        Self { min, max }
    }

    /// Empuja un punto de vuelta dentro de la caja (paredes estáticas con margen).
    /// No rebota: la velocidad la reconstruye el post-step desde Δx, así que la
    /// pared funciona como el `collide` del solver — desliza, no atraviesa.
    fn clamp_inside(&self, p: &mut [f32; 3], margin: f32) {
        for axis in 0..3 {
            let lo = self.min[axis] + margin;
            let hi = self.max[axis] - margin;
            if hi > lo {
                p[axis] = p[axis].clamp(lo, hi);
            }
        }
    }

    fn contains(&self, p: [f32; 3], margin: f32) -> bool {
        (0..3).all(|axis| p[axis] >= self.min[axis] + margin && p[axis] <= self.max[axis] - margin)
    }
}

/// Configuración física del fluido (valores por defecto ≈ agua).
#[derive(Debug, Clone, Copy)]
pub(crate) struct FluidConfig {
    /// Densidad de reposo (kg/m³) — el constraint PBF busca ρ = ρ0.
    pub rest_density: f32,
    /// Radio de suavizado h (m): alcance de todos los kernels SPH.
    pub smoothing_radius: f32,
    /// Masa por partícula (kg).
    pub particle_mass: f32,
    /// Máximo de iteraciones del solver por substep (3–4 es un valor típico).
    pub solver_iterations: u32,
    /// Iteraciones mínimas antes de permitir la salida anticipada (B3).
    pub min_solver_iterations: u32,
    /// Error medio de densidad aceptable para salir antes (B3). Umbral
    /// conservador del 3%, acorde con el constraint de solo-compresión.
    pub max_density_error: f32,
    /// Compliance XPBD (m·s²/kg): 0 = PBF clásico estrictamente incompresible.
    pub compliance: f32,
    /// Gravedad +Y (m/s²); caída = −9.8.
    pub gravity: f32,
    /// Amortiguación global por substep (1 = sin pérdida; 0.995 típico).
    pub damping: f32,
    /// Velocidad máxima (m/s) — clamp anti-explosión del PostStep.
    pub max_velocity: f32,
    /// Cohesión superficial (0–1): fuerza de atracción entre partículas cercanas.
    /// 0.35: con menos, el charco dilata libremente —
    /// el gate solo-compresión no se opone a la expansión — y se esparce como
    /// material granular sin volver al espaciado de reposo.
    pub cohesion: f32,
    /// Aceleración máxima de cohesión (m/s²) — clamp de estabilidad.
    pub max_cohesion_accel: f32,
    /// Viscosidad XSPH [0..1]: suavizado de velocidad entre vecinos.
    pub viscosity: f32,
    /// Presión artificial s_corr (anti-agrupamiento, PBF Eq. 13). 0 = desactivada.
    /// Escala relativa a λ: en el paper k es pequeño (el término compite con
    /// (λi+λj), no los domina). Con el gate solo-compresión en dilatación opera
    /// SOLO: k=0.15 empujaba cada par ~3.6 cm/iteración alejándose — la
    /// superficie (media cáscara, sin cancelación vectorial) explotaba hacia
    /// fuera y el charco dilataba como gas. 0.03 lo deja como repulsión de
    /// corta distancia sin dominar la dinámica.
    pub tensile_k: f32,
    /// Exponente n de s_corr (1–8, típicamente 4).
    pub tensile_n: u32,
    /// Δq de s_corr: fracción de h donde se mide el kernel de referencia (0.3 típico).
    pub tensile_delta_q: f32,
    /// Soporte de paredes (partículas de frontera virtuales): 0 = desactivado,
    /// 1 = peso completo. Las paredes reconstruyen la media cáscara de vecinos
    /// que quitan (sin ellas, el agua contra suelo/paredes mide ~0.4·ρ0 aunque
    /// esté perfectamente apoyada).
    pub boundary_support: f32,
    /// B6 — curvatura de superficie: suaviza el contorno usando la diferencia
    /// de normales entre partículas vecinas. 0 = desactivada.
    pub curvature: f32,
    /// B6 — adhesión a las partículas de frontera. 0 = desactivada.
    pub boundary_adhesion: f32,
}

impl Default for FluidConfig {
    fn default() -> Self {
        Self {
            rest_density: 1000.0,
            smoothing_radius: 0.3,
            particle_mass: 4.0,
            solver_iterations: 4,
            min_solver_iterations: 2,
            max_density_error: 0.03,
            compliance: 0.0,
            gravity: -9.8,
            damping: 0.995,
            max_velocity: 20.0,
            cohesion: 0.35,
            max_cohesion_accel: 6.0,
            viscosity: 0.08,
            tensile_k: 0.03,
            tensile_n: 4,
            tensile_delta_q: 0.3,
            boundary_support: 1.0,
            curvature: 0.05,
            boundary_adhesion: 0.05,
        }
    }
}

/// Solver PBF/XPBD: agua contenida dentro de una caja de cristal.
pub(crate) struct FluidSolver {
    pub config: FluidConfig,
    pub bounds: FluidBounds,
    /// Posiciones reales (post post-step del substep anterior).
    positions: Vec<[f32; 3]>,
    velocities: Vec<[f32; 3]>,
    /// Posiciones predichas (`x*`): las que corrige el solver de constraints.
    predicted: Vec<[f32; 3]>,
    densities: Vec<f32>,
    lambdas: Vec<f32>,
    /// Partículas de frontera: muestreo estático fuera de cada
    /// pared. Se generan perezosamente en el primer `step` (necesitan la ρ0
    /// final, que `spawn_block` calibra al espaciado real).
    boundary: Vec<[f32; 3]>,
    /// Masa efectiva ψ_b de cada virtual: calibrada para que la suma de
    /// kernels sobre la nube virtual completa dé ρ0.
    boundary_psi: Vec<f32>,
    boundary_grid: Option<SpatialGrid>,
    boundary_built: bool,
    /// B3/B9 — iteraciones realmente usadas en el último substep y error medio
    /// de densidad con el que se salió (perfilado y early-out).
    pub last_iterations: u32,
    pub last_density_error: f32,
    /// B6 — normales de superficie por partícula, derivadas del gradiente del
    /// kernel de sus vecinos.
    normals: Vec<[f32; 3]>,
    /// B5 — puntos del terreno que sustituyen a la caja como frontera cuando
    /// se proporcionan. Cada volumen sirve para calibrar la masa efectiva.
    terrain_boundary: Option<(Vec<[f32; 3]>, Vec<f32>)>,
}

// ---------------------------------------------------------------------------
// Kernels SPH
// ---------------------------------------------------------------------------

/// Kernel Poly6 (para densidad): `W(r) = 315/(64πh⁹)·(h²−r²)³`. Cero más allá de h.
fn poly6(r2: f32, h: f32) -> f32 {
    let diff = h * h - r2;
    if diff <= 0.0 {
        return 0.0;
    }
    let coeff = 315.0 / (64.0 * std::f32::consts::PI * h.powi(9));
    coeff * diff * diff * diff
}

/// Gradiente del kernel Spiky (para presión). Es el ∇W real respecto a `p_i`:
/// `∇W = W'(r)·r̂ = −45/(πh⁶)·(h−r)²·r̂_{i→j}` — apunta HACIA el vecino (W decrece
/// con la distancia). Con λ<0 (partícula comprimida) esto produce Δp que la separa,
/// que es la física de PBF; invertir el signo colapsa el fluido sobre sí mismo.
/// `dir_i_minus_j` debe venir normalizado (dirección de i hacia... i−j: desde el
/// vecino hacia la partícula).
fn spiky_grad(dir_i_minus_j: [f32; 3], rlen: f32, h: f32) -> [f32; 3] {
    if rlen <= SMALL || rlen >= h {
        return [0.0; 3];
    }
    let coeff = 45.0 / (std::f32::consts::PI * h.powi(6));
    let d = h - rlen;
    let scale = coeff * d * d / rlen;
    // Signo negativo: ∇W apunta hacia el vecino (−r̂_{i→j}).
    [
        -dir_i_minus_j[0] * scale,
        -dir_i_minus_j[1] * scale,
        -dir_i_minus_j[2] * scale,
    ]
}

/// Presión artificial s_corr (PBF Eq. 13-14, versión branchless):
/// `s_corr = −k·(W(r)/W(Δq·h))ⁿ`. El coeficiente del kernel cancela en el ratio,
/// así que ambos lados usan el numerador `(h²−r²)³` sin normalizar.
fn scorr(r2: f32, h: f32, k: f32, n: u32, delta_q: f32) -> f32 {
    let h2 = h * h;
    let diff = h2 - r2;
    if diff <= 0.0 {
        return 0.0;
    }
    let dq2 = h2 * delta_q * delta_q;
    let w_num = diff * diff * diff;
    let w_dq = (h2 - dq2).max(1e-9).powi(3);
    -k * (w_num / w_dq).powi(n as i32)
}

/// Spline de cohesión `C(r)`: atracción a r≈h/2, repulsión de cerca.
/// Positivo = atrae; evita clustering sin colapsar el fluido a un punto.
fn cohesion_spline(r: f32, h: f32) -> f32 {
    if r > h || r <= SMALL {
        return 0.0;
    }
    let coeff = 32.0 / (std::f32::consts::PI * h.powi(9));
    let diff3 = (h - r).powi(3);
    let r3 = r * r * r;
    if r > 0.5 * h {
        coeff * diff3 * r3
    } else {
        coeff * (2.0 * diff3 * r3 - h.powi(6) / 64.0)
    }
}

/// Curva de adhesión a la frontera: solo actúa en la media cáscara
/// `r ∈ (h/2, h]`; dentro de h/2 la repulsión de corta distancia evita que la
/// partícula se pegue al extremo de la pared.
fn surface_adhesion(r: f32, h: f32) -> f32 {
    if r <= 0.5 * h || r > h {
        return 0.0;
    }
    let raw = (-4.0 * r * r / h + 6.0 * r - 2.0 * h).max(0.0);
    0.007 / h.powf(3.25) * raw.powf(0.25)
}

/// B6 — normales de superficie por partícula, derivadas del gradiente del
/// kernel de sus vecinos y ponderadas por la densidad de cada uno.
fn compute_surface_normals(
    normals: &mut Vec<[f32; 3]>,
    neighbors: &[Vec<Neighbor>],
    densities: &[f32],
    h: f32,
    mass: f32,
) {
    normals.clear();
    normals.resize(neighbors.len(), [0.0; 3]);
    for (i, normal) in normals.iter_mut().enumerate() {
        let mut sum = [0.0f32; 3];
        for neighbor in &neighbors[i] {
            let j = neighbor.index as usize;
            let grad = spiky_grad(neighbor.dir, neighbor.rlen, h);
            let weight = mass / densities[j].max(SMALL) * h;
            sum[0] += grad[0] * weight;
            sum[1] += grad[1] * weight;
            sum[2] += grad[2] * weight;
        }
        let len2 = dot(sum, sum);
        *normal = if len2 > SMALL {
            let inv = 1.0 / len2.sqrt();
            [sum[0] * inv, sum[1] * inv, sum[2] * inv]
        } else {
            [0.0; 3]
        };
    }
}

// ---------------------------------------------------------------------------
// Hash espacial 3D + ordenación por celda
// ---------------------------------------------------------------------------

/// B4 — hasher de búsqueda con semilla fija. El `DefaultHasher` de `std` se
/// siembra al azar por proceso, así que el orden de vecinos cambiaba entre
/// ejecuciones y con él el resultado en coma flotante.
struct FnvHasher(u64);

impl Hasher for FnvHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= *byte as u64;
            self.0 = self.0.wrapping_mul(0x1000_0000_01b3);
        }
    }
}

#[derive(Default)]
struct FnvBuildHasher;

impl BuildHasher for FnvBuildHasher {
    type Hasher = FnvHasher;

    fn build_hasher(&self) -> Self::Hasher {
        FnvHasher(0xcbf2_9ce4_8422_2325 ^ 1820)
    }
}

/// Grid de celdas de tamaño h: celda → índices de partículas. Se reconstruye por
/// substep y la lista de vecinos se cachea a partir de él (las iteraciones del
/// solver reutilizan la cache con chequeo de distancia, como en el shader GPU).
struct SpatialGrid {
    cell_size: f32,
    cells: HashMap<(i32, i32, i32), Vec<u32>, FnvBuildHasher>,
}

/// B4 — datos del vecino calculados una vez al construir la lista y
/// reutilizados por todas las fases que lean posiciones reales. `dir` es la
/// dirección normalizada desde la partícula hacia el vecino (i − j).
#[derive(Clone, Copy)]
struct Neighbor {
    index: u32,
    dir: [f32; 3],
    rlen: f32,
    w: f32,
}

impl SpatialGrid {
    fn build(positions: &[[f32; 3]], cell_size: f32) -> Self {
        let mut cells: HashMap<(i32, i32, i32), Vec<u32>, FnvBuildHasher> = HashMap::default();
        for (i, p) in positions.iter().enumerate() {
            let key = (
                (p[0] / cell_size).floor() as i32,
                (p[1] / cell_size).floor() as i32,
                (p[2] / cell_size).floor() as i32,
            );
            cells.entry(key).or_default().push(i as u32);
        }
        Self { cell_size, cells }
    }

    /// Vecinos dentro de h de `p` (busca las 27 celdas alrededor). `source` es
    /// el slice con el que se construyó el grid (el índice devuelto indexa en
    /// él); `skip` es un índice de `source` a excluir (la propia partícula,
    /// `usize::MAX` cuando se consulta la nube de frontera). Caché con tope:
    /// el shader usa `MAX_NEIGHBORS_PER_PARTICLE = 64`; aquí 48 basta en CPU.
    fn neighbors(
        &self,
        source: &[[f32; 3]],
        skip: usize,
        p: [f32; 3],
        h: f32,
        cap: usize,
    ) -> Vec<u32> {
        let base = (
            (p[0] / self.cell_size).floor() as i32,
            (p[1] / self.cell_size).floor() as i32,
            (p[2] / self.cell_size).floor() as i32,
        );
        let h2 = h * h;
        let mut out = Vec::new();
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let Some(cell) = self.cells.get(&(base.0 + dx, base.1 + dy, base.2 + dz))
                    else {
                        continue;
                    };
                    for &j in cell {
                        let j = j as usize;
                        if j == skip || out.len() >= cap {
                            continue;
                        }
                        let d = sub(source[j], p);
                        if dot(d, d) < h2 {
                            out.push(j as u32);
                        }
                    }
                }
            }
        }
        out
    }

    /// Igual que `neighbors`, pero guarda también distancia, dirección y peso
    /// del kernel. Convierte cada búsqueda en un único `sqrt`/`pow`.
    fn neighbors_detailed(
        &self,
        source: &[[f32; 3]],
        skip: usize,
        p: [f32; 3],
        h: f32,
        cap: usize,
    ) -> Vec<Neighbor> {
        let base = (
            (p[0] / self.cell_size).floor() as i32,
            (p[1] / self.cell_size).floor() as i32,
            (p[2] / self.cell_size).floor() as i32,
        );
        let h2 = h * h;
        let mut out: Vec<Neighbor> = Vec::new();
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let Some(cell) = self.cells.get(&(base.0 + dx, base.1 + dy, base.2 + dz))
                    else {
                        continue;
                    };
                    for &j in cell {
                        let j = j as usize;
                        if j == skip || out.len() >= cap {
                            continue;
                        }
                        let r = sub(p, source[j]);
                        let r2 = dot(r, r);
                        if r2 >= h2 || r2 <= SMALL {
                            continue;
                        }
                        let rlen = r2.sqrt();
                        let scale = 1.0 / rlen;
                        out.push(Neighbor {
                            index: j as u32,
                            dir: [r[0] * scale, r[1] * scale, r[2] * scale],
                            rlen,
                            w: poly6(r2, h),
                        });
                    }
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Utilidades vec3 sobre [f32; 3] (sin dependencias de glam/nalgebra)
// ---------------------------------------------------------------------------

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn normalize(a: [f32; 3]) -> [f32; 3] {
    let len = length(a).max(SMALL);
    scale(a, 1.0 / len)
}

/// Densidad de reposo de un lattice cúbico infinito con `spacing`: suma de
/// `m·W(r)` sobre TODAS las celdas vecinas dentro de h (incluye la auto-
/// contribución). Es el valor contra el que el constraint PBF debe competir.
fn lattice_density(spacing: f32, h: f32, mass: f32) -> f32 {
    let steps = (h / spacing).ceil() as i32;
    let mut density = 0.0;
    for iz in -steps..=steps {
        for iy in -steps..=steps {
            for ix in -steps..=steps {
                let r2 = (ix * ix + iy * iy + iz * iz) as f32 * spacing * spacing;
                if r2 <= h * h {
                    density += mass * poly6(r2, h);
                }
            }
        }
    }
    density
}

impl FluidSolver {
    /// Solver vacío dentro de `bounds` con `config`.
    pub(crate) fn new(config: FluidConfig, bounds: FluidBounds) -> Self {
        Self {
            config,
            bounds,
            positions: Vec::new(),
            velocities: Vec::new(),
            predicted: Vec::new(),
            densities: Vec::new(),
            lambdas: Vec::new(),
            boundary: Vec::new(),
            boundary_psi: Vec::new(),
            boundary_grid: None,
            boundary_built: false,
            last_iterations: 0,
            last_density_error: 0.0,
            normals: Vec::new(),
            terrain_boundary: None,
        }
    }

    pub(crate) fn particle_count(&self) -> usize {
        self.positions.len()
    }

    pub(crate) fn positions(&self) -> &[[f32; 3]] {
        &self.positions
    }

    /// Empuja un punto de vuelta a la caja (paredes estáticas con margen).
    /// No rebota: la velocidad la reconstruye el post-step desde Δx, así que la
    /// pared funciona como el `collide` del solver — desliza, no atraviesa.
    fn clamp_position(&self, p: &mut [f32; 3], margin: f32) {
        self.bounds.clamp_inside(p, margin);
    }

    /// ¿Está el punto dentro de la caja de cristal?
    fn position_inside(&self, p: [f32; 3], margin: f32) -> bool {
        self.bounds.contains(p, margin)
    }

    /// Spawnea un BLOQUE de agua (lattice regular) centrado en `center` con
    /// semi-extensión `half`, espaciado `spacing`. Las partículas que caen fuera
    /// de la caja se recortan: el bloque solo puede existir DENTRO de la caja.
    /// Devuelve cuántas partículas se spawnearon.
    ///
    /// También auto-calibra `rest_density` a la densidad real del lattice: el
    /// constraint PBF con una ρ0 imposible desestabiliza el fluido.
    pub(crate) fn spawn_block(&mut self, center: [f32; 3], half: [f32; 3], spacing: f32) -> usize {
        let margin = self.config.smoothing_radius * 0.25;
        // CEIL, no floor: el bloque debe cubrir TODO el volumen pedido (el
        // exceso lo recorta el dominio de fluido). Con floor(), un span no
        // múltiplo del espaciado dejaba el relleno de la caja cuantizado muy por
        // debajo del objetivo (128 de ~300 partículas).
        let n = [
            ((half[0] * 2.0) / spacing).ceil().max(1.0) as usize,
            ((half[1] * 2.0) / spacing).ceil().max(1.0) as usize,
            ((half[2] * 2.0) / spacing).ceil().max(1.0) as usize,
        ];
        let mut spawned = 0;
        for iz in 0..n[2] {
            for iy in 0..n[1] {
                for ix in 0..n[0] {
                    let p = [
                        center[0] - half[0] + (ix as f32 + 0.5) * spacing,
                        center[1] - half[1] + (iy as f32 + 0.5) * spacing,
                        center[2] - half[2] + (iz as f32 + 0.5) * spacing,
                    ];
                    // Nace fuera de la caja → descartado: el bloque solo puede
                    // existir DENTRO del dominio de fluido (mismo margen que el
                    // clamp del solver).
                    if !self.position_inside(p, margin) {
                        continue;
                    }
                    self.positions.push(p);
                    self.velocities.push([0.0; 3]);
                    self.predicted.push(p);
                    self.densities.push(self.config.rest_density);
                    self.lambdas.push(0.0);
                    spawned += 1;
                }
            }
        }
        if spawned > 0 {
            // ρ0 analítico del LATTICE INFINITO con este espaciado: la densidad de
            // reposo correcta es la de un punto interior (todas las capas de
            // vecinos completas). Promediar el bloque entero contamina el valor
            // con partículas de superficie (les faltan vecinos) y deja ρ0 por
            // debajo del interior: el constraint C > 0 permanente sobre-comprime
            // todo el fluido.
            self.config.rest_density = lattice_density(
                spacing,
                self.config.smoothing_radius,
                self.config.particle_mass,
            );
        }
        spawned
    }

    /// B8 — substeps adaptativos por velocidad: se divide el intervalo para
    /// que ninguna partícula avance más que una fracción de su radio de
    /// suavizado (acotado a 1..10). Con agua rápida se subdivide; en reposo
    /// cuesta un solo substep, que es el caso habitual.
    fn cfl_substeps(&self, dt: f32) -> u32 {
        let h = self.config.smoothing_radius;
        let speed = self
            .velocities
            .iter()
            .map(|&v| length(v))
            .fold(0.0f32, f32::max);
        let cfl = 0.8 * h / speed.max(SMALL);
        ((dt / cfl).ceil() as u32).clamp(1, 10)
    }

    /// Un frame de fluido: predict → solver de constraints → post-step.
    /// `dt` se acota a 0.05 s como el `MAX_DT` del sistema de partículas.
    pub(crate) fn step(&mut self, dt: f32) {
        let dt = dt.clamp(1e-4, 0.05);
        if self.positions.is_empty() {
            return;
        }
        self.ensure_boundary();
        let substeps = self.cfl_substeps(dt);
        let sub_dt = dt / substeps as f32;
        for _ in 0..substeps {
            self.step_substep(sub_dt);
        }
    }

    /// Un substep del solver. Extraído de `step` para poder repetirlo con CFL.
    fn step_substep(&mut self, dt: f32) {
        // Cache de vecinos del SUBSTEP (no por iteración): primera búsqueda con
        // hash espacial, el resto de iteraciones reutiliza (chequeo de distancia
        // incluido porque las posiciones corrigen).
        let grid = SpatialGrid::build(&self.positions, self.config.smoothing_radius);
        let neighbors: Vec<Vec<Neighbor>> = (0..self.positions.len())
            .map(|i| {
                grid.neighbors_detailed(
                    &self.positions,
                    i,
                    self.positions[i],
                    self.config.smoothing_radius,
                    48,
                )
            })
            .collect();
        // Vecinos VIRTUALES por partícula: la nube de frontera es estática, pero
        // el fluido se mueve, así que la lista se refresca cada frame.
        let empty: Vec<u32> = Vec::new();
        let b_neighbors: Vec<Vec<u32>> = match &self.boundary_grid {
            Some(grid) => (0..self.positions.len())
                .map(|i| {
                    grid.neighbors(
                        &self.boundary,
                        usize::MAX,
                        self.positions[i],
                        self.config.smoothing_radius,
                        64,
                    )
                })
                .collect(),
            None => (0..self.positions.len()).map(|_| empty.clone()).collect(),
        };

        compute_surface_normals(
            &mut self.normals,
            &neighbors,
            &self.densities,
            self.config.smoothing_radius,
            self.config.particle_mass,
        );
        self.predict(dt, &neighbors, &b_neighbors);
        // Δp acumulado del FRAME: el guard anti-explosión es el desplazamiento
        // TOTAL del constraint por frame (0.2·h), no el de cada iteración.
        // Clampar por iteración permite 4×0.2·h = 0.8·h por substep ≈ 14 m/s de
        // velocidad inyectada: el λ de atracción (partícula dilatada con pocos
        // vecinos → Σ|∇C|² diminuto → λ enorme) rebotaba el fluido sin fin.
        // La corrección del constraint se acumula SOBRE la predicción cinemática
        // del predict (gravedad/cohesión/viscosidad): reconstruir `predicted`
        // solo desde la posición de origen borraba el desplazamiento cinemático
        // y el bloque se quedaba congelado flotando en el aire.
        let kinematic: Vec<[f32; 3]> = self.predicted.clone();
        let mut dp_accum = vec![[0.0f32; 3]; self.positions.len()];
        let max_iter = self.config.solver_iterations;
        let min_iter = self.config.min_solver_iterations.min(max_iter);
        let rho0 = self.config.rest_density.max(SMALL);
        let count = self.positions.len().max(1) as f32;
        let mut used = 0u32;
        let mut average_error = 0.0f32;
        for iteration in 0..max_iter {
            self.solve_constraints(dt, &neighbors, &b_neighbors, &kinematic, &mut dp_accum);
            used = iteration + 1;
            // El constraint es SOLO compresión (C = ρ/ρ0 − 1, C>0). El error
            // relevante es cuánto se excede ρ0: una lámina asentada mide menos
            // de ρ0 legítimamente y no debe forzar iteraciones extra.
            average_error = self
                .densities
                .iter()
                .map(|&rho| (rho / rho0 - 1.0).max(0.0))
                .sum::<f32>()
                / count;
            // B3: salida anticipada por convergencia. Solo cuando llevamos las iteraciones
            // mínimas y el error medio ya es aceptable.
            if used >= min_iter && average_error <= self.config.max_density_error {
                break;
            }
        }
        self.last_iterations = used;
        self.last_density_error = average_error;
        self.post_step(dt);
    }

    /// Añade una partícula suelta con estado inicial completo.
    pub(crate) fn spawn_particle(&mut self, position: [f32; 3], velocity: [f32; 3]) -> usize {
        let index = self.positions.len();
        self.positions.push(position);
        self.velocities.push(velocity);
        self.predicted.push(position);
        self.densities.push(self.config.rest_density);
        self.lambdas.push(0.0);
        self.normals.push([0.0; 3]);
        index
    }

    /// Elimina una partícula por índice (intercambio con la última).
    pub(crate) fn remove_particle(&mut self, index: usize) {
        if index >= self.positions.len() {
            return;
        }
        self.positions.swap_remove(index);
        self.velocities.swap_remove(index);
        self.predicted.swap_remove(index);
        self.densities.swap_remove(index);
        self.lambdas.swap_remove(index);
        if index < self.normals.len() {
            self.normals.swap_remove(index);
        }
    }

    pub(crate) fn velocities_mut(&mut self) -> &mut [[f32; 3]] {
        &mut self.velocities
    }

    pub(crate) fn clear_particles(&mut self) {
        self.positions.clear();
        self.velocities.clear();
        self.predicted.clear();
        self.densities.clear();
        self.lambdas.clear();
        self.normals.clear();
    }

    /// B5 — establece la nube de puntos del terreno como frontera. Se copian
    /// los datos porque el muestreador puede regenerarse mientras el solver
    /// sigue vivo; la nube se reconstruye en el próximo `step`.
    pub(crate) fn set_terrain_boundary(&mut self, points: &[[f32; 3]], volumes: &[f32]) {
        self.terrain_boundary = Some((points.to_vec(), volumes.to_vec()));
        self.boundary_built = false;
    }

    /// Genera (una sola vez) la nube de partículas de frontera virtuales.
    /// La ψ es UNA para todas: ρ0 dividido por la suma de kernels de la lattice
    /// infinita de paso sb (con sb = h/2 = paso de fluido típico da ψ = masa de
    /// partícula, exactamente la media cáscara que el suelo le quita al fluido).
    /// UNA lattice global de paso h/2 sobre la caja expandida h, excluyendo el
    /// dominio de fluido. Generar por pared con barridos solapados sobre-cuenta
    /// las esquinas (soporte ~3× → ρ 1561 > ρ0 → presión espuria que rociaba el
    /// charco) y las ψ calibradas con el vecindario local salían sobre-ponderadas
    /// en el borde de la nube.
    fn ensure_boundary(&mut self) {
        if self.boundary_built || self.config.boundary_support <= 0.0 {
            self.boundary_built = true;
            return;
        }
        self.boundary_built = true;
        // B5 — si hay terreno muestreado, esas partículas son la frontera.
        // La masa efectiva se calibra con el espaciado real de la nube.
        if let Some((points, volumes)) = self.terrain_boundary.clone() {
            let h = self.config.smoothing_radius;
            let rho0 = self.config.rest_density.max(SMALL);
            let spacing = volumes
                .first()
                .map(|volume| volume.cbrt())
                .unwrap_or(h * 0.5);
            let psi = rho0 / lattice_density(spacing, h, 1.0).max(SMALL);
            let grid = SpatialGrid::build(&points, h);
            self.boundary = points;
            self.boundary_psi = vec![psi; self.boundary.len()];
            self.boundary_grid = Some(grid);
            return;
        }
        let h = self.config.smoothing_radius;
        let sb = h * 0.5;
        let margin = h * 0.25;
        let mut positions: Vec<[f32; 3]> = Vec::new();
        let n = ((self.bounds.max[0] - self.bounds.min[0] + 2.0 * h) / sb)
            .ceil()
            .max(((self.bounds.max[1] - self.bounds.min[1] + 2.0 * h) / sb).ceil())
            .max(((self.bounds.max[2] - self.bounds.min[2] + 2.0 * h) / sb).ceil())
            as i32;
        for ix in 0..=n {
            for iy in 0..=n {
                for iz in 0..=n {
                    let p = [
                        self.bounds.min[0] - h + ix as f32 * sb,
                        self.bounds.min[1] - h + iy as f32 * sb,
                        self.bounds.min[2] - h + iz as f32 * sb,
                    ];
                    // Fuera del dominio de fluido en ALGÚN eje (estricto: un
                    // punto justo en el plano límite chocaría con el fluido).
                    let outside = (0..3).any(|a| {
                        p[a] <= self.bounds.min[a] + margin - SMALL
                            || p[a] >= self.bounds.max[a] - margin + SMALL
                    });
                    if outside {
                        positions.push(p);
                    }
                }
            }
        }
        // Masa efectiva ψ UNIFORME: ψ = ρ0 / Σ_b' W(lattice
        // infinita de paso sb). El vecindario LOCAL de una virtual del borde
        // está truncado (sobre-ponderaría); la lattice infinita no.
        let rho0 = self.config.rest_density.max(SMALL);
        let psi = rho0 / lattice_density(sb, h, 1.0).max(SMALL);
        let count = positions.len();
        let grid = SpatialGrid::build(&positions, h);
        self.boundary = positions;
        self.boundary_psi = vec![psi; count];
        self.boundary_grid = Some(grid);
    }

    /// Fase 1 — Predict: fuerzas + predicción.
    fn predict(&mut self, dt: f32, neighbors: &[Vec<Neighbor>], b_neighbors: &[Vec<u32>]) {
        let h = self.config.smoothing_radius;
        let mass = self.config.particle_mass;
        let normals = &self.normals;
        let boundary = &self.boundary;
        let boundary_psi = &self.boundary_psi;
        let curvature = self.config.curvature;
        let adhesion = self.config.boundary_adhesion;
        for i in 0..self.positions.len() {
            let mut vel = self.velocities[i];
            // Gravedad + cohesión superficial (K_ij corrige por asimetría de densidad).
            vel[1] += self.config.gravity * dt;
            if self.config.cohesion > 0.0 {
                let rho_i = self.densities[i].max(SMALL);
                let mut force = [0.0f32; 3];
                for neighbor in &neighbors[i] {
                    let j = neighbor.index as usize;
                    let rho_j = self.densities[j].max(SMALL);
                    let k_ij = (2.0 * self.config.rest_density / (rho_i + rho_j)).clamp(0.5, 2.0);
                    let c = cohesion_spline(neighbor.rlen, h);
                    // Dirección HACIA el vecino (atracción): opuesta a dir.
                    let dir = scale(neighbor.dir, -1.0);
                    force = add(force, scale(dir, k_ij * c * mass));
                    // B6 — curvatura: la diferencia de normales entre vecinos
                    // suaviza el contorno y redondea las gotas.
                    if curvature > 0.0 {
                        let delta = sub(normals[i], normals[j]);
                        force = add(force, scale(delta, -curvature * k_ij));
                    }
                }
                // a = F/m_i, con clamp de estabilidad.
                let accel = scale(force, self.config.cohesion / mass);
                let a_len = length(accel).max(SMALL);
                let clamped = if a_len > self.config.max_cohesion_accel {
                    scale(accel, self.config.max_cohesion_accel / a_len)
                } else {
                    accel
                };
                vel = add(vel, scale(clamped, dt));
            }
            // B6 — adhesión a las partículas de frontera.
            if adhesion > 0.0 {
                let mut force = [0.0f32; 3];
                for &b in &b_neighbors[i] {
                    let b = b as usize;
                    let r = sub(self.positions[i], boundary[b]);
                    let rlen = length(r);
                    let w = surface_adhesion(rlen, h);
                    if w <= 0.0 {
                        continue;
                    }
                    // Hacia la pared: el vector va de la pared a la partícula,
                    // así que se RESTA para que la atracción la acerque.
                    let dir = if rlen > SMALL {
                        scale(r, 1.0 / rlen)
                    } else {
                        [0.0; 3]
                    };
                    force = add(force, scale(dir, -adhesion * w * boundary_psi[b]));
                }
                vel = add(vel, scale(force, dt / mass));
            }
            // Viscosidad XSPH: difusión de velocidad (Σ(vj−vi)·W / ΣW).
            if self.config.viscosity > 0.0 {
                let mut corr = [0.0f32; 3];
                let mut weight = 0.0;
                for neighbor in &neighbors[i] {
                    let j = neighbor.index as usize;
                    corr = add(corr, scale(sub(self.velocities[j], vel), neighbor.w));
                    weight += neighbor.w;
                }
                if weight > SMALL {
                    vel = add(vel, scale(scale(corr, 1.0 / weight), self.config.viscosity));
                }
            }
            self.velocities[i] = vel;
            self.predicted[i] = add(self.positions[i], scale(vel, dt));
        }
    }

    /// Fase 2 — DensityPressure:
    /// densidad Poly6 → λ XPBD → Δp Spiky con s_corr → clamp contra paredes.
    fn solve_constraints(
        &mut self,
        dt: f32,
        neighbors: &[Vec<Neighbor>],
        b_neighbors: &[Vec<u32>],
        kinematic: &[[f32; 3]],
        dp_accum: &mut [[f32; 3]],
    ) {
        let h = self.config.smoothing_radius;
        let h2 = h * h;
        let mass = self.config.particle_mass;
        let inv_rest = 1.0 / self.config.rest_density.max(SMALL);
        let alpha_tilde = self.config.compliance / (dt * dt).max(1e-5);
        let n = self.positions.len();

        // Sub-pase A: densidad + λ (todos los λ del sub-pase anterior sirven de
        // warm start — igual que `LambdaIPrev` en el shader).
        for i in 0..n {
            let mut density = mass * poly6(0.0, h); // auto-contribución
            let mut sum_sq = 0.0f32;
            let mut grad_i = [0.0f32; 3];
            for neighbor in &neighbors[i] {
                let j = neighbor.index as usize;
                let r = sub(self.predicted[i], self.predicted[j]);
                let r2 = dot(r, r);
                if r2 >= h2 {
                    continue;
                }
                density += mass * poly6(r2, h);
                let grad_w = spiky_grad(normalize(r), r2.sqrt(), h);
                let grad_c = scale(grad_w, -inv_rest); // ∇C_j = −∇W/ρ0
                sum_sq += dot(grad_c, grad_c);
                grad_i = add(grad_i, scale(grad_w, inv_rest));
            }
            // Soporte de paredes: los virtuales aportan densidad
            // con su masa efectiva ψ_b, reconstruyendo la media cáscara que la
            // pared quita. NO entran en κ ni en Δp: su λ sería 0 y su gradiente
            // decaleraría λ hacia las paredes (los virtuales están más juntos,
            // paso h/2); la pared sigue actuando por clamp, que es estable.
            if self.config.boundary_support > 0.0 {
                let mut from_walls = 0.0f32;
                for &b in &b_neighbors[i] {
                    let d = sub(self.predicted[i], self.boundary[b as usize]);
                    let r2 = dot(d, d);
                    if r2 < h2 {
                        from_walls += self.boundary_psi[b as usize] * poly6(r2, h);
                    }
                }
                density += self.config.boundary_support * from_walls;
            }
            self.densities[i] = density;
            sum_sq += dot(grad_i, grad_i);

            // λ fresco por iteración, constraint de DESIGUALDAD: solo se
            // corrige la COMPRESIÓN (C > 0 → λ < 0 →
            // separa) porque la incompresibilidad es rígida. La atracción NO
            // va aquí: el λ de igualdad aplica la corrección completa de golpe
            // (correcto para un constraint rígido, catastrófico para la tensión
            // superficial que es blanda) y en partículas dilatadas con pocos
            // vecinos Σ|∇C|² es diminuto → λ enorme; el clamp convierte la
            // explosión en un ciclo bang-bang a velocidad máxima perpetua
            // (medido: v media clavada en 0.2·h/dt, el fluido llenando la caja
            // como gas y sin asentarse jamás). La atracción la aporta la
            // cohesión del predict: una ACELERACIÓN limitada (como la
            // gravedad), física y estable por construcción.
            let c = density * inv_rest - 1.0;
            self.lambdas[i] = if c > 0.0 && sum_sq + alpha_tilde > SMALL {
                -c / (sum_sq + alpha_tilde)
            } else {
                0.0
            };
        }

        // Sub-pase B: corrección de posición con los λ recién calculados.
        for i in 0..n {
            let mut dp = [0.0f32; 3];
            for neighbor in &neighbors[i] {
                let j = neighbor.index as usize;
                let r = sub(self.predicted[i], self.predicted[j]);
                let r2 = dot(r, r);
                if r2 >= h2 || r2 <= SMALL {
                    continue;
                }
                let grad_w = spiky_grad(normalize(r), r2.sqrt(), h);
                let s = scorr(
                    r2,
                    h,
                    self.config.tensile_k,
                    self.config.tensile_n,
                    self.config.tensile_delta_q,
                );
                dp = add(dp, scale(grad_w, self.lambdas[i] + self.lambdas[j] + s));
            }
            // Δp = (1/ρ0)·Σ(λi+λj+s_corr)·∇W, acumulado en `dp_accum` con clamp
            // de magnitud TOTAL del constraint por substep (0.2·h): en impactos
            // violentos o en partículas dilatadas con pocos vecinos, λ dispara
            // correcciones de medio metro que harían overshoot; el tope acota la
            // velocidad de corrección a 0.2·h/dt (~3.6 m/s a 60 Hz).
            let dp = scale(dp, inv_rest);
            dp_accum[i] = add(dp_accum[i], dp);
            let total = length(dp_accum[i]);
            let max_total = 0.2 * h;
            if total > max_total {
                dp_accum[i] = scale(dp_accum[i], max_total / total);
            }
            // Predicción = cinemática del predict + corrección acumulada: las
            // iteraciones refinan la corrección, no se suman en cascada.
            self.predicted[i] = add(kinematic[i], dp_accum[i]);
            // Paredes cada iteración: el fluido se queda DENTRO de la caja.
            let margin = h * 0.25;
            let mut p = self.predicted[i];
            self.clamp_position(&mut p, margin);
            self.predicted[i] = p;
        }
    }

    /// Fase 3 — PostStep: velocidad pura
    /// PBF `v = Δx/dt`, clamp, damping; la posición pasa a ser la corregida.
    fn post_step(&mut self, dt: f32) {
        let inv_dt = 1.0 / dt;
        for i in 0..self.positions.len() {
            let mut vel = scale(sub(self.predicted[i], self.positions[i]), inv_dt);
            let speed = length(vel);
            if speed > self.config.max_velocity {
                vel = scale(vel, self.config.max_velocity / speed);
            }
            vel = scale(vel, self.config.damping);
            self.positions[i] = self.predicted[i];
            self.velocities[i] = vel;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Caja de 1.8 × 1.2 × 1.8 m con suelo en y=0.
    fn test_bounds() -> FluidBounds {
        FluidBounds::new([0.0, 0.0, 0.0], [1.8, 1.2, 1.8])
    }

    /// Solver con bloque spawneado en el aire, listo para caer.
    fn solver_with_block() -> FluidSolver {
        let config = FluidConfig::default();
        let mut solver = FluidSolver::new(config, test_bounds());
        let count = solver.spawn_block([0.9, 0.75, 0.9], [0.3, 0.3, 0.3], 0.15);
        assert!(count > 40, "el bloque debe spawnear partículas: {count}");
        solver
    }

    #[test]
    fn kernels_are_sane() {
        let h = 0.3;
        // Poly6: máximo en r=0, estrictamente positivo dentro de h, cero fuera.
        assert!(poly6(0.0, h) > poly6(0.1 * h, h));
        assert_eq!(poly6(h * h, h), 0.0);
        assert_eq!(poly6(4.0 * h * h, h), 0.0);
        // Spiky: el ∇W apunta HACIA el vecino (el vecino está en −x si
        // dir_i_minus_j = +x), y decae a 0 en h.
        let dir = normalize([1.0, 0.0, 0.0]);
        let g = spiky_grad(dir, 0.15, h);
        assert!(g[0] < 0.0 && g[1] == 0.0 && g[2] == 0.0);
        assert_eq!(spiky_grad(dir, h, h)[0], 0.0);
        // s_corr: repulsión (negativa) que crece cerca, nula en el borde.
        assert!(scorr(0.1 * 0.1, h, 2.0, 4, 0.3) < 0.0);
        assert_eq!(scorr(h * h, h, 2.0, 4, 0.3), 0.0);
        // Cohesión superficial: atracción a r≈h/2, repulsión cerca de 0.
        assert!(cohesion_spline(0.5 * h, h) > 0.0);
        assert!(cohesion_spline(0.05 * h, h) < 0.0);
        assert_eq!(cohesion_spline(2.0 * h, h), 0.0);
    }

    #[test]
    fn block_falls_and_stays_inside_the_container() {
        let mut solver = solver_with_block();
        let spawn_top = 0.75 + 0.3;
        let spawn_center_y = 0.75;
        // 2 s de simulación a 60 Hz: el bloque cae ~0.65 m, salpica y se asienta.
        for _ in 0..120 {
            solver.step(1.0 / 60.0);
        }
        let margin = solver.config.smoothing_radius * 0.25;
        let mut sum_y = 0.0;
        let mut sum_speed = 0.0;
        for (i, &p) in solver.positions().iter().enumerate() {
            assert!(
                solver.position_inside(p, margin * 0.5),
                "partícula fuera de la caja: {p:?}"
            );
            // El impacto puede lanzar un salpicón por encima del spawn (física
            // real); lo que no puede es flotar indefinidamente: 0.6 m de margen.
            assert!(p[1] < spawn_top + 0.6, "una partícula salió volando: {p:?}");
            sum_y += p[1];
            sum_speed += length(solver.velocities[i]);
        }
        let count = solver.particle_count() as f32;
        // El bloque CAYÓ y se convirtió en charco: centro de masa bajo el centro
        // de spawn.
        assert!(
            sum_y / count < spawn_center_y,
            "el bloque no cayó: centro de masa {}",
            sum_y / count
        );
        // El fluido se asentó: velocidad media baja (reposo volumétrico).
        let avg_speed = sum_speed / count;
        assert!(
            avg_speed < 0.5,
            "el fluido no se estabilizó: velocidad media {avg_speed}"
        );
    }

    #[test]
    fn settled_density_stays_close_to_rest() {
        // Incompresibilidad en una caja PROPORCIONAL al volumen del bloque: 125
        // partículas (5³, espaciado 0.15 → 0.42 m³ de agua) sobre 0.81 m² de
        // fondo ≈ 0.52 m de profundidad ≈ 3.4 capas. Con ≥3 capas existen
        // partículas interiores con la cáscara 3D de vecinos completa, así que
        // el promedio puede acercarse de verdad a ρ0. (Una caja más grande con
        // menos agua es un charco de 1-2 capas donde CASI TODA partícula es
        // superficie o pared: su densidad de equilibrio es ~0.42·ρ0 legítimamente
        // — lámina 2D ≈ 0.55·ρ0, capa superior de una pila ≈ 0.37·ρ0 — y no
        // mide incompresibilidad.)
        let config = FluidConfig::default();
        let basin = FluidBounds::new([0.0, 0.0, 0.0], [1.05, 1.2, 1.05]);
        let mut solver = FluidSolver::new(config, basin);
        let count = solver.spawn_block([0.525, 0.6, 0.525], [0.375, 0.375, 0.375], 0.15);
        assert_eq!(count, 125, "el bloque 5³ debe spawnear completo: {count}");
        for _ in 0..150 {
            solver.step(1.0 / 60.0);
        }
        let avg_density: f32 = solver.densities.iter().sum::<f32>() / solver.densities.len() as f32;
        let ratio = avg_density / solver.config.rest_density;
        assert!(
            (0.6..=1.35).contains(&ratio),
            "densidad media {avg_density:.0} vs ρ0 {:.0} (ratio {ratio:.2}): el fluido se \
             colapsó o se dispersó",
            solver.config.rest_density
        );
        // Todo sigue dentro de la caja y en reposo volumétrico.
        let margin = solver.config.smoothing_radius * 0.25;
        let mut sum_speed = 0.0;
        for (i, &p) in solver.positions().iter().enumerate() {
            assert!(
                solver.position_inside(p, margin * 0.5),
                "partícula fuera de la caja: {p:?}"
            );
            sum_speed += length(solver.velocities[i]);
        }
        let avg_speed = sum_speed / solver.particle_count() as f32;
        assert!(
            avg_speed < 0.5,
            "el fluido no se estabilizó: velocidad media {avg_speed}"
        );
    }

    #[test]
    fn solver_is_deterministic() {
        let mut a = solver_with_block();
        let mut b = solver_with_block();
        for _ in 0..60 {
            a.step(1.0 / 60.0);
            b.step(1.0 / 60.0);
        }
        assert_eq!(
            a.positions(),
            b.positions(),
            "mismo input → mismo resultado"
        );
    }

    /// B4: la lista detallada guarda el mismo peso y dirección que se
    /// calcularían en sitio, de modo que las fases de fuerzas no repitan sqrt.
    #[test]
    fn detailed_neighbors_cache_kernel_data() {
        let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [0.1, 0.0, 0.0]];
        let grid = SpatialGrid::build(&positions, 0.3);
        let found = grid.neighbors_detailed(&positions, 0, positions[0], 0.3, 48);
        assert_eq!(found.len(), 1);
        let neighbor = found[0];
        assert_eq!(neighbor.index, 1);
        assert!((neighbor.rlen - 0.1).abs() < 1e-6);
        assert!((neighbor.dir[0] + 1.0).abs() < 1e-6);
        assert!((neighbor.w - poly6(0.01, 0.3)).abs() < 1e-6);
    }

    /// B5: las partículas de frontera del terreno sustituyen a las paredes de
    /// la caja cuando se proporcionan.
    #[test]
    fn terrain_boundary_replaces_box_walls() {
        let mut solver = solver_with_block();
        let points: Vec<[f32; 3]> = vec![[0.2, 0.0, 0.2], [0.4, 0.0, 0.2], [0.6, 0.0, 0.2]];
        let volumes = vec![0.008; points.len()];
        solver.set_terrain_boundary(&points, &volumes);
        solver.step(1.0 / 60.0);
        assert_eq!(solver.boundary, points);
        assert!(solver.boundary_grid.is_some());
        assert!(solver.boundary_psi.iter().all(|&psi| psi > 0.0));
    }

    /// B8: con partículas rápidas el criterio CFL debe subdividir el frame.
    #[test]
    fn cfl_adds_substeps_for_fast_fluid() {
        let mut solver = FluidSolver::new(
            FluidConfig::default(),
            FluidBounds::new([0.0; 3], [10.0, 10.0, 10.0]),
        );
        assert!(solver.spawn_block([5.0, 1.0, 5.0], [0.2, 0.2, 0.2], 0.15) > 0);
        for velocity in solver.velocities.iter_mut() {
            *velocity = [20.0, 0.0, 0.0];
        }
        assert!(solver.cfl_substeps(1.0 / 60.0) > 1);
        for velocity in solver.velocities.iter_mut() {
            *velocity = [0.0; 3];
        }
        assert_eq!(solver.cfl_substeps(1.0 / 60.0), 1);
    }

    /// B3: con el fluido asentado el early-out debe reducir iteraciones por
    /// debajo del máximo, manteniendo el error de densidad dentro del umbral.
    #[test]
    fn solver_exits_early_when_settled() {
        let mut solver = solver_with_block();
        for _ in 0..180 {
            solver.step(1.0 / 60.0);
        }
        assert!(
            solver.last_iterations < solver.config.solver_iterations,
            "debe salir antes: {} de {}",
            solver.last_iterations,
            solver.config.solver_iterations
        );
        assert!(solver.last_density_error <= solver.config.max_density_error);
    }

    /// B6: las normales de superficie se calculan y la simulación sigue estable
    /// con curvatura y adhesión activas.
    #[test]
    fn surface_normals_are_computed_and_simulation_stays_stable() {
        let config = FluidConfig {
            curvature: 0.4,
            boundary_adhesion: 0.4,
            ..Default::default()
        };
        let mut solver = FluidSolver::new(config, FluidBounds::new([0.0; 3], [1.8, 1.2, 1.8]));
        let count = solver.spawn_block([0.9, 0.75, 0.9], [0.3, 0.3, 0.3], 0.15);
        assert!(count > 40);
        for _ in 0..120 {
            solver.step(1.0 / 60.0);
        }
        assert_eq!(solver.normals.len(), count);
        assert!(
            solver
                .normals
                .iter()
                .any(|n| n[0].abs() + n[1].abs() + n[2].abs() > 0.1),
            "deben calcularse normales de superficie"
        );
        let margin = solver.config.smoothing_radius * 0.25;
        for &p in solver.positions() {
            assert!(p.iter().all(|component| component.is_finite()));
            assert!(solver.position_inside(p, margin * 0.5), "fuera: {p:?}");
        }
    }

    #[test]
    fn spawn_block_clips_outside_particles() {
        // Vaso mínimo: el cubo solo cabe en la zona interior; no se spawnea nada
        // fuera de la caja (mismo margen que el clamp del solver).
        let config = FluidConfig::default();
        let tiny = FluidBounds::new([0.0, 0.0, 0.0], [0.4, 0.4, 0.4]);
        let mut solver = FluidSolver::new(config, tiny);
        let count = solver.spawn_block([0.2, 0.2, 0.2], [0.5, 0.5, 0.5], 0.15);
        assert!(count > 0);
        let margin = solver.config.smoothing_radius * 0.25;
        for &p in solver.positions() {
            assert!(solver.position_inside(p, margin * 0.5), "fuera: {p:?}");
        }
    }
}
