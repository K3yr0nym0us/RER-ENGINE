//! Campo de densidad del mundo: heightfield base + features planificados +
//! carving (cuevas, ríos, lagos) + formaciones rocosas.
//!
//! Convención heredada de la referencia: **densidad positiva = sólido**,
//! `density(x, y, z) = height(x, z) - y` (con carving reductivo/aditivo). La
//! densidad es una **función pura del espacio de mundo** dado el plan, por lo
//! que los chunks comparten puntos exactos y las regeneraciones son fiables.

pub(crate) mod spatial;

use glam::{Vec2, Vec3};

use crate::config::{WorldGenConfig, plains_height};
use crate::field::spatial::{CarveIndex, SiteIndex};
use crate::noise::{FractalType, Noise};
use crate::planner::FeaturePlan;
use crate::scale::{land_grade_amplitude, world_radius};
use crate::seed::{
    SALT_CAVE_NOISE, SALT_CLIFF, SALT_DETAIL, SALT_MOUNTAIN, SALT_TERRAIN, domain_seed, mix_u32,
};

/// Factor de amortiguación de features cerca de los límites del mundo.
const EDGE_FALLBACK_FRACTION: f32 = 0.12;

/// Eje cardinal de la pendiente del disco (nunca diagonal: no “tuerce” el mundo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandAxis {
    /// Pendiente a lo largo de X.
    X,
    /// Pendiente a lo largo de Z.
    Z,
}

/// Pendiente suave y determinista: una mitad del disco queda un poco más alta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandGrade {
    /// Eje de la rampa.
    axis: LandAxis,
    /// Si es true, la coordenada mayor es el lado alto.
    high_positive: bool,
    /// Desnivel total de lado a lado (metros).
    amplitude: f32,
    width: f32,
    length: f32,
    /// Esquina mínima del mundo visible dentro del área generada.
    offset: Vec2,
}

impl LandGrade {
    /// Deriva eje, sentido y amplitud del seed. Pendiente suave; el desnivel
    /// absoluto crece con el tamaño del mundo (mínimo en mundos pequeños,
    /// máximo en los grandes).
    #[must_use]
    pub fn from_config(config: &WorldGenConfig) -> Self {
        let bits = mix_u32(config.seed ^ 0x51ed_5106);
        let radius = world_radius(&config.bounds).max(32.0);
        let width = if config.bounds.visible_width > 0.0 {
            config.bounds.visible_width
        } else {
            config.bounds.width
        };
        let length = if config.bounds.visible_length > 0.0 {
            config.bounds.visible_length
        } else {
            config.bounds.length
        };
        Self {
            axis: if bits & 1 == 0 {
                LandAxis::X
            } else {
                LandAxis::Z
            },
            high_positive: (bits >> 1) & 1 == 1,
            amplitude: land_grade_amplitude(radius),
            width,
            length,
            offset: Vec2::new(
                config.bounds.visible_offset_x,
                config.bounds.visible_offset_z,
            ),
        }
    }

    /// `0` en el borde bajo, `1` en el borde alto (relativo al mundo visible).
    #[must_use]
    pub fn along(self, pos: Vec2) -> f32 {
        let (value, span) = match self.axis {
            LandAxis::X => (pos.x - self.offset.x, self.width),
            LandAxis::Z => (pos.y - self.offset.y, self.length),
        };
        let t = if span > 1.0 {
            (value / span).clamp(0.0, 1.0)
        } else {
            0.5
        };
        if self.high_positive { t } else { 1.0 - t }
    }

    /// Metros a sumar al heightfield (`0` en el borde bajo, `amplitude` en el alto).
    #[must_use]
    pub fn lift(self, x: f32, z: f32) -> f32 {
        self.along(Vec2::new(x, z)) * self.amplitude
    }

    /// Desnivel de lado a lado, en metros.
    #[inline]
    #[must_use]
    pub fn amplitude(self) -> f32 {
        self.amplitude
    }

    /// Mitad alta del disco (incluye el ecuador).
    #[must_use]
    pub fn is_high_half(self, pos: Vec2) -> bool {
        self.along(pos) >= 0.5
    }

