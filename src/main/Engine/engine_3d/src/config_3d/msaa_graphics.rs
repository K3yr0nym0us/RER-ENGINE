//! Nivel de anti-aliasing por multisampling (MSAA) del pase forward 3D.
//!
//! Low = Off (1 muestra). El resolve escribe en los G-buffers 1× que consumen TAA/SSR.
//! Shadow maps, probes y post-FX permanecen en sample_count = 1.

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

/// Elige el mayor `sample_count` ≤ `desired` soportado por todos los formatos del MRT forward.
pub fn clamp_sample_count(device: &wgpu::Device, desired: u32) -> u32 {
    if desired <= 1 {
        return 1;
    }
    let formats = [
        crate::taa::MRT_LIT_FORMAT,
        crate::taa::SHADOW_MASK_FORMAT,
        crate::taa::DEPTH_EXPORT_FORMAT,
        crate::taa::VELOCITY_FORMAT,
        crate::taa::BASE_COLOR_FORMAT,
        crate::taa::WORLD_POS_FORMAT,
        crate::engine::types::DEPTH_FORMAT,
    ];
    let mut best = 1u32;
    for &candidate in &[2u32, 4, 8] {
        if candidate > desired {
            break;
        }
        if formats
            .iter()
            .all(|&fmt| format_supports_samples(device, fmt, candidate))
        {
            best = candidate;
        }
    }
    best
}

fn format_supports_samples(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    samples: u32,
) -> bool {
    let flags = format.guaranteed_format_features(device.features()).flags;
    use wgpu::TextureFormatFeatureFlags as F;
    match samples {
        1 => true,
        2 => flags.contains(F::MULTISAMPLE_X2),
        4 => flags.contains(F::MULTISAMPLE_X4),
        8 => flags.contains(F::MULTISAMPLE_X8),
        _ => false,
    }
}

/// `true` si el formato admite resolve por hardware (MS → 1× en el render pass).
pub fn format_supports_resolve(device: &wgpu::Device, format: wgpu::TextureFormat) -> bool {
    format
        .guaranteed_format_features(device.features())
        .flags
        .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
}
