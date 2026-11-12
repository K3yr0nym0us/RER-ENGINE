//! Campo de densidad del mundo: heightfield base + features planificados +
//! carving (cuevas, ríos, lagos) + formaciones rocosas.
//!
//! Convención heredada de la referencia: **densidad positiva = sólido**,
//! `density(x, y, z) = height(x, z) - y` (con carving reductivo/aditivo). La
//! densidad es una **función pura del espacio de mundo** dado el plan, por lo
//! que los chunks comparten puntos exactos y las regeneraciones son fiables.

pub(crate) mod spatial;

use glam::Vec3;

use crate::config::WorldGenConfig;
use crate::field::spatial::{CarveIndex, SiteIndex};
use crate::noise::{FractalType, Noise};
use crate::planner::FeaturePlan;
use crate::seed::{
    SALT_CAVE_NOISE, SALT_CLIFF, SALT_DETAIL, SALT_MOUNTAIN, SALT_TERRAIN, domain_seed,
};

/// Factor de amortiguación de features cerca de los límites del mundo.
const EDGE_FALLBACK_FRACTION: f32 = 0.12;

/// Heightfield **base** (sin plan): FBM de colinas global + detalle.
///
/// La usa el planificador (ríos descendentes, niveles de agua, bocas de cueva)
/// y el [`Terrain`] como sustrato sobre el que se aplican los sitios.
#[derive(Debug, Clone)]
pub struct HeightField {
    base_height: f32,
    height_noise: Noise,
    detail_noise: Noise,
    mountain_noise: Noise,
    cliff_noise: Noise,
    hills_amplitude: f32,
    detail_amplitude: f32,
    mountain_sharpness: f32,
    world_max: Vec3,
}

impl HeightField {
    /// Construye la heightfield base a partir de la configuración validada.
    #[must_use]
    pub fn new(config: &WorldGenConfig) -> Self {
        let terrain_seed = domain_seed(config.seed, SALT_TERRAIN);
        let detail_seed = domain_seed(config.seed, SALT_DETAIL);
        let mountain_seed = domain_seed(config.seed, SALT_MOUNTAIN);
        let cliff_seed = domain_seed(config.seed, SALT_CLIFF);

        Self {
            base_height: config.bounds.height * 0.35,
            // FBM global de baja amplitud: relieve por defecto del mundo.
            height_noise: Noise::new(terrain_seed).frequency(0.006).fractal(
                FractalType::Fbm,
                4,
                0.35,
                0.0,
            ),
            detail_noise: Noise::new(detail_seed).frequency(0.06),
            // Ridged fractal (ya normalizado ~[0,1]): patrón de crestas.
            mountain_noise: Noise::new(mountain_seed).frequency(0.004).fractal(
                FractalType::Ridged,
                4,
                0.5,
                0.0,
            ),
            cliff_noise: Noise::new(cliff_seed).frequency(0.02),
            hills_amplitude: if config.hills.base.enabled { 6.0 } else { 2.0 },
            detail_amplitude: 0.35,
            mountain_sharpness: config.mountains.sharpness.max(0.001),
            world_max: Vec3::new(
                config.bounds.width,
                config.bounds.height,
                config.bounds.length,
            ),
        }
    }

    /// Altura base del terreno en `(x, z)` (metros, sin features planificados).
    #[must_use]
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let h = self.base_level()
            + self.hill_fbm_value(x, z) * self.hills_amplitude()
            + self.detail_value(x, z) * self.detail_amplitude();
        h.clamp(2.0, self.world_max.y - 2.0)
    }

    /// FBM global de colinas (relieve por defecto del mundo).
    #[inline]
    pub(crate) fn hill_fbm_value(&self, x: f32, z: f32) -> f32 {
        self.height_noise.get2(x, z)
    }

    /// Amplitud del FBM global de colinas.
    #[inline]
    pub(crate) fn hills_amplitude(&self) -> f32 {
        self.hills_amplitude
    }

    /// Ruido de detalle de alta frecuencia.
    #[inline]
    pub(crate) fn detail_value(&self, x: f32, z: f32) -> f32 {
        self.detail_noise.get2(x, z)
    }

    /// Amplitud del detalle.
    #[inline]
    pub(crate) fn detail_amplitude(&self) -> f32 {
        self.detail_amplitude
    }

    /// Factor `[0, 1]` que atenúa el relieve cerca de los bordes XZ del mundo.
    #[inline]
    #[must_use]
    pub fn edge_fallback(&self, x: f32, z: f32) -> f32 {
        let span = EDGE_FALLBACK_FRACTION;
        let fx = smoothstep(0.0, self.world_max.x * span, x)
            * smoothstep(0.0, self.world_max.x * span, self.world_max.x - x);
        let fz = smoothstep(0.0, self.world_max.z * span, z)
            * smoothstep(0.0, self.world_max.z * span, self.world_max.z - z);
        (fx * fz).max(0.0)
    }

    /// Altura base sin detalle (para el aplanado de llanuras).
    #[inline]
    pub(crate) fn base_level(&self) -> f32 {
        self.base_height
    }

    /// Ruido de crestas para montañas planificadas (`~[0, 1]`).
    #[inline]
    pub(crate) fn mountain_pattern(&self, x: f32, z: f32) -> f32 {
        let ridge = self.mountain_noise.get2(x, z).clamp(0.0, 1.0);
        ridge.powf(self.mountain_sharpness)
    }

    /// Variación local para colinas planificadas (muestreo de dominio desplazado).
    #[inline]
    pub(crate) fn hill_pattern(&self, x: f32, z: f32) -> f32 {
        self.height_noise.get2(x * 2.7 + 31.0, z * 2.7 - 17.0) * 0.5 + 0.5
    }

    /// Ruido de terrazas para acantilados planificados (`~[0, 1]`).
    #[inline]
    pub(crate) fn cliff_pattern(&self, x: f32, z: f32) -> f32 {
        self.cliff_noise.get2(x, z) * 0.5 + 0.5
    }

    /// Altura máxima del mundo (metros).
    #[inline]
    pub(crate) fn world_max_y(&self) -> f32 {
        self.world_max.y
    }
}