    /// Mitad baja del disco (incluye el ecuador).
    #[must_use]
    pub fn is_low_half(self, pos: Vec2) -> bool {
        self.along(pos) <= 0.5
    }

    /// Dirección XZ hacia el lado alto (cardinal).
    #[must_use]
    pub fn high_dir(self) -> Vec2 {
        match (self.axis, self.high_positive) {
            (LandAxis::X, true) => Vec2::X,
            (LandAxis::X, false) => -Vec2::X,
            (LandAxis::Z, true) => Vec2::Y,
            (LandAxis::Z, false) => -Vec2::Y,
        }
    }

    /// Dirección XZ hacia el lado bajo (cardinal).
    #[must_use]
    pub fn low_dir(self) -> Vec2 {
        -self.high_dir()
    }

    /// Ángulo del lado alto, para muestrear solo ese semicírculo.
    #[must_use]
    pub fn high_heading(self) -> f32 {
        self.high_dir().y.atan2(self.high_dir().x)
    }

    /// Punto interior del lado alto, para tests y muestreo.
    #[must_use]
    pub fn sample_high(self) -> Vec2 {
        self.center() + self.high_dir() * self.probe_reach()
    }

    /// Punto interior del lado bajo.
    #[must_use]
    pub fn sample_low(self) -> Vec2 {
        self.center() - self.high_dir() * self.probe_reach()
    }

    fn center(self) -> Vec2 {
        self.offset + Vec2::new(self.width, self.length) * 0.5
    }

    fn probe_reach(self) -> f32 {
        self.width.min(self.length) * 0.28
    }

