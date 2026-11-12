//! Configuración del generador de mundos: dimensiones, resolución, features con
//! cantidades máximas y parámetros por elemento.
//!
//! El esquema usa camelCase en serde para alinearse 1:1 con el catálogo de comandos
//! del editor (`engineCommandCatalog.ts`) y con la sección `worldGen3d` del manifest.

use serde::{Deserialize, Serialize};

/// Límites de dimensiones del mundo (metros). Son límites duros: el planificador y
/// el campo de densidad clampan todo a estos valores.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorldBounds {
    /// Ancho máximo del mundo en el eje X (metros).
    pub width: f32,
    /// Largo máximo del mundo en el eje Z (metros).
    pub length: f32,
    /// Altura máxima del mundo en el eje Y (metros).
    pub height: f32,
}

impl Default for WorldBounds {
    fn default() -> Self {
        Self {
            width: 512.0,
            length: 512.0,
            height: 128.0,
        }
    }
}

impl WorldBounds {
    /// Valida los invariantes de dimensiones.
    pub fn validate(&self) -> Result<(), ConfigError> {
        const MIN_DIM: f32 = 32.0;
        const MAX_DIM: f32 = 8192.0;
        for (name, value) in [
            ("width", self.width),
            ("length", self.length),
            ("height", self.height),
        ] {
            if !value.is_finite() || !(MIN_DIM..=MAX_DIM).contains(&value) {
                return Err(ConfigError::InvalidField {
                    field: format!("bounds.{name}"),
                    reason: format!("debe estar entre {MIN_DIM} y {MAX_DIM} metros"),
                });
            }
        }
        Ok(())
    }
}

/// Resolución de generación geométrica.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Resolution {
    /// Tamaño de voxel en metros. Valores menores = más detalle = más coste.
    pub voxel_size: f32,
    /// LOD por anillos de distancia (implementado en fases posteriores).
    pub lod_enabled: bool,
}

impl Default for Resolution {
    fn default() -> Self {
        Self {
            voxel_size: 1.0,
            lod_enabled: false,
        }
    }
}

/// Config base compartida por features geográficos localizables.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FeatureConfig {
    /// Activación individual de la característica ("Agregar").
    pub enabled: bool,
    /// Cantidad máxima de apariciones (límite duro del planificador).
    pub max_count: u32,
    /// Tamaño mínimo (radio o longitud mínima, metros, según feature).
    pub min_size: f32,
    /// Tamaño máximo.
    pub max_size: f32,
    /// Separación mínima entre instancias de la misma feature (metros).
    pub min_separation: f32,
}

impl FeatureConfig {
    /// Valores razonables por defecto, deshabilitado.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            max_count: 10,
            min_size: 20.0,
            max_size: 60.0,
            min_separation: 40.0,
        }
    }

    fn validate(&self, field: &str) -> Result<(), ConfigError> {
        if self.max_count > MAX_FEATURE_COUNT {
            return Err(ConfigError::InvalidField {
                field: format!("{field}.maxCount"),
                reason: format!("no puede superar {MAX_FEATURE_COUNT}"),
            });
        }
        if self.min_size < 0.0 || self.max_size <= 0.0 || self.min_size > self.max_size {
            return Err(ConfigError::InvalidField {
                field: format!("{field}.minSize/maxSize"),
                reason: "minSize debe ser >= 0 y <= maxSize, y maxSize > 0".to_string(),
            });
        }
        if self.min_separation < 0.0 {
            return Err(ConfigError::InvalidField {
                field: format!("{field}.minSeparation"),
                reason: "no puede ser negativa".to_string(),
            });
        }
        Ok(())
    }
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_count: 10,
            min_size: 20.0,
            max_size: 60.0,
            min_separation: 40.0,
        }
    }
}

/// Límite duro global de instancias por feature.
pub const MAX_FEATURE_COUNT: u32 = 4_096;

/// Montañas: crestas ridged sobre el heightfield.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MountainsConfig {
    /// Campos comunes (activación, máximo, tamaños, separación).
    #[serde(flatten)]
    pub base: FeatureConfig,
    /// Altura máxima de las montañas generadas (metros).
    pub max_height: f32,
    /// Afilado de la cresta (`pow(1 - |n|, k)`; mayor = más afilada).
    pub sharpness: f32,
}

