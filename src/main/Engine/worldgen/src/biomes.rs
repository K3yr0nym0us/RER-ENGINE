//! Mapa de biomas: clasificación Whittaker (temperatura + humedad) con nieve
//! solo por altitud.
//!
//! Antes el ruido 2D se partía en bandas iguales: con los 5 biomas activos el
//! 20 % del disco era nieve a cualquier cota y otro 20 % desierto. Ahora la
//! pradera es el suelo, el bosque sale de humedad, el desierto de calor seco,
//! el volcán de un hotspot raro y la nieve de la línea de cumbres.
//!
//! Con porcentajes (`BiomesConfig::shares`) un campo de regiones amplias se
//! reparte por cuantiles entre los biomas de suelo; la nieve ocupa su fracción
//! en las cotas más altas del relieve.

use glam::Vec2;

use crate::config::plains_height;
use crate::field::smoothstep;
use crate::noise::{FractalType, Noise};
use crate::planner::{FeatureKind, Site};
use crate::scale::{snow_line_meters, world_radius};
use crate::seed::domain_seed;

mod regions;

use regions::{GRID, RegionGrid};

/// Biomas soportados. El orden es parte del contrato (prioridad de huecos);
/// no reordenar sin romper mundos ya generados.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Biome {
    /// Pradera (hierba). Suelo por defecto.
    Grassland,
    /// Bosque (hierba; los árboles llegan con el scatter).
    Forest,
    /// Desierto (arena).
    Desert,
    /// Cimas nevadas.
    Snow,
    /// Zona volcánica (roca/arena oscura).
    Volcanic,
}

impl Biome {
    /// Índice canónico del bioma.
    #[inline]
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Grassland => 0,
            Self::Forest => 1,
            Self::Desert => 2,
            Self::Snow => 3,
            Self::Volcanic => 4,
        }
    }

    /// Todos los biomas en orden canónico.
    pub const ALL: [Biome; 5] = [
        Biome::Grassland,
        Biome::Forest,
        Biome::Desert,
        Biome::Snow,
        Biome::Volcanic,
    ];
}

/// Orden de las bandas de suelo por porcentaje. Nieve y volcánico no son
/// bandas 2D: nieve en cumbres, volcánico alrededor de montañas.
const SHARE_BAND_ORDER: [Biome; 3] = [Biome::Desert, Biome::Grassland, Biome::Forest];
/// Muestras por eje para medir el cuantil de altura de la nieve en el disco.
const SHARE_SAMPLES: u32 = 96;
/// Amplitud (celdas de rejilla) del desplazamiento que curva las fronteras.
const SHARE_WARP_CELLS: f32 = 3.6;
/// Frecuencia del desplazamiento relativa al campo de regiones.
const SHARE_WARP_SCALE: f32 = 3.8;
/// Media anchura de la transición vertical de la nieve (metros).
const SNOW_BLEND_M: f32 = 1.0;
/// La nieve nunca baja de esta altura sobre las llanuras (metros).
const SNOW_MIN_RISE_M: f32 = 32.0;
/// Aunque el porcentaje pida más, la nieve no baja de este tramo superior
/// del relieve sobre las llanuras (1 = cumbre). 0.97 ≈ solo el último 3 %.
const SNOW_PEAK_SPAN: f32 = 0.97;
/// Solo las muestras más altas del disco pueden ser nieve (0.98 = percentil 98).
const SNOW_HEIGHT_QUANTILE: f32 = 0.98;
/// El desierto no pisa el vaso del lago (`norm_dist` 1 = orilla).
const DESERT_LAKE_PAD: f32 = 1.22;
/// Distancia mínima del desierto al eje de un río (metros).
const DESERT_RIVER_M: f32 = 18.0;

/// Reparto por porcentajes: regiones de suelo y cota de nieve.
#[derive(Debug, Clone)]
struct ShareBands {
    /// Regiones de los biomas de suelo; `None` si solo hay nieve.
    regions: Option<RegionGrid>,
    /// Fracción del disco pedida para la nieve (0 = sin nieve).
    snow_fraction: f32,
    /// Altura desde la que hay nieve. [`BiomeMap::for_field`] la ajusta al
    /// relieve real; sin terreno vale la línea de nieve.
    snow_y: f32,
    /// Fracción pedida para el volcánico (anillo alrededor de montañas).
    volcanic_fraction: f32,
    /// Umbral de `norm_dist` de montaña: por debajo hay volcánico.
    volcanic_t: f32,
}

