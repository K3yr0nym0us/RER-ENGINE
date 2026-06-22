//! Nivel de calidad de sombras (resolución del shadow map por tier).
//!
//! Hoy el motor usa un único shadow map (sin cascadas); este tier solo controla su resolución.
//! Las cascadas (CSM) y la distancia de sombra extendida de Ultra están en el checklist como
//! tarea pendiente (`CHECKLIST-3D.md` → Gráficos / Preset de calidad).

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

    /// Resolución del shadow map (px por lado) según el tier. Off usa 1×1 (mínimo,
    /// no se renderiza ni se muestrea). Low reutiliza el tamaño base; el resto ×2 hasta 8192.
    pub fn shadow_map_size(self) -> u32 {
        match self {
            Self::Off => 1,
            Self::Low => crate::engine::SHADOW_MAP_SIZE,
            Self::Medium => 2048,
            Self::High => 4096,
            Self::Ultra => 8192,
        }
    }
}

pub const DEFAULT_SHADOW_TIER: ShadowTier = ShadowTier::Low;
