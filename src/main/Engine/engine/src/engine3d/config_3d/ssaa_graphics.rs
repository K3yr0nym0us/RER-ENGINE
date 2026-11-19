//! SSAA (supersampling anti-aliasing): el motor rasteriza el G-buffer a una resolución mayor
//! que la del viewport y reduce la imagen al presentar. A diferencia de MSAA (que comparte el
//! fragment shader entre muestras), SSAA evalúa el shading de cada píxel supersampleado.
//!
//! El tier elige cuántas "muestras lógicas" se piden (1/2/4/8, las mismas que MSAA). La escala
//! lineal es `sqrt(muestras)`: 1×, √2×, 2× y √8×. `FrameRenderer` la recorta al límite de imagen
//! del adaptador en el momento de grabar el frame.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SsaaTier {
    #[default]
    Low,
    Medium,
    High,
    Ultra,
}

impl SsaaTier {
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

    /// Muestras lógicas pedidas por el tier (1 = sin SSAA). Equivale al `sample_count` de MSAA.
    pub fn desired_samples(self) -> u32 {
        match self {
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 4,
            Self::Ultra => 8,
        }
    }

    /// Escala lineal de supersampling derivada de las muestras lógicas (`sqrt(samples)`).
    pub fn desired_scale(self) -> f32 {
        (self.desired_samples() as f32).sqrt()
    }
}

pub const DEFAULT_SSAA_TIER: SsaaTier = SsaaTier::Low;

/// Escala máxima que cabe en las dimensiones de imagen del adaptador para el viewport dado.
/// Devuelve al menos 1× (sin supersampling) aunque el viewport sea mayor que el límite.
pub fn clamp_scale(
    device: &crate::engine3d::vulkan::SharedGpu,
    desired: f32,
    width: u32,
    height: u32,
) -> f32 {
    let limit = device.max_image_dimension().max(1) as f32;
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;
    desired.max(1.0).min(limit / w).min(limit / h).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [SsaaTier; 4] = [
        SsaaTier::Low,
        SsaaTier::Medium,
        SsaaTier::High,
        SsaaTier::Ultra,
    ];

    #[test]
    fn sample_counts_and_scales_grow_with_tier() {
        assert_eq!(SsaaTier::Low.desired_samples(), 1);
        assert_eq!(SsaaTier::Medium.desired_samples(), 2);
        assert_eq!(SsaaTier::High.desired_samples(), 4);
        assert_eq!(SsaaTier::Ultra.desired_samples(), 8);
        assert_eq!(SsaaTier::Low.desired_scale(), 1.0);
        assert!((SsaaTier::High.desired_scale() - 2.0).abs() < 1e-4);
        for pair in ALL.windows(2) {
            assert!(
                pair[0].desired_scale() <= pair[1].desired_scale(),
                "el tier {} pide menos que {}",
                pair[0].wire(),
                pair[1].wire()
            );
        }
    }

    #[test]
    fn wire_round_trip_and_off_maps_to_low() {
        for tier in ALL {
            assert_eq!(SsaaTier::from_wire(tier.wire()), Some(tier));
        }
        assert_eq!(SsaaTier::from_wire("apagado"), Some(SsaaTier::Low));
        assert_eq!(SsaaTier::from_wire("4x"), None);
        assert_eq!(DEFAULT_SSAA_TIER, SsaaTier::Low);
    }
}
