//! Preset de calidad RT/SSR por tier (distancia, rebotes, denoise, material, resolución).

use super::settings::ReflectionTier;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtMaterialQuality {
    Simplified = 0,
    Hybrid = 1,
    Full = 2,
}

impl RtMaterialQuality {
    pub fn shader_value(self) -> f32 {
        self as u32 as f32
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DenoiseProfile {
    pub depth_sigma: f32,
    pub normal_sigma: f32,
    pub luminance_sigma: f32,
    pub radius: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct ReflectionQualityPreset {
    pub ssr_resolution_scale: f32,
    pub rt_resolution_scale: f32,
    pub max_trace_distance_m: f32,
    pub max_bounces: u32,
    pub shadow_rays: bool,
    pub denoise: Option<DenoiseProfile>,
    pub material_quality: RtMaterialQuality,
}

/// Low + RT: cambiar a `UltraLight` para benchmark Opción B (`radius = 1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum LowRtDenoiseMode {
    /// Opción A — OFF (default hasta benchmark GPU).
    Off,
    /// Opción B — ultraligero.
    UltraLight,
}

const LOW_RT_DENOISE: LowRtDenoiseMode = LowRtDenoiseMode::Off;

impl ReflectionQualityPreset {
    pub fn from_tier(tier: ReflectionTier) -> Option<Self> {
        let scale = tier.reflection_screen_fraction();
        match tier {
            ReflectionTier::Off => None,
            ReflectionTier::Low => Some(Self {
                ssr_resolution_scale: scale,
                rt_resolution_scale: scale,
                max_trace_distance_m: 25.0,
                max_bounces: 1,
                shadow_rays: false,
                denoise: low_rt_denoise_profile(),
                material_quality: RtMaterialQuality::Simplified,
            }),
            ReflectionTier::Medium => Some(Self {
                ssr_resolution_scale: scale,
                rt_resolution_scale: scale,
                max_trace_distance_m: 50.0,
                max_bounces: 1,
                shadow_rays: false,
                denoise: Some(DenoiseProfile {
                    depth_sigma: 5.0,
                    normal_sigma: 10.0,
                    luminance_sigma: 10.0,
                    radius: 2,
                }),
                material_quality: RtMaterialQuality::Simplified,
            }),
            ReflectionTier::High => Some(Self {
                ssr_resolution_scale: scale,
                rt_resolution_scale: scale,
                max_trace_distance_m: 100.0,
                max_bounces: 1,
                shadow_rays: true,
                denoise: Some(DenoiseProfile {
                    depth_sigma: 4.0,
                    normal_sigma: 12.0,
                    luminance_sigma: 8.0,
                    radius: 3,
                }),
                material_quality: RtMaterialQuality::Hybrid,
            }),
            ReflectionTier::Ultra => Some(Self {
                ssr_resolution_scale: scale,
                rt_resolution_scale: scale,
                max_trace_distance_m: 100.0,
                max_bounces: 2,
                shadow_rays: true,
                denoise: Some(DenoiseProfile {
                    depth_sigma: 3.0,
                    normal_sigma: 14.0,
                    luminance_sigma: 6.0,
                    radius: 3,
                }),
                material_quality: RtMaterialQuality::Full,
            }),
        }
    }
}

fn low_rt_denoise_profile() -> Option<DenoiseProfile> {
    match LOW_RT_DENOISE {
        LowRtDenoiseMode::Off => None,
        LowRtDenoiseMode::UltraLight => Some(DenoiseProfile {
            depth_sigma: 4.0,
            normal_sigma: 6.0,
            luminance_sigma: 8.0,
            radius: 1,
        }),
    }
}
