//! Nivel global de reflejos (Off / Low / Medium / High) y presets internos.

use super::quality_preset::{DenoiseProfile, ReflectionQualityPreset, RtMaterialQuality};

/// El pass RT se ejecuta solo con el switch manual del editor (`raytracing_enabled`).
pub const RT_PIPELINE_AVAILABLE: bool = true;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Hash, Default)]
pub enum ReflectionTier {
    #[default]
    Off,
    Low,
    Medium,
    High,
    Ultra,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReflectionDebugView {
    Final,
    /// Una sola vista SSR: máscara de aciertos + logs `[reflexiones][ssr]` en consola.
    SsrDebug,
    /// Prueba 1: verde si el SSR falla (sin hit), normal si acierta.
    SsrMissGreen,
    /// Prueba 2: rojo=fuea de pantalla, azul=sin iteraciones, normal=hit.
    SsrExitReason,
    /// Prueba 3: vector de reflexión mapeado a RGB.
    SsrVectorRgb,
    /// Rojo=self-hit rechazado, verde=hit SSR válido, azul=miss (path idéntico a `ssr.wgsl`).
    SsrHitClass,
    /// Escala de grises: distancia del hit en px (`path_px`); negro=miss, blanco=hit lejano.
    SsrPathPx,
    /// RGB = `R_world * 0.5 + 0.5` — dirección exacta pasada a `ssr_evaluate_trace`.
    SsrMarchReflDir,
    /// `fract(hit_uv * 8)` — UV de muestreo en hits SSR válidos (post-reject, como `ssr.wgsl`).
    SsrHitUv,
    /// RGB = `ssr_reflected_radiance(hit_uv)` en hits válidos (sin composite).
    SsrHitSampleColor,
    /// Mapa de calor: |clip.z/w − prepass z| (moiré corona si alto).
    SsrProjDepthDelta,
}

impl ReflectionDebugView {
    pub fn from_wire(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "final" | "" => Some(Self::Final),
            "ssr_debug" | "ssrdebug" | "ssr" | "debug" | "ssr_hits" | "ssrhits" | "hits" => {
                Some(Self::SsrDebug)
            }
            "ssr_miss_green" | "miss_green" | "missgreen" | "green" => Some(Self::SsrMissGreen),
            "ssr_exit_reason" | "exit_reason" | "exitreason" => Some(Self::SsrExitReason),
            "ssr_vector_rgb" | "vector_rgb" | "vectorrgb" | "refl_vector" => {
                Some(Self::SsrVectorRgb)
            }
            "ssr_hit_class"
            | "ssr_hitclass"
            | "hit_class"
            | "self_hit"
            | "ssr_self_hit" => Some(Self::SsrHitClass),
            "ssr_path_px" | "ssr_pathpx" | "path_px" | "ssr_ray_path" => Some(Self::SsrPathPx),
            "ssr_march_refl_dir"
            | "ssr_refl_dir"
            | "ssr_r_world"
            | "R_world"
            | "march_refl_dir" => Some(Self::SsrMarchReflDir),
            "ssr_hit_uv" | "ssr_hituv" | "hit_uv" | "ssr_sample_uv" => Some(Self::SsrHitUv),
            "ssr_hit_sample_color"
            | "ssr_sample_color"
            | "hit_sample_color"
            | "ssr_lit_at_hit" => Some(Self::SsrHitSampleColor),
            "ssr_proj_depth_delta"
            | "ssr_start_cs_z_delta"
            | "proj_depth_delta"
            | "start_cs_z_delta" => Some(Self::SsrProjDepthDelta),
            _ => None,
        }
    }