impl Default for MountainsConfig {
    fn default() -> Self {
        Self {
            base: FeatureConfig {
                enabled: true,
                max_count: 15,
                min_size: 60.0,
                max_size: 180.0,
                min_separation: 120.0,
            },
            max_height: 90.0,
            sharpness: 3.0,
        }
    }
}

/// Colinas: FBM de media frecuencia sobre el heightfield.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HillsConfig {
    #[serde(flatten)]
    pub base: FeatureConfig,
    /// Altura máxima de las colinas (metros).
    pub max_height: f32,
}

impl Default for HillsConfig {
    fn default() -> Self {
        Self {
            base: FeatureConfig {
                enabled: true,
                max_count: 30,
                min_size: 40.0,
                max_size: 120.0,
                min_separation: 80.0,
            },
            max_height: 24.0,
        }
    }
}

/// Valles: depresiones suaves excavadas en el heightfield.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ValleysConfig {
    #[serde(flatten)]
    pub base: FeatureConfig,
    /// Profundidad máxima del valle (metros).
    pub max_depth: f32,
}

impl Default for ValleysConfig {
    fn default() -> Self {
        Self {
            base: FeatureConfig {
                enabled: true,
                max_count: 8,
                min_size: 80.0,
                max_size: 200.0,
                min_separation: 150.0,
            },
            max_depth: 30.0,
        }
    }
}

/// Llanuras: zonas de baja amplitud (los biomas base ya las forman; esto fuerza
/// regiones planas dedicadas).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlainsConfig {
    #[serde(flatten)]
    pub base: FeatureConfig,
    /// Amplitud máxima del microdetalle permitido (metros).
    pub max_roughness: f32,
}

impl Default for PlainsConfig {
    fn default() -> Self {
        Self {
            base: FeatureConfig {
                enabled: true,
                max_count: 10,
                min_size: 100.0,
                max_size: 260.0,
                min_separation: 120.0,
            },
            max_roughness: 1.5,
        }
    }
}

/// Acantilados: terrazas + overhangs por SDF de cápsulas con smooth-min.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CliffsConfig {
    #[serde(flatten)]
    pub base: FeatureConfig,
    /// Altura del salto del acantilado (metros).
    pub drop_height: f32,
}

impl Default for CliffsConfig {
    fn default() -> Self {
        Self {
            base: FeatureConfig {
                enabled: false,
                max_count: 12,
                min_size: 30.0,
                max_size: 120.0,
                min_separation: 100.0,
            },
            drop_height: 18.0,
        }
    }
}

/// Lagos: cuencas con nivel de agua plano.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LakesConfig {
    #[serde(flatten)]
    pub base: FeatureConfig,
    /// Profundidad máxima del lago (metros).
    pub max_depth: f32,
}

impl Default for LakesConfig {
    fn default() -> Self {
        Self {
            base: FeatureConfig {
                enabled: true,
                max_count: 8,
                min_size: 20.0,
                max_size: 90.0,
                min_separation: 100.0,
            },
            max_depth: 12.0,
        }
    }
}

/// Ríos: random-walk descendente sobre el heightfield con cauce tallado.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RiversConfig {
    /// Activación individual de la característica.
    pub enabled: bool,
    /// Cantidad máxima de ríos.
    pub max_count: u32,
    /// Longitud mínima del río (metros).
    pub length_min: f32,
    /// Longitud máxima del río (metros).
    pub length_max: f32,
    /// Semiancho del cauce (metros).
    pub width: f32,
    /// Profundidad del cauce (metros).
    pub depth: f32,
}

impl Default for RiversConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_count: 5,
            length_min: 150.0,
            length_max: 600.0,
            width: 4.0,
            depth: 3.0,
        }
    }
}

impl RiversConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.max_count > MAX_FEATURE_COUNT {
            return Err(ConfigError::InvalidField {
                field: "rivers.maxCount".to_string(),
                reason: format!("no puede superar {MAX_FEATURE_COUNT}"),
            });
        }
        if self.length_min <= 0.0 || self.length_min > self.length_max {
            return Err(ConfigError::InvalidField {
                field: "rivers.lengthMin/lengthMax".to_string(),
                reason: "lengthMin debe ser > 0 y <= lengthMax".to_string(),
            });
        }
        if self.width <= 0.0 || self.depth <= 0.0 {
            return Err(ConfigError::InvalidField {
                field: "rivers.width/depth".to_string(),
                reason: "deben ser positivos".to_string(),
            });
        }
        Ok(())
    }
}

