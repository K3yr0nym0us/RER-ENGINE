//! Nivel de anti-aliasing por multisampling (MSAA) del pase forward 3D.
//!
//! El tier pide un número de muestras; `clamp_sample_count` lo ajusta a lo que el adaptador
//! soporta realmente como adjunto de color Y de profundidad (`SharedGpu::max_gbuffer_samples`),
//! y `FrameRenderer` reserva el G-buffer multisampleado con ese valor.

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

/// Elige el mayor `sample_count` ≤ `desired` que el adaptador admite para el G-buffer del pase
/// forward (color y profundidad a la vez). Sin soporte de MSAA devuelve 1×.
pub fn clamp_sample_count(device: &crate::vulkan::SharedGpu, desired: u32) -> u32 {
    device.max_gbuffer_samples(desired).clamp(1, 8)
}
