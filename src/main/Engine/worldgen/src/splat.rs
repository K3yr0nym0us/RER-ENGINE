//! Splat mapping (F5): pesos de material por punto de superficie + albedo de
//! preview por chunk.
//!
//! - **Splat RGBA8** (R=hierba, G=roca, B=arena, A=nieve): pesos por texel que
//!   el shader de terreno del engine mezclará entre 4 capas del TextureArray
//!   cuando se implemente el modo splat. Son los datos canónicos de material.
//! - **Albedo de preview** (RGBA): color plano sintetizado de los pesos con la
//!   paleta, para que el engine lo suba hoy como capa de textura del chunk y el
//!   terreno se vea por biomas sin esperar el shader de splat.

use glam::Vec2;

use crate::biomes::{Biome, BiomeMap};
use crate::chunks::CHUNK_SIZE;
use crate::config::WorldGenConfig;
use crate::field::spatial::SiteIndex;
use crate::field::{DensityField, smoothstep};
use crate::planner::{FeatureKind, Site};

/// Resolución del splat por chunk (texels por lado, lod 0).
pub const SPLAT_DIM: u32 = 64;
/// Paso de muestreo de pendiente (metros).
const SLOPE_STEP: f32 = 2.0;

/// Pesos de material de superficie normalizados (suman 255).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplatWeights {
    /// Hierba (pradera/bosque).
    pub grass: u8,
    /// Roca (pendiente alta / montañas / volcánico).
    pub rock: u8,
    /// Arena (desierto / playas).
    pub sand: u8,
    /// Nieve (zona nevada / altitud).
    pub snow: u8,
}

impl SplatWeights {
    /// RGBA8 en orden (R, G, B, A).
    #[inline]
    #[must_use]
    pub const fn to_rgba(self) -> [u8; 4] {
        [self.grass, self.rock, self.sand, self.snow]
    }
}

/// Paleta de albedo de preview por material (RGB lineal 0..255).
const GRASS_RGB: [f32; 3] = [78.0, 132.0, 62.0];
const ROCK_RGB: [f32; 3] = [108.0, 102.0, 96.0];
const SAND_RGB: [f32; 3] = [196.0, 174.0, 116.0];
const SNOW_RGB: [f32; 3] = [236.0, 240.0, 244.0];

/// Contexto de splat: mapa de biomas + sitios (para orillas de lagos).
pub struct SplatContext<'a> {
    field: &'a DensityField,
    biomes: BiomeMap,
    sites: SiteIndex,
    /// Altura del mundo (para líneas de nieve/roca por altitud).
    world_max_y: f32,
    /// Tamaño de voxel del mundo (ancho real de chunk = CHUNK_SIZE * voxel).
    voxel_size: f32,
}

impl<'a> SplatContext<'a> {
    /// Construye el contexto a partir del campo ya generado.
    #[must_use]
    pub fn new(field: &'a DensityField, config: &'a WorldGenConfig, sites: Vec<Site>) -> Self {
        let biomes = BiomeMap::new(config);
        let world_max_y = config.bounds.height;
        Self {
            field,
            biomes,
            sites: SiteIndex::new(sites),
            world_max_y,
            voxel_size: config.resolution.voxel_size,
        }
    }

