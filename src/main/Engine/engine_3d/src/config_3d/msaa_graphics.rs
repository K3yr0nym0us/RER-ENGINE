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

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [MsaaTier; 4] = [
        MsaaTier::Low,
        MsaaTier::Medium,
        MsaaTier::High,
        MsaaTier::Ultra,
    ];

    #[test]
    fn sample_counts_grow_and_ultra_asks_for_eight() {
        // Card `MSAA EXTENDIDO (3D)`: comprobar que Ultra PIDE 8x. Si el adaptador recorta,
        // `ensure_msaa` lo avisa; aquí sólo se fija la petición del tier.
        assert_eq!(MsaaTier::Low.desired_sample_count(), 1);
        assert_eq!(MsaaTier::Medium.desired_sample_count(), 2);
        assert_eq!(MsaaTier::High.desired_sample_count(), 4);
        assert_eq!(MsaaTier::Ultra.desired_sample_count(), 8);
        for pair in ALL.windows(2) {
            assert!(
                pair[0].desired_sample_count() <= pair[1].desired_sample_count(),
                "el tier {} pide menos que {}",
                pair[0].wire(),
                pair[1].wire()
            );
        }
    }

    #[test]
    fn wire_round_trip_and_off_maps_to_low() {
        for tier in ALL {
            assert_eq!(MsaaTier::from_wire(tier.wire()), Some(tier));
        }
        assert_eq!(MsaaTier::from_wire("apagado"), Some(MsaaTier::Low));
        assert_eq!(MsaaTier::from_wire("2x"), None);
        assert_eq!(DEFAULT_MSAA_TIER, MsaaTier::Low);
    }
}