/// Mapa de biomas del mundo.
#[derive(Debug, Clone)]
pub struct BiomeMap {
    temperature: Noise,
    moisture: Noise,
    volcanic: Noise,
    /// Regiones grandes del reparto por porcentajes (pocas octavas).
    regions: Noise,
    enabled: [bool; 5],
    snow_line: f32,
    plains_y: f32,
    shares: Option<ShareBands>,
    mountains: Vec<Site>,
    lakes: Vec<Site>,
    rivers: Vec<Vec<Vec2>>,
    radius: f32,
    center: (f32, f32),
}

impl BiomeMap {
    /// Construye el mapa a partir de la config (tras `fit_config_to_world`).
    #[must_use]
    pub fn new(config: &crate::config::WorldGenConfig) -> Self {
        let biomes = config.biomes;
        let radius = world_radius(&config.bounds).max(32.0);
        // Más ciclos en mundos grandes: si no, un mapa de 1 km queda en un
        // solo clima (casi todo desierto o todo hierba).
        let cycles = (2.6 + radius / 160.0).clamp(2.6, 7.0);
        let diameter = radius * 2.0;
        let temp_freq = cycles / diameter;
        let moist_freq = (cycles * 1.3) / diameter;
        let mut map = Self {
            temperature: Noise::new(domain_seed(config.seed, crate::seed::SALT_BIOME))
                .frequency(temp_freq)
                .fractal(FractalType::Fbm, 3, 0.5, 0.0),
            moisture: Noise::new(domain_seed(config.seed, crate::seed::SALT_MOISTURE))
                .frequency(moist_freq)
                .fractal(FractalType::Fbm, 3, 0.55, 0.0),
            volcanic: Noise::new(domain_seed(config.seed, crate::seed::SALT_VOLCANIC))
                .frequency((cycles * 1.8) / diameter)
                .fractal(FractalType::Ridged, 2, 0.5, 0.0),
            regions: Noise::new(domain_seed(config.seed, crate::seed::SALT_BIOME_REGIONS))
                .frequency((1.9 + radius / 320.0).clamp(1.9, 4.2) / diameter)
                .fractal(FractalType::Fbm, 3, 0.42, 0.0),
            enabled: [
                biomes.grassland,
                biomes.forest,
                biomes.desert,
                biomes.snow,
                biomes.volcanic,
            ],
            snow_line: snow_line_meters(config),
            plains_y: plains_height(),
            shares: None,
            mountains: Vec::new(),
            lakes: Vec::new(),
            rivers: Vec::new(),
            radius: world_radius(&config.bounds),
            center: (config.bounds.width * 0.5, config.bounds.length * 0.5),
        };
        if let Some(shares) = biomes.shares {
            map.shares = map.share_bands(shares);
        }
        map
    }

    /// Mapa con la nieve ajustada al relieve real del campo: el porcentaje de
    /// nieve cubre las cotas más altas del disco.
    #[must_use]
    pub fn for_field(
        config: &crate::config::WorldGenConfig,
        field: &crate::field::DensityField,
    ) -> Self {
        let mut map = Self::new(config);
        map.mountains = field
            .plan()
            .sites
            .iter()
            .copied()
            .filter(|site| site.kind == FeatureKind::Mountain)
            .collect();
        map.lakes = field
            .plan()
            .sites
            .iter()
            .copied()
            .filter(|site| site.kind == FeatureKind::Lake)
            .collect();
        map.rivers = field
            .plan()
            .rivers
            .iter()
            .map(|river| river.points.iter().map(|(pos, _)| *pos).collect())
            .collect();
        map.fit_snow_to_terrain(|x, z| field.height_at(x, z));
        map.fit_volcanic_to_mountains();
        map
    }

