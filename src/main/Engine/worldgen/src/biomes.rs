//! Mapa de biomas (F5): selección determinista por ruido de baja frecuencia
//! restringida a los biomas activados en la config, con override de nieve por
//! altitud.
//!
//! El bioma alimenta el splat (materiales de superficie); la mezcla por
//! altitud/pendiente se hace en `splat.rs`.

use crate::noise::{FractalType, Noise};
use crate::seed::domain_seed;

/// Biomas soportados. El orden es parte del contrato (bandas del ruido de
/// biomas); no reordenar sin romper mundos ya generados.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Biome {
    /// Pradera (hierba).
    Grassland,
    /// Bosque (hierba; los árboles llegan con el scatter de F7).
    Forest,
    /// Desierto (arena).
    Desert,
    /// Zona nevada (nieve).
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

/// Mapa de biomas del mundo.
#[derive(Debug, Clone)]
pub struct BiomeMap {
    noise: Noise,
    enabled: [bool; 5],
    /// Fracción de la altura del mundo a partir de la cual hay nieve.
    snow_line_fraction: f32,
    world_max_y: f32,
}

impl BiomeMap {
    /// Construye el mapa a partir de la config validada.
    #[must_use]
    pub fn new(config: &crate::config::WorldGenConfig) -> Self {
        let biomes = config.biomes;
        let seed = domain_seed(config.seed, crate::seed::SALT_BIOME);
        Self {
            noise: Noise::new(seed)
                .frequency(0.0025)
                .fractal(FractalType::Fbm, 3, 0.5, 0.0),
            enabled: [
                biomes.grassland,
                biomes.forest,
                biomes.desert,
                biomes.snow,
                biomes.volcanic,
            ],
            snow_line_fraction: 0.72,
            world_max_y: config.bounds.height,
        }
    }

    /// Bioma en la columna `(x, z)` a la altitud `y`.
    ///
    /// Bandas contiguas del ruido de baja frecuencia repartidas entre los
    /// biomas activados (fronteras orgánicas); la nieve por altitud tiene
    /// prioridad si está activada.
    #[must_use]
    pub fn biome_at(&self, x: f32, z: f32, y: f32) -> Biome {
        let active: Vec<Biome> = Biome::ALL
            .iter()
            .copied()
            .filter(|b| self.enabled[b.index()])
            .collect();
        if active.is_empty() {
            return Biome::Grassland;
        }

        let snow_line = self.world_max_y * self.snow_line_fraction;
        if y > snow_line && self.enabled[Biome::Snow.index()] {
            return Biome::Snow;
        }

        let noise01 = (self.noise.get2(x, z) * 0.5 + 0.5).clamp(0.0, 1.0);
        // Mapear [0,1] a bandas con fronteras suaves: cerca del borde de banda
        // se decide por la banda y el redondeo del propio ruido.
        let band = (noise01 * active.len() as f32) as usize;
        active[band.min(active.len() - 1)]
    }

    /// Línea de nieve en metros.
    #[inline]
    #[must_use]
    pub fn snow_line(&self) -> f32 {
        self.world_max_y * self.snow_line_fraction
    }
}
