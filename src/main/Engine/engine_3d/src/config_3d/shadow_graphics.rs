//! Nivel de calidad de sombras (Cascaded Shadow Maps): cascadas, resolución por cascada y
//! distancia de sombra.
//!
//! El motor usa un array de `MAX_SHADOW_CASCADES` capas de profundidad; el tier decide cuántas capas
//! se dibujan y con qué lado de mapa. `vulkan::FrameRenderer` reasigna el array cuando cambia el
//! tamaño y `scene_shadow_params` fija el radio PCF en unidades de mundo, así que
//! low/medium/high/ultra se ven distinto y cuestan distinto. Ultra añade la distancia de sombra
//! extendida (card `CASCADES / CSM (3D)` en Trello).

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ShadowTier {
    Off,
    #[default]
    Low,
    Medium,
    High,
    Ultra,
}

impl ShadowTier {
    pub fn from_wire(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "off" | "apagado" => Some(Self::Off),
            "low" | "bajo" => Some(Self::Low),
            "medium" | "medio" => Some(Self::Medium),
            "high" | "alto" => Some(Self::High),
            "ultra" => Some(Self::Ultra),
            _ => None,
        }
    }

    pub fn wire(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    /// Cascadas dibujadas por tier: Low 2, Medium 3, High 4, Ultra 4 (card `CASCADES / CSM (3D)`).
    /// Off no dibuja ninguna; el shader lee el número en `shadow_params.w` y no selecciona vacías.
    pub fn cascades(self) -> usize {
        match self {
            Self::Off => 0,
            Self::Low => 2,
            Self::Medium => 3,
            Self::High | Self::Ultra => 4,
        }
    }

    /// Resolución del shadow map (px por lado) **por cascada**. Off usa 1×1 (mínimo, no se
    /// renderiza ni se muestrea).
    ///
    /// Con CSM el lado deja de subir a 8192: el array ya cuesta 4× un mapa único, y el detalle cerca
    /// de la cámara lo da la primera cascada (que encuadra unos pocos metros), no un mapa gigante
    /// que desperdicia texeles en el primer plano.
    pub fn shadow_map_size(self) -> u32 {
        match self {
            Self::Off => 1,
            Self::Low | Self::Medium => crate::engine::SHADOW_MAP_SIZE,
            Self::High | Self::Ultra => 2048,
        }
    }

    /// Multiplicador sobre el radio de escena que fija el borde lejano de la última cascada. Ultra
    /// es la "extended shadow distance" de la card; se expresa como fracción del tamaño del mundo
    /// para que las sombras no se corten antes si la escena crece.
    pub fn shadow_distance_scale(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Low => 0.55,
            Self::Medium => 0.75,
            Self::High => 1.0,
            Self::Ultra => 1.45,
        }
    }

    /// Radio PCF pedido en unidades de mundo. El shader lo traduce a texeles de la cascada que
    /// toque (`shadow_params.x`), porque cada cascada cubre un área distinta con el mismo mapa.
    pub fn shadow_soft_radius(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Low => 0.045,
            Self::Medium => 0.055,
            Self::High => 0.065,
            Self::Ultra => 0.085,
        }
    }

    /// Fracción del borde de cada cascada (menos la última) donde la sombra se desvanece, para que
    /// el salto de densidad de texel entre cascadas no se lea como una línea dura.
    pub const fn cascade_fade(self) -> f32 {
        match self {
            Self::Low | Self::Medium => 0.10,
            Self::High | Self::Ultra => 0.12,
            Self::Off => 0.0,
        }
    }
}

pub const DEFAULT_SHADOW_TIER: ShadowTier = ShadowTier::Low;
