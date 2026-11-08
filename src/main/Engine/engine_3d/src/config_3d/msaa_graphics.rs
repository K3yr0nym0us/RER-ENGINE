//! Nivel de anti-aliasing por multisampling (MSAA) del pase forward 3D.
//!
//! Nota: la creación de pipelines wgpu y la verificación de formatos contra el device
//! se eliminó al cortar el frame a Ash (Fase 2). Mientras tanto, `clamp_sample_count`
//! devuelve el `desired` clampado sin consultar al device (Fase 4 lo hará con Vulkan).

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum MsaaTier {
    #[default]
    Low,
    Medium,
    High,
    Ultra,
}

impl MsaaTier {
    pub fn from_wire(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "low" | "bajo" | "off" | "apagado" => Some(Self::Low),
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

    /// Muestras pedidas por el tier (antes de clamp al soporte del dispositivo).
    pub fn desired_sample_count(self) -> u32 {
        match self {
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 4,
            Self::Ultra => 8,
        }
    }
}

pub const DEFAULT_MSAA_TIER: MsaaTier = MsaaTier::Low;

/// Elige el mayor `sample_count` ≤ `desired`. Versión temporal sin consulta al device;
/// se conecta a Vulkan en Fase 4.
pub fn clamp_sample_count(_device: &crate::vulkan::SharedGpu, desired: u32) -> u32 {
    desired.clamp(1, 8)
}
