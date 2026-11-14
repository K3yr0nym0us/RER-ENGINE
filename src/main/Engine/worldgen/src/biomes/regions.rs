//! Regiones del reparto por porcentajes: etiquetas en rejilla sin fragmentos
//! pequeños y pesos difuminados para fundir las fronteras.

use super::Biome;

/// Celdas por eje de la rejilla que cubre el cuadrado del disco.
pub(super) const GRID: usize = 128;
/// Radio (celdas) del difuminado de caja; se aplica [`BLUR_PASSES`] veces.
const BLUR_RADIUS: isize = 2;
const BLUR_PASSES: usize = 2;
/// Un trozo aislado de bioma por debajo de esta fracción del disco se funde
/// con el bioma que lo rodea (salvo que sea la región mayor de su bioma).
const MIN_FRAGMENT: f32 = 0.04;
/// Pasadas de corrección de cuantiles tras fundir fragmentos.
const BALANCE_ITERS: usize = 6;

/// Pesos por bioma (índice canónico) en cada celda de la rejilla.
#[derive(Debug, Clone)]
pub(super) struct RegionGrid {
    weights: Vec<[f32; 5]>,
}

struct Component {
    label: usize,
    cells: Vec<usize>,
}

impl RegionGrid {
    /// `values`: campo de regiones por celda (fila a fila en z); `inside`:
    /// celda dentro del disco; `bands`: biomas de suelo con su porcentaje.
    pub(super) fn build(values: &[f32], inside: &[bool], bands: &[(Biome, f32)]) -> Option<Self> {
        let mut sorted: Vec<f32> = values
            .iter()
            .zip(inside)
            .filter_map(|(value, inside)| inside.then_some(*value))
            .collect();
        let last_band = bands.last()?.0;
        let last = sorted.len().checked_sub(1)?;
        sorted.sort_by(f32::total_cmp);

        let total: f32 = bands.iter().map(|(_, share)| share).sum();
        let targets: Vec<f32> = bands
            .iter()
            .scan(0.0, |cumulative, (_, share)| {
                *cumulative += share / total;
                Some(*cumulative)
            })
            .collect();
        let min_cells = (MIN_FRAGMENT * sorted.len() as f32) as usize;

        // Fundir fragmentos desvía las áreas: se corrigen los cuantiles con lo
        // obtenido y se repite.
        let mut ranks = targets.clone();
        let mut labels = Vec::new();
        for _ in 0..BALANCE_ITERS {
            let uppers: Vec<f32> = ranks
                .iter()
                .enumerate()
                .map(|(i, rank)| {
                    if i + 1 == bands.len() {
                        f32::INFINITY
                    } else {
                        sorted[((rank * last as f32).round() as usize).min(last)]
                    }
                })
                .collect();
            labels = values
                .iter()
                .map(|value| {
                    bands
                        .iter()
                        .zip(&uppers)
                        .find(|(_, upper)| *value < **upper)
                        .map_or(last_band, |((biome, _), _)| *biome)
                        .index()
                })
                .collect();
            merge_fragments(&mut labels, inside, min_cells);

            let mut achieved = 0.0;
            let mut floor = 0.0f32;
            for (i, (biome, _)) in bands.iter().enumerate() {
                let count = labels
                    .iter()
                    .zip(inside)
                    .filter(|(label, inside)| **inside && **label == biome.index())
                    .count();
                achieved += count as f32 / sorted.len() as f32;
                ranks[i] = (ranks[i] + targets[i] - achieved).clamp(floor, 1.0);
                floor = ranks[i];
            }
        }
        fill_outside(&mut labels, inside);
        let mut weights = blurred(&labels);

        // El difuminado puede estrangular cuellos estrechos y dejar trozos
        // sueltos: se reetiqueta por máximo, se limpia otra vez y se difumina.
        for (label, weight) in labels.iter_mut().zip(&weights) {
            *label = argmax(weight);
        }
        merge_fragments(&mut labels, inside, min_cells);
        fill_outside(&mut labels, inside);
        weights = blurred(&labels);
        Some(Self { weights })
    }

    /// Pesos interpolados en coordenadas de celda continuas (centro de la
    /// celda `i` en `i`).
    pub(super) fn sample(&self, gx: f32, gz: f32) -> [f32; 5] {
        let max = (GRID - 1) as f32;
        let (gx, gz) = (gx.clamp(0.0, max), gz.clamp(0.0, max));
        let (x0, z0) = (gx.floor() as usize, gz.floor() as usize);
        let (x1, z1) = ((x0 + 1).min(GRID - 1), (z0 + 1).min(GRID - 1));
        let (fx, fz) = (gx - x0 as f32, gz - z0 as f32);
        let at = |x: usize, z: usize| self.weights[z * GRID + x];
        let (a, b, c, d) = (at(x0, z0), at(x1, z0), at(x0, z1), at(x1, z1));
        let mut out = [0.0f32; 5];
        for (i, slot) in out.iter_mut().enumerate() {
            let top = a[i] + (b[i] - a[i]) * fx;
            let bottom = c[i] + (d[i] - c[i]) * fx;
            *slot = top + (bottom - top) * fz;
        }
        out
    }
}

