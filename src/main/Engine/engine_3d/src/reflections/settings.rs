//! Nivel global de reflejos (Off / Low / Medium / High) y presets internos.

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
    Normals,
    Roughness,
    SsrHits,
    ReflectionMask,
    RtInstances,
    /// Colorea por índice de ranura de probe resuelto (nearest / own-slot).
    ProbeLayers,
}

impl ReflectionDebugView {
    pub fn from_wire(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "final" | "" => Some(Self::Final),
            "normals" | "normal" => Some(Self::Normals),
            "roughness" | "rough" => Some(Self::Roughness),
            "ssr_hits" | "ssrhits" | "hits" => Some(Self::SsrHits),
            "reflection_mask" | "mask" => Some(Self::ReflectionMask),
            "rt_instances" | "rt_diag" => Some(Self::RtInstances),
            "probe_layers" | "probe_slots" | "probes" => Some(Self::ProbeLayers),
            _ => None,
        }
    }

    pub fn wire(self) -> &'static str {
        match self {
            Self::Final => "final",
            Self::Normals => "normals",
            Self::Roughness => "roughness",
            Self::SsrHits => "ssr_hits",
            Self::ReflectionMask => "reflection_mask",
            Self::RtInstances => "rt_instances",
            Self::ProbeLayers => "probe_layers",
        }
    }

    pub fn is_visual_debug(self) -> bool {
        !matches!(self, Self::Final | Self::RtInstances)
    }

    pub fn shader_index(self) -> u32 {
        match self {
            Self::Final => 0,
            Self::Normals => 1,
            Self::SsrHits => 2,
            Self::ReflectionMask => 3,
            Self::Roughness => 4,
            Self::RtInstances => 3,
            Self::ProbeLayers => 28,
        }
    }
}

/// Preset derivado del tier; no se persiste en `.save`.
#[derive(Clone, Copy, Debug)]
pub struct ReflectionSettings {
    pub tier: ReflectionTier,
    pub max_steps: u32,
    pub max_distance_m: f32,
    pub temporal_blend: f32,
    pub max_roughness_to_trace: f32,
    pub rt_enabled: bool,
    pub rt_static_only: bool,
    /// Peso del color RT sobre SSR donde la máscara SSR es baja (0–1).
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

    /// High/Ultra: sincronizan TLAS/BLAS y ejecutan RT compute.
    pub fn uses_rt_hw(self) -> bool {
        matches!(self, Self::High | Self::Ultra)
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
    pub fn from_tier(tier: ReflectionTier, rt_available: bool) -> Self {
        match tier {
            ReflectionTier::Off => Self {
                tier,
                max_steps: 0,
                max_distance_m: 0.0,
                temporal_blend: 0.0,
                max_roughness_to_trace: 1.0,
                rt_enabled: false,
                rt_static_only: true,
                rt_blend: 0.85,
            },
            ReflectionTier::Low => Self {
                tier,
                max_steps: 16,
                max_distance_m: 8.0,
                temporal_blend: 0.0,
                max_roughness_to_trace: 0.70,
                rt_enabled: false,
                rt_static_only: true,
                rt_blend: 0.85,
            },
            ReflectionTier::Medium => Self {
                tier,
                max_steps: 32,
                max_distance_m: 20.0,
                temporal_blend: 0.35,
                max_roughness_to_trace: 0.70,
                rt_enabled: false,
                rt_static_only: true,
                rt_blend: 0.85,
            },
            ReflectionTier::High => Self {
                tier,
                max_steps: 48,
                max_distance_m: 40.0,
                temporal_blend: 0.42,
                max_roughness_to_trace: 0.70,
                rt_enabled: rt_available,
                rt_static_only: true,
                rt_blend: 0.85,
            },
            ReflectionTier::Ultra => Self {
                tier,
                max_steps: 64,
                max_distance_m: 80.0,
                temporal_blend: 0.45,
                max_roughness_to_trace: 0.80,
                rt_enabled: rt_available,
                rt_static_only: false,
                rt_blend: 0.85,
            },
        }
    }

    pub fn active(self) -> bool {
        self.tier != ReflectionTier::Off
    }
}

/// Preset efectivo: degrada High/Ultra a Medium si la GPU no expone ray query.
pub fn effective_reflection_settings(
    requested: ReflectionTier,
    rt_available: bool,
) -> (ReflectionSettings, bool) {
    if requested.uses_rt_hw() && !rt_available {
        return (
            ReflectionSettings::from_tier(ReflectionTier::Medium, false),
            true,
        );
    }
    (
        ReflectionSettings::from_tier(requested, rt_available),
        false,
    )
}

pub const DEFAULT_REFLECTION_TIER: ReflectionTier = ReflectionTier::Off;

#[derive(Clone, Copy, Debug, Default)]
pub struct ReflectionProfilerMs {
    pub ssr_ms: f32,
    pub temporal_ms: f32,
    pub denoise_ms: f32,
    pub rt_ms: f32,
    pub composite_ms: f32,
}