/// Cuevas: túneles-gusano con entradas en superficie, rooms y carving reductivo.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CavesConfig {
    /// Activación individual de la característica.
    pub enabled: bool,
    /// Cantidad máxima de sistemas de cuevas.
    pub max_count: u32,
    /// Profundidad mínima de los túneles bajo la superficie (metros).
    pub depth_min: f32,
    /// Profundidad máxima de los túneles bajo la superficie (metros).
    pub depth_max: f32,
    /// Radio base de los túneles (metros).
    pub radius: f32,
    /// Probabilidad de room por segmento `[0, 1]`.
    pub room_chance: f32,
    /// Segmentos por túnel.
    pub segments: u32,
}

impl Default for CavesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_count: 20,
            depth_min: 10.0,
            depth_max: 60.0,
            radius: 3.0,
            room_chance: 0.18,
            segments: 18,
        }
    }
}

impl CavesConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.max_count > MAX_FEATURE_COUNT {
            return Err(ConfigError::InvalidField {
                field: "caves.maxCount".to_string(),
                reason: format!("no puede superar {MAX_FEATURE_COUNT}"),
            });
        }
        if self.depth_min <= 0.0 || self.depth_min > self.depth_max {
            return Err(ConfigError::InvalidField {
                field: "caves.depthMin/depthMax".to_string(),
                reason: "depthMin debe ser > 0 y <= depthMax".to_string(),
            });
        }
        if self.radius <= 0.0 {
            return Err(ConfigError::InvalidField {
                field: "caves.radius".to_string(),
                reason: "debe ser positivo".to_string(),
            });
        }
        if !(0.0..=1.0).contains(&self.room_chance) {
            return Err(ConfigError::InvalidField {
                field: "caves.roomChance".to_string(),
                reason: "debe estar entre 0 y 1".to_string(),
            });
        }
        if self.segments == 0 || self.segments > 256 {
            return Err(ConfigError::InvalidField {
                field: "caves.segments".to_string(),
                reason: "debe estar entre 1 y 256".to_string(),
            });
        }
        Ok(())
    }
}

/// Cascadas: detección de saltos de río; el plugin solo produce datos espaciales.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WaterfallsConfig {
    /// Activación individual de la característica.
    pub enabled: bool,
    /// Cantidad máxima de cascadas detectadas.
    pub max_count: u32,
    /// Salto mínimo de altura para considerar cascada (metros).
    pub min_drop: f32,
}

impl Default for WaterfallsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_count: 3,
            min_drop: 6.0,
        }
    }
}

impl WaterfallsConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.max_count > MAX_FEATURE_COUNT {
            return Err(ConfigError::InvalidField {
                field: "waterfalls.maxCount".to_string(),
                reason: format!("no puede superar {MAX_FEATURE_COUNT}"),
            });
        }
        if self.min_drop <= 0.0 {
            return Err(ConfigError::InvalidField {
                field: "waterfalls.minDrop".to_string(),
                reason: "debe ser positivo".to_string(),
            });
        }
        Ok(())
    }
}

/// Biomas activables individualmente. La mezcla por altura/pendiente decide el
/// material de superficie (splat) de cada punto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BiomesConfig {
    pub forest: bool,
    pub desert: bool,
    pub grassland: bool,
    pub snow: bool,
    pub volcanic: bool,
}

/// Configuración raíz de generación de un mundo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorldGenConfig {
    /// Seed de generación determinista. La misma seed + config produce
    /// exactamente el mismo mundo.
    pub seed: u32,
    /// Límites de dimensiones del mundo.
    pub bounds: WorldBounds,
    /// Resolución geométrica.
    pub resolution: Resolution,
    /// Montañas.
    pub mountains: MountainsConfig,
    /// Colinas.
    pub hills: HillsConfig,
    /// Valles.
    pub valleys: ValleysConfig,
    /// Llanuras.
    pub plains: PlainsConfig,
    /// Acantilados.
    pub cliffs: CliffsConfig,
    /// Lagos.
    pub lakes: LakesConfig,
    /// Ríos.
    pub rivers: RiversConfig,
    /// Cuevas y sistemas subterráneos.
    pub caves: CavesConfig,
    /// Cascadas.
    pub waterfalls: WaterfallsConfig,
    /// Biomas activables.
    pub biomes: BiomesConfig,
}