    /// Fija la cota de nieve en las cumbres: el porcentaje solo decide cuánto
    /// del tramo alto se cubre, nunca baja a lomas ni llanuras.
    pub fn fit_snow_to_terrain(&mut self, height_at: impl Fn(f32, f32) -> f32) {
        let floor = self.plains_y + SNOW_MIN_RISE_M;
        let Some(fraction) = self.shares.as_ref().map(|s| s.snow_fraction) else {
            return;
        };
        if fraction <= 0.0 {
            return;
        }
        let mut heights = self.disk_samples(height_at);
        if heights.is_empty() {
            return;
        }
        heights.sort_by(|a, b| a.total_cmp(b));
        let high = heights[heights.len() - 1];
        // El mínimo del disco incluye lechos de lago: no sirve de llanura.
        let base = self.plains_y;
        let peak = high.max(base + 1.0);
        let summit_floor = base + (peak - base) * SNOW_PEAK_SPAN;
        let quantile_idx = ((heights.len() - 1) as f32 * SNOW_HEIGHT_QUANTILE).floor() as usize;
        let quantile_y = heights[quantile_idx];
        let span = (peak - summit_floor).max(1.0);
        let snow_y = (peak - span * fraction)
            .max(quantile_y)
            .max(summit_floor)
            .max(floor);
        self.snow_line = snow_y;
        if let Some(shares) = self.shares.as_mut() {
            shares.snow_y = snow_y;
        }
    }

    /// El volcánico cubre las faldas de las montañas, no manchas sueltas.
    fn fit_volcanic_to_mountains(&mut self) {
        let Some(fraction) = self.shares.as_ref().map(|s| s.volcanic_fraction) else {
            return;
        };
        if fraction <= 0.0 || self.mountains.is_empty() {
            if let Some(shares) = self.shares.as_mut() {
                shares.volcanic_t = 0.0;
            }
            return;
        }
        let mut distances = self.disk_samples(|x, z| self.mountain_norm(x, z));
        if distances.is_empty() {
            return;
        }
        distances.sort_by(|a, b| a.total_cmp(b));
        let idx = ((distances.len() - 1) as f32 * fraction.clamp(0.02, 0.9)).floor() as usize;
        let volcanic_t = distances[idx].max(0.4);
        if let Some(shares) = self.shares.as_mut() {
            shares.volcanic_t = volcanic_t;
        }
    }

    fn mountain_norm(&self, x: f32, z: f32) -> f32 {
        self.mountains
            .iter()
            .map(|site| site.norm_dist(x, z))
            .fold(f32::MAX, f32::min)
    }

    fn share_volcanic(&self, shares: &ShareBands, x: f32, z: f32) -> f32 {
        if shares.volcanic_fraction <= 0.0 || shares.volcanic_t <= 0.0 || self.mountains.is_empty()
        {
            return 0.0;
        }
        let t = self.mountain_norm(x, z);
        1.0 - smoothstep(shares.volcanic_t * 0.68, shares.volcanic_t, t)
    }

    /// Valores de `f` en una rejilla regular dentro del disco del mundo.
    fn disk_samples(&self, f: impl Fn(f32, f32) -> f32) -> Vec<f32> {
        let (cx, cz) = self.center;
        let radius = self.radius;
        let step = (radius * 2.0) / SHARE_SAMPLES as f32;
        let mut values = Vec::with_capacity((SHARE_SAMPLES * SHARE_SAMPLES) as usize);
        for iz in 0..SHARE_SAMPLES {
            for ix in 0..SHARE_SAMPLES {
                let x = cx - radius + (ix as f32 + 0.5) * step;
                let z = cz - radius + (iz as f32 + 0.5) * step;
                if (x - cx).hypot(z - cz) <= radius {
                    values.push(f(x, z));
                }
            }
        }
        values
    }

    /// Umbrales de las regiones para que cada bioma de suelo cubra su
    /// porcentaje del disco; la nieve queda como fracción por altura.
    fn share_bands(&self, shares: crate::config::BiomeShares) -> Option<ShareBands> {
        let percent = |biome: Biome| -> f32 {
            if !self.enabled[biome.index()] {
                return 0.0;
            }
            f32::from(match biome {
                Biome::Grassland => shares.grassland,
                Biome::Forest => shares.forest,
                Biome::Desert => shares.desert,
                Biome::Snow => shares.snow,
                Biome::Volcanic => shares.volcanic,
            })
        };
        let active: Vec<(Biome, f32)> = SHARE_BAND_ORDER
            .iter()
            .map(|biome| (*biome, percent(*biome)))
            .filter(|(_, value)| *value > 0.0)
            .collect();
        let ground_total: f32 = active.iter().map(|(_, value)| value).sum();
        let snow = percent(Biome::Snow);
        let volcanic = percent(Biome::Volcanic);
        let total = ground_total + snow + volcanic;
        if total <= 0.0 {
            return None;
        }

        let regions = if active.is_empty() {
            None
        } else {
            let (ox, oz, cell) = self.grid_frame();
            let (cx, cz) = self.center;
            let mut values = Vec::with_capacity(GRID * GRID);
            let mut inside = Vec::with_capacity(GRID * GRID);
            for iz in 0..GRID {
                for ix in 0..GRID {
                    let x = ox + (ix as f32 + 0.5) * cell;
                    let z = oz + (iz as f32 + 0.5) * cell;
                    // Tres escalas anisotrópicas: un solo FBM suave da
                    // cuantiles casi circulares (el desierto parecía un disco).
                    let n1 = self.regions.get2(x, z);
                    let n2 = self.regions.get2(x * 1.7 + 71.0, z * 0.58 - 43.0);
                    let n3 = self.regions.get2(x * 0.52 - 19.0, z * 1.45 + 88.0);
                    values.push(n1 * 0.48 + n2 * 0.32 + n3 * 0.20);
                    inside.push((x - cx).hypot(z - cz) <= self.radius);
                }
            }
            RegionGrid::build(&values, &inside, &active)
        };
        Some(ShareBands {
            regions,
            snow_fraction: snow / total,
            snow_y: self.snow_line,
            volcanic_fraction: volcanic / total,
            volcanic_t: 0.0,
        })
    }

