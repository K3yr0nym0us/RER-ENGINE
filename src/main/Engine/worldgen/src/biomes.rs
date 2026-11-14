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

use crate::config::plains_height;
use crate::field::smoothstep;
use crate::noise::{FractalType, Noise};
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

/// Orden de las bandas de suelo por porcentaje (cálido → frío). La nieve no es
/// una banda: ocupa las cotas más altas.
const SHARE_BAND_ORDER: [Biome; 4] = [
    Biome::Volcanic,
    Biome::Desert,
    Biome::Grassland,
    Biome::Forest,
];
/// Muestras por eje para medir el cuantil de altura de la nieve en el disco.
const SHARE_SAMPLES: u32 = 96;
/// Amplitud (celdas de rejilla) del desplazamiento que curva las fronteras.
const SHARE_WARP_CELLS: f32 = 1.5;
/// Frecuencia del desplazamiento relativa al campo de regiones.
const SHARE_WARP_SCALE: f32 = 6.1;
/// Media anchura de la transición vertical de la nieve (metros).
const SNOW_BLEND_M: f32 = 1.5;
/// La nieve nunca baja de esta altura sobre las llanuras (metros).
const SNOW_MIN_RISE_M: f32 = 4.0;

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
            // Pocos ciclos y dos octavas suaves: regiones amplias y continuas,
            // sin islotes de un bioma dentro de otro.
            regions: Noise::new(domain_seed(config.seed, crate::seed::SALT_BIOME_REGIONS))
                .frequency((1.3 + radius / 400.0).clamp(1.3, 3.2) / diameter)
                .fractal(FractalType::Fbm, 2, 0.3, 0.0),
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
        map.fit_snow_to_terrain(|x, z| field.height_at(x, z));
        map
    }

    /// Fija la cota de nieve en el cuantil de altura que deja `snow_fraction`
    /// del disco por encima, sin bajar de las llanuras + [`SNOW_MIN_RISE_M`].
    pub fn fit_snow_to_terrain(&mut self, height_at: impl Fn(f32, f32) -> f32) {
        let floor = self.plains_y + SNOW_MIN_RISE_M;
        let Some(fraction) = self.shares.as_ref().map(|s| s.snow_fraction) else {
            return;
        };
        if fraction <= 0.0 {
            return;
        }
        let mut heights = self.disk_samples(height_at);
        heights.sort_by(f32::total_cmp);
        let Some(last) = heights.len().checked_sub(1) else {
            return;
        };
        let rank = ((1.0 - fraction) * last as f32).round() as usize;
        let snow_y = heights[rank.min(last)].max(floor);
        if let Some(shares) = self.shares.as_mut() {
            shares.snow_y = snow_y;
        }
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
        let total = ground_total + snow;
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
                    values.push(self.regions.get2(x, z));
                    inside.push((x - cx).hypot(z - cz) <= self.radius);
                }
            }
            RegionGrid::build(&values, &inside, &active)
        };
        Some(ShareBands {
            regions,
            snow_fraction: snow / total,
            snow_y: self.snow_line,
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
        weights[Biome::Snow.index()] = snow;
        let ground = 1.0 - snow;
        let Some(regions) = shares.regions.as_ref().filter(|_| ground > 0.0) else {
            return weights;
        };
        let region = self.region_weights(regions, x, z);
        for (biome, (slot, value)) in weights.iter_mut().zip(region).enumerate() {
            if biome != Biome::Snow.index() {
                *slot = value * ground;
            }
        }
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
            let Some(regions) = &shares.regions else {
                return Biome::Snow;
            };
            let weights = self.region_weights(regions, x, z);
            return SHARE_BAND_ORDER
                .into_iter()
                .max_by(|a, b| weights[a.index()].total_cmp(&weights[b.index()]))
                .unwrap_or(Biome::Grassland);
        }

        if self.enabled[Biome::Snow.index()] && y > self.snow_line {
            return Biome::Snow;
        }

        let mut temp = (self.temperature.get2(x, z) * 0.5 + 0.5).clamp(0.0, 1.0);
        // Sesgo húmedo: la pradera/bosque cubren el suelo; el desierto es raro.
        let moist = (self.moisture.get2(x, z) * 0.5 + 0.5 + 0.14).clamp(0.0, 1.0);
        let above = (y - self.plains_y).max(0.0);
        temp = (temp - above / 90.0).clamp(0.0, 1.0);

        let volcanic = (self.volcanic.get2(x, z) * 0.5 + 0.5).clamp(0.0, 1.0);
        if self.enabled[Biome::Volcanic.index()] && volcanic > 0.92 && temp > 0.70 {
            return Biome::Volcanic;
        }
        if self.enabled[Biome::Desert.index()] && temp > 0.78 && moist < 0.28 {
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
