//! Nivel de calidad de sombras (resolución del shadow map por tier).
//!
//! Hoy el motor usa un único shadow map (sin cascadas); este tier solo controla su resolución.
//! Las cascadas (CSM) y la distancia de sombra extendida de Ultra están en el checklist como
//! tarea pendiente (`CHECKLIST-3D.md` → Gráficos / Preset de calidad).

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ShadowTier {
    #[default]
    Low,
    Medium,
    High,
    Ultra,
}

impl ShadowTier {
    pub fn from_wire(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "low" | "bajo" => Some(Self::Low),
            "medium" | "medio" => Some(Self::Medium),
            "high" | "alto" => Some(Self::High),
            "ultra" => Some(Self::Ultra),
            _ => None,
        }
    }

    pub fn wire(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    /// Resolución del shadow map (px por lado) según el tier. Low reutiliza el tamaño base del
    /// motor (`SHADOW_MAP_SIZE`); el resto escala ×2 hasta 8192 en Ultra.
    pub fn shadow_map_size(self) -> u32 {
        match self {
            Self::Low => crate::engine::SHADOW_MAP_SIZE,
            Self::Medium => 2048,
            Self::High => 4096,
            Self::Ultra => 8192,
        }
    }
}

pub const DEFAULT_SHADOW_TIER: ShadowTier = ShadowTier::Low;