    /// Refleja un punto hacia la mitad pedida.
    #[must_use]
    pub fn mirror_to_half(self, mut pos: Vec2, high: bool) -> Vec2 {
        if self.is_high_half(pos) == high {
            return pos;
        }
        match self.axis {
            LandAxis::X => pos.x = self.offset.x * 2.0 + self.width - pos.x,
            LandAxis::Z => pos.y = self.offset.y * 2.0 + self.length - pos.y,
        }
        pos
    }
}

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
    /// Relieve de larga escala: unas zonas del disco más altas que otras.
    macro_amplitude: f32,
    detail_amplitude: f32,
    mountain_sharpness: f32,
    land_grade: LandGrade,
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

        let radius = world_radius(&config.bounds).max(32.0);
        Self {
            base_height: plains_height(),
            // FBM global de baja amplitud: relieve por defecto del mundo.
            height_noise: Noise::new(terrain_seed).frequency(0.004).fractal(
                FractalType::Fbm,
                3,
                0.28,
                0.0,
            ),
            detail_noise: Noise::new(detail_seed).frequency(0.035),
            mountain_noise: Noise::new(mountain_seed)
                .frequency((1.8 / radius).clamp(0.002, 0.012))
                .fractal(FractalType::Ridged, 2, 0.45, 0.0),
            cliff_noise: Noise::new(cliff_seed).frequency(0.02),
            hills_amplitude: if config.hills.base.enabled { 4.0 } else { 1.4 },
            macro_amplitude: (radius * 0.018).clamp(2.5, 6.0),
            detail_amplitude: 0.12,
            mountain_sharpness: config.mountains.sharpness.max(0.001),
            land_grade: LandGrade::from_config(config),
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
        self.raw_base(x, z)
            .clamp(crate::config::TERRAIN_FLOOR, self.world_max.y - 2.0)
    }

    /// Pendiente cardinal del disco (mitad alta / mitad baja).
    #[inline]
    #[must_use]
    pub fn land_grade(&self) -> LandGrade {
        self.land_grade
    }

    /// Techo del volumen (el fit puede subir `bounds.height` tras construir el campo).
    #[must_use]
    pub(crate) fn with_ceiling(&self, height: f32) -> Self {
        let mut cloned = self.clone();
        cloned.world_max.y = cloned.world_max.y.max(height);
        cloned
    }

    fn raw_base(&self, x: f32, z: f32) -> f32 {
        self.base_level()
            + self.land_grade.lift(x, z)
            + self.macro_value(x, z) * self.macro_amplitude
            + self.hill_fbm_value(x, z) * self.hills_amplitude()
            + self.detail_value(x, z) * self.detail_amplitude()
    }

    /// FBM de muy baja frecuencia: mesetas y depresiones amplias.
    #[inline]
    pub(crate) fn macro_value(&self, x: f32, z: f32) -> f32 {
        self.height_noise.get2(x * 0.22, z * 0.22)
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
        let mut h = self.base.raw_base(x, z);
        let fallback = self.base.edge_fallback(x, z);
        let mut high_relief = f32::NEG_INFINITY;

        for site in self.sites.query(x, z) {
            let t = site.norm_dist(x, z);
            if t >= 1.0 {
                continue;
            }
            let w = 1.0 - smoothstep(0.45, 1.0, t);
            match site.kind {
                crate::planner::FeatureKind::Mountain => {
                    let pad = self.base.raw_base(site.pos.x, site.pos.y);
                    high_relief =
                        high_relief.max(pad + mountain_rise(&self.base, site, x, z, t, fallback));
                }
                crate::planner::FeatureKind::Hill => {
                    let pad = self.base.raw_base(site.pos.x, site.pos.y);
                    let bump = self.base.hill_pattern(x, z);
                    high_relief = high_relief.max(pad + fallback * w * site.value * bump);
                }
                crate::planner::FeatureKind::Valley => {
                    let bowl = (1.0 - t).max(0.0);
                    h -= site.value * bowl * bowl;
                }
                crate::planner::FeatureKind::Plain => {
                    let target = self.base.base_level()
                        + self.base.land_grade.lift(x, z)
                        + detail * site.value;
                    h = h + (target - h) * w;
                }
                crate::planner::FeatureKind::Cliff => {
                    let step = site.value.max(1.0);
                    let quantized = (h / step).floor() * step;
                    let mix = w * (0.55 + 0.35 * self.base.cliff_pattern(x, z));
                    h = h + (quantized - h) * mix;
                }
                crate::planner::FeatureKind::Lake => {}
            }
        }

        h = h.max(high_relief);
        for site in self.sites.query(x, z) {
            if site.kind != crate::planner::FeatureKind::Lake {
                continue;
            }
            let t = site.norm_dist(x, z);
            if t >= 1.0 {
                continue;
            }
            let basin = 1.0 - smoothstep(0.36, 0.97, t);
            let ripples = 0.90 + 0.10 * self.base.hill_pattern(x, z);
            h -= site.value * basin * ripples;
        }

        h.clamp(crate::config::TERRAIN_FLOOR, self.base.world_max_y() - 2.0)
    }
}

/// Relieve de una montaña sobre la cota de **su centro** (la parte alta local).
fn mountain_rise(
    base: &HeightField,
    site: &crate::planner::Site,
    x: f32,
    z: f32,
    t: f32,
    fallback: f32,
) -> f32 {
    let ridge = site.ridge.clamp(0.0, 1.0);
    let mesa = 1.0 - smoothstep(0.38, 0.98, t);
    let spike = (1.0 - t).max(0.0).powf(1.35 + 1.7 * ridge);
    let envelope = mesa * (1.0 - ridge) + spike * ridge;
    let mass = smoothstep(0.18, 0.88, base.hill_pattern(x, z));
    let foothills = smoothstep(0.18, 0.88, base.macro_value(x, z) * 0.5 + 0.5);
    let ridges = base.mountain_pattern(x, z);
    let shape =
        0.78 + mass * 0.10 * (1.0 - ridge) + foothills * 0.04 + ridges * (0.04 + 0.14 * ridge);
    fallback * envelope * shape * site.value
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
        let mut fitted = config.clone();
        crate::scale::fit_config_to_world(&mut fitted);
        let config = &fitted;
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

    /// Índice espacial de sitios ya construido con el plan (splat y scatter).
    #[must_use]
    pub(crate) fn sites(&self) -> &SiteIndex {
        &self.terrain.sites
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