    /// Pesos de material en el punto de superficie `(x, z)` a la altitud `y`.
    #[must_use]
    pub fn weights_at(&self, x: f32, z: f32, y: f32) -> SplatWeights {
        // Pendiente por gradiente numérico de la altura con plan.
        let gx = (self.field.height_at(x + SLOPE_STEP, z)
            - self.field.height_at(x - SLOPE_STEP, z))
            / (2.0 * SLOPE_STEP);
        let gz = (self.field.height_at(x, z + SLOPE_STEP)
            - self.field.height_at(x, z - SLOPE_STEP))
            / (2.0 * SLOPE_STEP);
        let slope_deg = gx.hypot(gz).atan().to_degrees();

        // Roca: pendiente alta o altitud de alta montaña.
        let rock_alt = smoothstep(self.world_max_y * 0.60, self.world_max_y * 0.72, y);
        let mut rock = crate::field::smoothstep(24.0, 42.0, slope_deg).max(rock_alt);

        // Nieve: bioma nevado o altitud sobre la línea de nieve (transición 6 m).
        let snow_line = self.biomes.snow_line();
        let snow_alt = smoothstep(snow_line - 3.0, snow_line + 3.0, y);
        let biome = self.biomes.biome_at(x, z, y);
        let mut snow: f32 = if biome == Biome::Snow { 1.0 } else { 0.0 };
        snow = snow.max(snow_alt);

        // Arena: desierto; playas cerca del nivel del agua en lagos cercanos.
        let mut sand: f32 = if biome == Biome::Desert { 1.0 } else { 0.0 };
        for site in self.sites.query(x, z) {
            if site.kind == FeatureKind::Lake {
                let d = (site.pos - Vec2::new(x, z)).length();
                // Orilla: banda 1.5 m junto al borde del vaso.
                let shore = smoothstep(site.radius + 1.5, site.radius - 1.5, d);
                sand = sand.max(shore);
            }
        }
        // Volcánico: roca oscura en volcán (lo tratamos como roca + algo de arena).
        if biome == Biome::Volcanic {
            rock = rock.max(0.75);
            sand = sand.max(0.25);
        }
        // Nieve no pisa roca de pendiente alta; la roca no pisa arena.
        snow *= 1.0 - rock * 0.85;
        sand *= 1.0 - rock;

        let mut grass = (1.0 - rock - snow - sand).max(0.0);
        // Normalizar a 255.
        let total = rock + snow + sand + grass;
        if total <= 0.0 {
            grass = 1.0;
        }
        let to_u8 = |v: f32| (v * 255.0 / total.max(1.0)).round().clamp(0.0, 255.0) as u8;
        SplatWeights {
            grass: to_u8(grass),
            rock: to_u8(rock),
            sand: to_u8(sand),
            snow: to_u8(snow),
        }
    }

    /// Albedo de preview (RGB) de los pesos.
    #[must_use]
    pub fn albedo_from_weights(w: SplatWeights) -> [u8; 3] {
        let f = |v: u8| v as f32 / 255.0;
        let (wg, wr, ws, wn) = (f(w.grass), f(w.rock), f(w.sand), f(w.snow));
        let mut rgb = [0.0f32; 3];
        for i in 0..3 {
            rgb[i] = wg * GRASS_RGB[i] + wr * ROCK_RGB[i] + ws * SAND_RGB[i] + wn * SNOW_RGB[i];
        }
        [
            rgb[0].round() as u8,
            rgb[1].round() as u8,
            rgb[2].round() as u8,
        ]
    }
}

/// Genera el splat y el albedo de un chunk (columna XZ; el splat es por columna
/// de superficie, compartido por todos los chunks Y de la misma).
///
/// Devuelve `(splat_rgba, albedo_rgba, dim)`: dos buffers de `dim² * 4` bytes,
/// fila 0 = borde z mínimo, x más rápido (convención de `upload_rgba_layer`).
#[must_use]
pub fn chunk_splat(
    field: &DensityField,
    ctx: &SplatContext<'_>,
    chunk_x: i32,
    chunk_z: i32,
    lod: u32,
) -> (Vec<u8>, Vec<u8>, u32) {
    let dim = SPLAT_DIM.checked_shr(lod.min(3)).unwrap_or(8).max(8);
    let cell = CHUNK_SIZE as f32 * ctx.voxel_size;
    let origin_x = chunk_x as f32 * cell;
    let origin_z = chunk_z as f32 * cell;
    let step = cell / dim as f32;

    let mut splat = Vec::with_capacity((dim * dim * 4) as usize);
    let mut albedo = Vec::with_capacity((dim * dim * 4) as usize);

    for tz in 0..dim {
        let z = origin_z + (tz as f32 + 0.5) * step;
        for tx in 0..dim {
            let x = origin_x + (tx as f32 + 0.5) * step;
            let y = field.height_at(x, z);
            let w = ctx.weights_at(x, z, y);
            splat.extend_from_slice(&w.to_rgba());
            let rgb = SplatContext::albedo_from_weights(w);
            albedo.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    (splat, albedo, dim)
}
