//! Mapa de biomas: clasificación Whittaker (temperatura + humedad) con nieve
//! solo por altitud.
//!
//! Antes el ruido 2D se partía en bandas iguales: con los 5 biomas activos el
//! 20 % del disco era nieve a cualquier cota y otro 20 % desierto. Ahora la
//! pradera es el suelo, el bosque sale de humedad, el desierto de calor seco,
//! el volcán de un hotspot raro y la nieve de la línea de cumbres.

use crate::config::plains_height;
use crate::noise::{FractalType, Noise};
use crate::scale::{snow_line_meters, world_radius};
use crate::seed::domain_seed;

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

/// Mapa de biomas del mundo.
#[derive(Debug, Clone)]
pub struct BiomeMap {
    temperature: Noise,
    moisture: Noise,
    volcanic: Noise,
    enabled: [bool; 5],
    snow_line: f32,
    plains_y: f32,
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
        Self {
            temperature: Noise::new(domain_seed(config.seed, crate::seed::SALT_BIOME))
                .frequency(temp_freq)
                .fractal(FractalType::Fbm, 3, 0.5, 0.0),
            moisture: Noise::new(domain_seed(config.seed, crate::seed::SALT_MOISTURE))
                .frequency(moist_freq)
                .fractal(FractalType::Fbm, 3, 0.55, 0.0),
            volcanic: Noise::new(domain_seed(config.seed, crate::seed::SALT_VOLCANIC))
                .frequency((cycles * 1.8) / diameter)
                .fractal(FractalType::Ridged, 2, 0.5, 0.0),
            enabled: [
                biomes.grassland,
                biomes.forest,
                biomes.desert,
                biomes.snow,
                biomes.volcanic,
            ],
            snow_line: snow_line_meters(config),
            plains_y: plains_height(),
        }
    }

    /// Bioma en la columna `(x, z)` a la altitud `y`.
    #[must_use]
    pub fn biome_at(&self, x: f32, z: f32, y: f32) -> Biome {
        if self.none_enabled() {
            return Biome::Grassland;
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