impl Default for WorldGenConfig {
    fn default() -> Self {
        Self {
            seed: 1,
            bounds: WorldBounds::default(),
            resolution: Resolution::default(),
            mountains: MountainsConfig::default(),
            hills: HillsConfig::default(),
            valleys: ValleysConfig::default(),
            plains: PlainsConfig::default(),
            cliffs: CliffsConfig::default(),
            lakes: LakesConfig::default(),
            rivers: RiversConfig::default(),
            caves: CavesConfig::default(),
            waterfalls: WaterfallsConfig::default(),
            biomes: BiomesConfig {
                forest: true,
                desert: false,
                grassland: true,
                snow: false,
                volcanic: false,
            },
        }
    }
}

impl WorldGenConfig {
    /// Valida todos los invariantes. El planificador solo acepta configs validadas.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.bounds.validate()?;
        if self.resolution.voxel_size < 0.25 || self.resolution.voxel_size > 4.0 {
            return Err(ConfigError::InvalidField {
                field: "resolution.voxelSize".to_string(),
                reason: "debe estar entre 0.25 y 4.0 metros".to_string(),
            });
        }
        self.mountains.base.validate("mountains")?;
        self.hills.base.validate("hills")?;
        self.valleys.base.validate("valleys")?;
        self.plains.base.validate("plains")?;
        self.cliffs.base.validate("cliffs")?;
        self.lakes.base.validate("lakes")?;
        self.rivers.validate()?;
        self.caves.validate()?;
        self.waterfalls.validate()?;
        if self.mountains.base.enabled && self.mountains.max_height <= 0.0 {
            return Err(ConfigError::InvalidField {
                field: "mountains.maxHeight".to_string(),
                reason: "debe ser positivo".to_string(),
            });
        }
        if self.mountains.base.enabled && self.mountains.sharpness < 0.0 {
            return Err(ConfigError::InvalidField {
                field: "mountains.sharpness".to_string(),
                reason: "no puede ser negativo".to_string(),
            });
        }
        if self.hills.base.enabled && self.hills.max_height <= 0.0 {
            return Err(ConfigError::InvalidField {
                field: "hills.maxHeight".to_string(),
                reason: "debe ser positivo".to_string(),
            });
        }
        if self.valleys.base.enabled && self.valleys.max_depth <= 0.0 {
            return Err(ConfigError::InvalidField {
                field: "valleys.maxDepth".to_string(),
                reason: "debe ser positivo".to_string(),
            });
        }
        if self.plains.base.enabled && self.plains.max_roughness < 0.0 {
            return Err(ConfigError::InvalidField {
                field: "plains.maxRoughness".to_string(),
                reason: "no puede ser negativo".to_string(),
            });
        }
        if self.cliffs.base.enabled && self.cliffs.drop_height <= 0.0 {
            return Err(ConfigError::InvalidField {
                field: "cliffs.dropHeight".to_string(),
                reason: "debe ser positivo".to_string(),
            });
        }
        if self.lakes.base.enabled && self.lakes.max_depth <= 0.0 {
            return Err(ConfigError::InvalidField {
                field: "lakes.maxDepth".to_string(),
                reason: "debe ser positivo".to_string(),
            });
        }
        Ok(())
    }

    /// Número de chunks en X y Z que cubren los límites del mundo.
    ///
    /// Los chunks se indexan en unidades de CHUNK_SIZE voxels; los bordes del mundo
    /// se completan con chunks parciales.
    #[must_use]
    pub fn chunk_count_xz(&self, chunk_size_voxels: u32) -> (u32, u32) {
        let chunks_x = (self.bounds.width / (chunk_size_voxels as f32 * self.resolution.voxel_size))
            .ceil() as u32;
        let chunks_z = (self.bounds.length
            / (chunk_size_voxels as f32 * self.resolution.voxel_size))
            .ceil() as u32;
        (chunks_x.max(1), chunks_z.max(1))
    }

    /// Altura máxima en chunks Y que pueden contener terreno.
    #[must_use]
    pub fn max_chunk_y(&self, chunk_size_voxels: u32) -> u32 {
        (self.bounds.height / (chunk_size_voxels as f32 * self.resolution.voxel_size)).ceil() as u32
    }
}

/// Error de validación de configuración.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigError {
    /// Un campo no cumple sus invariantes.
    InvalidField {
        /// Ruta del campo en el esquema camelCase.
        field: String,
        /// Descripción del problema.
        reason: String,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidField { field, reason } => write!(f, "campo invalido '{field}': {reason}"),
        }
    }
}

impl std::error::Error for ConfigError {}
