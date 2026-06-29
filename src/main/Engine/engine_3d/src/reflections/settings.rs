//! Nivel global de reflejos (Off / Low / Medium / High) y presets internos.

/// Reactivar cuando `rt_pipeline` vuelva a `ReflectionPass::run`.
pub const RT_ENABLED: bool = false;

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
        }
    }

    pub fn is_visual_debug(self) -> bool {
        matches!(self, Self::SsrDebug | Self::SsrMissGreen | Self::SsrExitReason | Self::SsrVectorRgb)
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
    pub screen_fraction: f32,
    pub temporal_blend: f32,
    pub max_roughness_to_trace: f32,
    /// Toggle de editor: captura cubemap + IBL forward (independiente del SSR).
    pub probes_enabled: bool,
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
}

impl ReflectionSettings {
    pub fn from_tier(tier: ReflectionTier) -> Self {
        // SSR por tier (Lettier):
        // | Tier   | Resolución | Ray march | Binary |
        // | Low    | 1/4        | 12        | 1      |
        // | Medium | 1/2        | 24        | 3      |
        // | High   | 1/2        | 48        | 5      |
        // | Ultra  | Full       | 512       | 7      |
        match tier {
            ReflectionTier::Off => Self {
                tier,
                max_steps: 0,
                binary_steps: 0,
                max_distance_m: 0.0,
                screen_fraction: 1.0,
                temporal_blend: 0.0,
                max_roughness_to_trace: 1.0,
                probes_enabled: false,
            },
            ReflectionTier::Low => Self {
                tier,
                max_steps: 16,
                binary_steps: 3,
                max_distance_m: 50.0,
                screen_fraction: 0.50,
                temporal_blend: 0.18,
                max_roughness_to_trace: 0.70,
                probes_enabled: false,
            },
            ReflectionTier::Medium => Self {
                tier,
                max_steps: 32,
                binary_steps: 5,
                max_distance_m: 50.0,
                screen_fraction: 0.50,
                temporal_blend: 0.22,
                max_roughness_to_trace: 0.70,
                probes_enabled: false,
            },
            ReflectionTier::High => Self {
                tier,
                max_steps: 64,
                binary_steps: 5,
                max_distance_m: 100.0,
                screen_fraction: 0.75,
                temporal_blend: 0.42,
                max_roughness_to_trace: 0.70,
                probes_enabled: false,
            },
            ReflectionTier::Ultra => Self {
                tier,
                max_steps: 128,
                binary_steps: 7,
                max_distance_m: 100.0,
                screen_fraction: 1.0,
                temporal_blend: 0.45,
                max_roughness_to_trace: 0.85,
                probes_enabled: false,
            },
        }
    }

    pub fn active(self) -> bool {
        self.tier != ReflectionTier::Off
    }

    pub fn uses_probes(self) -> bool {
        self.probes_enabled && self.active()
    }

    pub fn uses_rt(self) -> bool {
        RT_ENABLED && self.active()
    }

    /// Lettier `resolution`: fracción de píxeles del rayo en pantalla (0–1).
    pub fn ssr_coarse_resolution(self) -> f32 {
        crate::reflections::ssr_pipeline::ssr_settings::ssr_coarse_resolution(self.tier)
    }

    /// Tope de iteraciones del coarse pass (Lettier `int(delta)` acotado por tier).
    pub fn ssr_coarse_max_iters(self) -> u32 {
        self.max_steps
    }

    /// Lettier `thickness`: tolerancia del test de profundidad (metros). Guía usa 0.5 por defecto.
    pub fn ssr_thickness_m(self) -> f32 {
        crate::reflections::ssr_pipeline::ssr_settings::ssr_thickness_m(self.tier)
    }

    /// Lettier `steps`: iteraciones del paso de refinamiento binario.
    pub fn ssr_binary_steps(self) -> u32 {
        crate::reflections::ssr_pipeline::ssr_settings::ssr_binary_steps(self.tier)
    }
}

pub const DEFAULT_REFLECTION_TIER: ReflectionTier = ReflectionTier::Off;

#[derive(Clone, Copy, Debug, Default)]
pub struct ReflectionProfilerMs {
    pub ssr_ms: f32,
    pub temporal_ms: f32,
    pub denoise_ms: f32,
    pub composite_ms: f32,
}