    pub fn wire(self) -> &'static str {
        match self {
            Self::Final => "final",
            Self::SsrDebug => "ssr_debug",
            Self::SsrMissGreen => "ssr_miss_green",
            Self::SsrExitReason => "ssr_exit_reason",
            Self::SsrVectorRgb => "ssr_vector_rgb",
            Self::SsrHitClass => "ssr_hit_class",
            Self::SsrPathPx => "ssr_path_px",
            Self::SsrMarchReflDir => "ssr_march_refl_dir",
            Self::SsrHitUv => "ssr_hit_uv",
            Self::SsrHitSampleColor => "ssr_hit_sample_color",
            Self::SsrProjDepthDelta => "ssr_proj_depth_delta",
        }
    }

    pub fn is_visual_debug(self) -> bool {
        matches!(
            self,
            Self::SsrDebug
                | Self::SsrMissGreen
                | Self::SsrExitReason
                | Self::SsrVectorRgb
                | Self::SsrHitClass
                | Self::SsrPathPx
                | Self::SsrMarchReflDir
                | Self::SsrHitUv
                | Self::SsrHitSampleColor
                | Self::SsrProjDepthDelta
        )
    }

    pub fn enables_ssr_stats(self) -> bool {
        matches!(self, Self::SsrDebug)
    }

    pub fn shader_index(self) -> u32 {
        match self {
            Self::Final => 0,
            Self::SsrDebug => 3,
            Self::SsrMissGreen => 30,
            Self::SsrExitReason => 31,
            Self::SsrVectorRgb => 32,
            Self::SsrHitClass => 33,
            Self::SsrPathPx => 34,
            Self::SsrMarchReflDir => 35,
            Self::SsrHitUv => 36,
            Self::SsrHitSampleColor => 37,
            Self::SsrProjDepthDelta => 38,
        }
    }
}

/// Preset derivado del tier; no se persiste en `.save`.
#[derive(Clone, Copy, Debug)]
pub struct ReflectionSettings {
    pub tier: ReflectionTier,
    pub max_steps: u32,
    pub binary_steps: u32,
    pub max_distance_m: f32,
    /// Fracción del viewport para SSR / temporal / denoise.
    pub screen_fraction: f32,
    /// Fracción del viewport para dispatch RT (hoy igual que `screen_fraction`).
    pub rt_resolution_scale: f32,
    pub temporal_blend: f32,
    pub max_roughness_to_trace: f32,
    /// Toggle de editor: captura cubemap + IBL forward (independiente del SSR).
    pub probes_enabled: bool,
    /// Toggle de editor: ray tracing HW (independiente del tier).
    pub raytracing_enabled: bool,
    pub rt_blend: f32,
}

impl ReflectionTier {
    pub fn from_wire(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "off" | "disabled" | "desactivado" | "none" => Some(Self::Off),
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

    /// Resolución por cara del cubemap de probes según el tier (px). Off conserva el mínimo
    /// (no captura, pero la textura existe). Low 128, Medium 256, High 512, Ultra 1024.
    pub fn cubemap_face_size(self) -> u32 {
        use crate::reflections::probe_env::PROBE_FACE_SIZE;
        match self {
            Self::Off => PROBE_FACE_SIZE,
            Self::Low => PROBE_FACE_SIZE,
            Self::Medium => 256,
            Self::High => 512,
            Self::Ultra => 1024,
        }
    }

    /// Fracción lineal del viewport para el pass de reflejos (SSR hoy; RT usará la misma escala al cablearse).
    /// | Tier   | Resolución |
    /// | Low    | 25%        |
    /// | Medium | 50%        |
    /// | High   | 75%        |
    /// | Ultra  | 100%       |
    pub fn reflection_screen_fraction(self) -> f32 {
        match self {
            Self::Off => 1.0,
            Self::Low => 0.25,
            Self::Medium => 0.50,
            Self::High => 0.75,
            Self::Ultra => 1.0,
        }
    }
}

impl ReflectionSettings {
    pub fn quality_preset(self) -> Option<ReflectionQualityPreset> {
        ReflectionQualityPreset::from_tier(self.tier)
    }