/// Terreno con plan aplicado: heightfield base + sitios espacialmente indexados.
#[derive(Debug)]
pub(crate) struct Terrain {
    base: HeightField,
    sites: SiteIndex,
}

impl Terrain {
    /// Construye el terreno a partir de la heightfield base y los sitios.
    pub(crate) fn new(base: HeightField, sites: Vec<crate::planner::Site>) -> Self {
        Self {
            base,
            sites: SiteIndex::new(sites),
        }
    }

    /// Altura del terreno con features planificados en `(x, z)`.
    #[must_use]
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let detail = self.base.detail_value(x, z) * self.base.detail_amplitude();
        let mut h = self.base.base_level()
            + self.base.hill_fbm_value(x, z) * self.base.hills_amplitude()
            + detail;
        let fallback = self.base.edge_fallback(x, z);

        for site in self.sites.query(x, z) {
            let d = (site.pos.x - x).hypot(site.pos.y - z);
            if d >= site.radius {
                continue;
            }
            // Peso radial suave hacia el centro del sitio.
            let w = 1.0 - smoothstep(site.radius * 0.45, site.radius, d);
            match site.kind {
                crate::planner::FeatureKind::Mountain => {
                    // Crestas ridged escaladas por la altura del sitio.
                    let ridge = self.base.mountain_pattern(x, z);
                    h += fallback * w * ridge * site.value;
                }
                crate::planner::FeatureKind::Hill => {
                    let bump = self.base.hill_pattern(x, z);
                    h += fallback * w * site.value * bump;
                }
                crate::planner::FeatureKind::Valley => {
                    // Cuenca suave: w² da bordes tendidos y fondo plano.
                    h -= site.value * w * w;
                }
                crate::planner::FeatureKind::Plain => {
                    // Aplanado: mezcla hacia base + microdetalle acotado.
                    let target = self.base.base_level() + detail * site.value;
                    h = h + (target - h) * w;
                }
                crate::planner::FeatureKind::Cliff => {
                    // Terrazas: cuantización de altura con suavizado local.
                    let step = site.value.max(1.0);
                    let quantized = (h / step).floor() * step;
                    let mix = w * (0.55 + 0.35 * self.base.cliff_pattern(x, z));
                    h = h + (quantized - h) * mix;
                }
                crate::planner::FeatureKind::Lake => {
                    // Vaso paraboloidal de la cuenca.
                    let t = 1.0 - (d / site.radius) * (d / site.radius);
                    h -= site.value * t.max(0.0);
                }
            }
        }

        h.clamp(2.0, self.base.world_max_y() - 2.0)
    }
}

/// Campo de densidad completo del mundo (terreno con plan + carving).
#[derive(Debug)]
pub struct DensityField {
    terrain: Terrain,
    carve: CarveIndex,
    /// Plan del que deriva el terreno (para splat/scatter/persistencia).
    plan: FeaturePlan,
    world_max_y: f32,
}

impl DensityField {
    /// Construye el campo a partir de la configuración validada.
    ///
    /// Genera el [`FeaturePlan`] (determinista) y los índices espaciales de
    /// sitios y carving.
    #[must_use]
    pub fn new(config: &WorldGenConfig) -> Self {
        let base = HeightField::new(config);
        let plan = FeaturePlan::generate(config, &base);
        let terrain = Terrain::new(base, plan.sites.clone());
        let cave_noise = Noise::new(domain_seed(config.seed, SALT_CAVE_NOISE))
            .frequency(0.045)
            .fractal(FractalType::Fbm, 3, 0.5, 0.0);
        let carve = CarveIndex::from_plan(&plan, &terrain, cave_noise);
        let world_max_y = config.bounds.height;
        Self {
            terrain,
            carve,
            plan,
            world_max_y,
        }
    }

    /// Plan de features del mundo (sitios, cuevas, ríos, lagos, cascadas).
    #[must_use]
    pub fn plan(&self) -> &FeaturePlan {
        &self.plan
    }

    /// Densidad en el punto de mundo (positiva = sólido).
    #[inline]
    #[must_use]
    pub fn density_at(&self, x: f32, y: f32, z: f32) -> f32 {
        if y > self.world_max_y {
            return -1.0;
        }
        let surface = self.terrain.height_at(x, z);
        let mut d = surface - y;
        // Carving reductivo (máximo de influencias) y aditivo (roca).
        d -= self.carve.carve_at(x, y, z, surface);
        d += self.carve.add_at(x, y, z);
        d
    }

    /// Altura del terreno con features planificados (para pruning de chunks y
    /// para el scatter de blueprints en F7).
    #[must_use]
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        self.terrain.height_at(x, z)
    }

    /// Acceso a la heightfield base (solo para tests y utilidades).
    #[must_use]
    pub fn base_heightfield(&self) -> &HeightField {
        &self.terrain.base
    }
}

/// Función de altura **con plan** para el planificador (bocas de cueva, cauces
/// de río y niveles de agua quedan pegados a la superficie real con features).
pub(crate) fn plan_height_fn(
    sites: Vec<crate::planner::Site>,
    base: &HeightField,
) -> impl Fn(f32, f32) -> f32 {
    let terrain = Terrain::new(base.clone(), sites);
    move |x, z| terrain.height_at(x, z)
}

/// `smoothstep(e0, e1, x)` estándar.
#[inline]
#[must_use]
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