    /// Esquina mínima y tamaño de celda de la rejilla de regiones.
    fn grid_frame(&self) -> (f32, f32, f32) {
        let (cx, cz) = self.center;
        let cell = (self.radius * 2.0).max(1.0) / GRID as f32;
        (cx - self.radius, cz - self.radius, cell)
    }

    /// Pesos de los biomas de suelo en `(x, z)`, con las fronteras curvadas
    /// por un desplazamiento suave para que no sigan la rejilla.
    fn region_weights(&self, regions: &RegionGrid, x: f32, z: f32) -> [f32; 5] {
        let (ox, oz, cell) = self.grid_frame();
        let warp = SHARE_WARP_CELLS * cell;
        let (sx, sz) = (x * SHARE_WARP_SCALE, z * SHARE_WARP_SCALE);
        let wx = x + self.regions.get2(sx + 113.0, sz) * warp;
        let wz = z + self.regions.get2(sx, sz - 271.0) * warp;
        regions.sample((wx - ox) / cell - 0.5, (wz - oz) / cell - 0.5)
    }

    /// Peso de nieve por altura en el reparto por porcentajes.
    fn share_snow(shares: &ShareBands, y: f32) -> f32 {
        if shares.snow_fraction <= 0.0 {
            return 0.0;
        }
        if shares.regions.is_none() {
            return 1.0;
        }
        smoothstep(
            shares.snow_y - SNOW_BLEND_M,
            shares.snow_y + SNOW_BLEND_M,
            y,
        )
    }

    /// `true` si el reparto sale de los porcentajes de la config.
    #[inline]
    #[must_use]
    pub fn uses_shares(&self) -> bool {
        self.shares.is_some()
    }

    /// `true` si el bioma está activo en la config.
    #[inline]
    #[must_use]
    pub fn is_enabled(&self, biome: Biome) -> bool {
        self.enabled[biome.index()]
    }

    /// Peso `[0, 1]` de cada bioma (índice canónico) en `(x, z, y)`; suma 1.
    ///
    /// Con porcentajes las fronteras se funden en una banda estrecha; sin
    /// ellos el bioma es único por punto.
    #[must_use]
    pub fn biome_weights(&self, x: f32, z: f32, y: f32) -> [f32; 5] {
        let mut weights = [0.0f32; 5];
        let Some(shares) = &self.shares else {
            weights[self.biome_at(x, z, y).index()] = 1.0;
            return weights;
        };
        let snow = Self::share_snow(shares, y);
        let volcanic = self.share_volcanic(shares, x, z) * (1.0 - snow);
        weights[Biome::Snow.index()] = snow;
        weights[Biome::Volcanic.index()] = volcanic;
        let ground = (1.0 - snow - volcanic).max(0.0);
        let Some(regions) = shares.regions.as_ref().filter(|_| ground > 0.0) else {
            return weights;
        };
        let region = self.region_weights(regions, x, z);
        for (biome, (slot, value)) in weights.iter_mut().zip(region).enumerate() {
            if biome != Biome::Snow.index() && biome != Biome::Volcanic.index() {
                *slot = value * ground;
            }
        }
        self.keep_desert_away_from_water(&mut weights, x, z);
        weights
    }