    pub fn from_tier(tier: ReflectionTier) -> Self {
        let preset = ReflectionQualityPreset::from_tier(tier);
        let screen_fraction = preset
            .map(|p| p.ssr_resolution_scale)
            .unwrap_or_else(|| tier.reflection_screen_fraction());
        let rt_resolution_scale = preset
            .map(|p| p.rt_resolution_scale)
            .unwrap_or(screen_fraction);
        let max_distance_m = preset.map(|p| p.max_trace_distance_m).unwrap_or(0.0);
        match tier {
            ReflectionTier::Off => Self {
                tier,
                max_steps: 0,
                binary_steps: 0,
                max_distance_m,
                screen_fraction,
                rt_resolution_scale,
                temporal_blend: 0.0,
                max_roughness_to_trace: 1.0,
                probes_enabled: false,
                raytracing_enabled: false,
                rt_blend: 0.85,
            },
            ReflectionTier::Low => Self {
                tier,
                max_steps: 16,
                binary_steps: 3,
                max_distance_m,
                screen_fraction,
                rt_resolution_scale,
                temporal_blend: 0.18,
                max_roughness_to_trace: 0.70,
                probes_enabled: false,
                raytracing_enabled: false,
                rt_blend: 0.85,
            },
            ReflectionTier::Medium => Self {
                tier,
                max_steps: 32,
                binary_steps: 5,
                max_distance_m,
                screen_fraction,
                rt_resolution_scale,
                temporal_blend: 0.22,
                max_roughness_to_trace: 0.70,
                probes_enabled: false,
                raytracing_enabled: false,
                rt_blend: 0.85,
            },
            ReflectionTier::High => Self {
                tier,
                max_steps: 64,
                binary_steps: 5,
                max_distance_m,
                screen_fraction,
                rt_resolution_scale,
                temporal_blend: 0.42,
                max_roughness_to_trace: 0.70,
                probes_enabled: false,
                raytracing_enabled: false,
                rt_blend: 0.85,
            },
            ReflectionTier::Ultra => Self {
                tier,
                max_steps: 128,
                binary_steps: 7,
                max_distance_m,
                screen_fraction,
                rt_resolution_scale,
                temporal_blend: 0.45,
                max_roughness_to_trace: 0.85,
                probes_enabled: false,
                raytracing_enabled: false,
                rt_blend: 0.85,
            },
        }
    }

    pub fn max_bounces(self) -> u32 {
        self.quality_preset().map(|p| p.max_bounces).unwrap_or(0)
    }

    pub fn rt_shadow_rays(self) -> bool {
        self.quality_preset()
            .map(|p| p.shadow_rays)
            .unwrap_or(false)
    }

    pub fn rt_material_quality(self) -> RtMaterialQuality {
        self.quality_preset()
            .map(|p| p.material_quality)
            .unwrap_or(RtMaterialQuality::Full)
    }

    pub fn denoise_profile(self) -> Option<DenoiseProfile> {
        self.quality_preset().and_then(|p| p.denoise)
    }

    pub fn active(self) -> bool {
        self.tier != ReflectionTier::Off
    }

    pub fn uses_probes(self) -> bool {
        self.probes_enabled && self.active()
    }

    pub fn uses_rt(self) -> bool {
        RT_PIPELINE_AVAILABLE && self.raytracing_enabled && self.active()
    }

    /// Denoise bilateral en reflejos: solo con RT activo y perfil definido por tier.
    pub fn uses_denoise(self) -> bool {
        self.active() && self.uses_rt() && self.denoise_profile().is_some()
    }

    /// Fracción de píxeles del rayo SSR en pantalla (0–1).
    pub fn ssr_coarse_resolution(self) -> f32 {
        self.screen_fraction
    }

    /// Tope de iteraciones del coarse pass SSR.
    pub fn ssr_coarse_max_iters(self) -> u32 {
        self.max_steps
    }

    /// Tolerancia del test de profundidad SSR (metros).
    pub fn ssr_thickness_m(self) -> f32 {
        crate::reflections::ssr_pipeline::ssr_settings::ssr_thickness_m(self.tier)
    }

    /// Iteraciones del refinamiento binario SSR.
    pub fn ssr_binary_steps(self) -> u32 {
        crate::reflections::ssr_pipeline::ssr_settings::ssr_binary_steps(self.tier)
    }
}

pub const DEFAULT_REFLECTION_TIER: ReflectionTier = ReflectionTier::Off;

#[derive(Clone, Copy, Debug, Default)]
pub struct ReflectionProfilerMs {
    pub ssr_ms: f32,
    pub rt_ms: f32,
    pub temporal_ms: f32,
    pub denoise_ms: f32,
    pub composite_ms: f32,
}