fn neighbours(i: usize) -> impl Iterator<Item = usize> {
    let (x, z) = (i % GRID, i / GRID);
    [
        (x > 0).then(|| i - 1),
        (x + 1 < GRID).then(|| i + 1),
        (z > 0).then(|| i - GRID),
        (z + 1 < GRID).then(|| i + GRID),
    ]
    .into_iter()
    .flatten()
}

/// Regiones conexas (vecindad 4) de celdas dentro del disco: dos trozos que
/// solo se tocan por fuera del mundo son regiones distintas.
fn components(labels: &[usize], inside: &[bool]) -> Vec<Component> {
    let mut seen: Vec<bool> = inside.iter().map(|inside| !inside).collect();
    let mut stack = Vec::new();
    let mut out = Vec::new();
    for start in 0..labels.len() {
        if seen[start] {
            continue;
        }
        let label = labels[start];
        seen[start] = true;
        stack.push(start);
        let mut cells = Vec::new();
        while let Some(i) = stack.pop() {
            cells.push(i);
            for j in neighbours(i) {
                if !seen[j] && labels[j] == label {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        out.push(Component { label, cells });
    }
    out
}

/// Funde cada trozo menor que `min_cells` con el bioma vecino más frecuente;
/// la región mayor de cada bioma se conserva para que ninguno desaparezca.
fn merge_fragments(labels: &mut [usize], inside: &[bool], min_cells: usize) {
    let all = components(labels, inside);
    let mut largest = [0usize; 5];
    for component in &all {
        largest[component.label] = largest[component.label].max(component.cells.len());
    }
    let mut small: Vec<&Component> = all
        .iter()
        .filter(|c| c.cells.len() < min_cells && c.cells.len() < largest[c.label])
        .collect();
    small.sort_by_key(|c| c.cells.len());
    for component in small {
        let mut counts = [0usize; 5];
        for &i in &component.cells {
            for j in neighbours(i) {
                if inside[j] && labels[j] != component.label {
                    counts[labels[j]] += 1;
                }
            }
        }
        let target = counts
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .max_by_key(|(_, count)| **count)
            .map(|(label, _)| label);
        if let Some(target) = target {
            for &i in &component.cells {
                labels[i] = target;
            }
        }
    }
}

/// Las celdas fuera del disco toman la etiqueta interior más cercana (BFS),
/// para que el difuminado no meta biomas ajenos en el borde del mundo.
fn fill_outside(labels: &mut [usize], inside: &[bool]) {
    let mut done = inside.to_vec();
    let mut queue: std::collections::VecDeque<usize> =
        (0..labels.len()).filter(|i| inside[*i]).collect();
    while let Some(i) = queue.pop_front() {
        for j in neighbours(i) {
            if !done[j] {
                done[j] = true;
                labels[j] = labels[i];
                queue.push_back(j);
            }
        }
    }
}

fn argmax(weights: &[f32; 5]) -> usize {
    weights
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map_or(0, |(i, _)| i)
}

/// Pesos one-hot de `labels` difuminados [`BLUR_PASSES`] veces.
fn blurred(labels: &[usize]) -> Vec<[f32; 5]> {
    let mut weights: Vec<[f32; 5]> = labels
        .iter()
        .map(|label| {
            let mut one_hot = [0.0; 5];
            one_hot[*label] = 1.0;
            one_hot
        })
        .collect();
    for _ in 0..BLUR_PASSES {
        blur(&mut weights);
    }
    weights
}

/// Difuminado de caja separable con bordes replicados.
fn blur(weights: &mut [[f32; 5]]) {
    let norm = 1.0 / (2 * BLUR_RADIUS + 1) as f32;
    let max = GRID as isize - 1;
    for horizontal in [true, false] {
        let source = weights.to_vec();
        for z in 0..GRID {
            for x in 0..GRID {
                let mut acc = [0.0f32; 5];
                for k in -BLUR_RADIUS..=BLUR_RADIUS {
                    let (sx, sz) = if horizontal {
                        ((x as isize + k).clamp(0, max) as usize, z)
                    } else {
                        (x, (z as isize + k).clamp(0, max) as usize)
                    };
                    for (slot, value) in acc.iter_mut().zip(source[sz * GRID + sx]) {
                        *slot += value;
                    }
                }
                weights[z * GRID + x] = acc.map(|value| value * norm);
            }
        }
    }
}