    /// Bioma en la columna `(x, z)` a la altitud `y`.
    #[must_use]
    pub fn biome_at(&self, x: f32, z: f32, y: f32) -> Biome {
        if self.none_enabled() {
            return Biome::Grassland;
        }
        if let Some(shares) = &self.shares {
            if Self::share_snow(shares, y) > 0.5 {
                return Biome::Snow;
            }
            if self.share_volcanic(shares, x, z) > 0.5 {
                return Biome::Volcanic;
            }
            let Some(regions) = &shares.regions else {
                return Biome::Snow;
            };
            let mut weights = self.region_weights(regions, x, z);
            self.keep_desert_away_from_water(&mut weights, x, z);
            return SHARE_BAND_ORDER
                .into_iter()
                .max_by(|a, b| weights[a.index()].total_cmp(&weights[b.index()]))
                .unwrap_or(Biome::Grassland);
        }

        if self.enabled[Biome::Snow.index()] && y > self.snow_line {
            return Biome::Snow;
        }

        let mut temp = (self.temperature.get2(x, z) * 0.5 + 0.5).clamp(0.0, 1.0);
        let moist = (self.moisture.get2(x, z) * 0.5 + 0.5 + 0.14).clamp(0.0, 1.0);
        let above = (y - self.plains_y).max(0.0);
        temp = (temp - above / 90.0).clamp(0.0, 1.0);

        if self.enabled[Biome::Volcanic.index()]
            && !self.mountains.is_empty()
            && self.mountain_norm(x, z) < 1.05
        {
            return Biome::Volcanic;
        } else if self.mountains.is_empty() {
            let volcanic = (self.volcanic.get2(x, z) * 0.5 + 0.5).clamp(0.0, 1.0);
            if self.enabled[Biome::Volcanic.index()] && volcanic > 0.92 && temp > 0.70 {
                return Biome::Volcanic;
            }
        }
        if self.enabled[Biome::Desert.index()]
            && temp > 0.78
            && moist < 0.28
            && !self.near_water(x, z)
        {
            return Biome::Desert;
        }
        if self.enabled[Biome::Forest.index()] && moist > 0.50 && (0.22..0.82).contains(&temp) {
            return Biome::Forest;
        }
        if self.enabled[Biome::Grassland.index()] {
            return Biome::Grassland;
        }
        self.first_enabled_ground()
    }

    /// Línea de nieve en metros.
    #[inline]
    #[must_use]
    pub fn snow_line(&self) -> f32 {
        self.snow_line
    }

    fn near_water(&self, x: f32, z: f32) -> bool {
        let point = Vec2::new(x, z);
        self.lakes
            .iter()
            .any(|lake| lake.norm_dist(x, z) < DESERT_LAKE_PAD)
            || self
                .rivers
                .iter()
                .any(|path| polyline_distance(path, point) < DESERT_RIVER_M)
    }

    fn keep_desert_away_from_water(&self, weights: &mut [f32; 5], x: f32, z: f32) {
        let desert = weights[Biome::Desert.index()];
        if desert <= 0.0 || !self.near_water(x, z) {
            return;
        }
        weights[Biome::Desert.index()] = 0.0;
        if self.enabled[Biome::Grassland.index()] {
            weights[Biome::Grassland.index()] += desert;
        } else if self.enabled[Biome::Forest.index()] {
            weights[Biome::Forest.index()] += desert;
        }
    }

    fn none_enabled(&self) -> bool {
        self.enabled.iter().all(|on| !*on)
    }

    fn first_enabled_ground(&self) -> Biome {
        for biome in Biome::ALL {
            if biome != Biome::Snow && self.enabled[biome.index()] {
                return biome;
            }
        }
        if self.enabled[Biome::Snow.index()] {
            Biome::Snow
        } else {
            Biome::Grassland
        }
    }
}

fn polyline_distance(path: &[Vec2], point: Vec2) -> f32 {
    if path.is_empty() {
        return f32::INFINITY;
    }
    if path.len() == 1 {
        return (path[0] - point).length();
    }
    let mut best = f32::MAX;
    for pair in path.windows(2) {
        let a = pair[0];
        let b = pair[1];
        let ab = b - a;
        let len2 = ab.length_squared();
        let t = if len2 <= f32::EPSILON {
            0.0
        } else {
            ((point - a).dot(ab) / len2).clamp(0.0, 1.0)
        };
        best = best.min((a + ab * t - point).length());
    }
    best
}
