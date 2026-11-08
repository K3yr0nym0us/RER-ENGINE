//! Tiers y presets de reflejos (versión mínima sin backend GPU).
//!
//! El módulo `reflections/` (wgpu) fue borrado; este archivo conserva los tipos que el editor
//! y el IPC todavía usan (`ReflectionTier`, `ReflectionDebugView`, `ReflectionSettings`,
//! `DEFAULT_REFLECTION_TIER`). La presición de los valores de `screen_fraction` /
//! `cubemap_face_size` se recupera en Fase 4 sobre `vulkan::FrameRenderer`.

/// Nivel de calidad de reflejos.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReflectionTier {
    #[default]
    Off,
    Low,
    Medium,
    High,
    Ultra,
}

impl ReflectionTier {
    pub const fn wire(self) -> &'static str {
        match self {
            ReflectionTier::Off => "off",
            ReflectionTier::Low => "low",
            ReflectionTier::Medium => "medium",
            ReflectionTier::High => "high",
            ReflectionTier::Ultra => "ultra",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "off" => Some(ReflectionTier::Off),
            "low" => Some(ReflectionTier::Low),
            "medium" => Some(ReflectionTier::Medium),
            "high" => Some(ReflectionTier::High),
            "ultra" => Some(ReflectionTier::Ultra),
            _ => None,
        }
    }
}

/// Por defecto los reflejos van **apagados**: la cadena SSR/denoise/composite y el trazado solo se
/// activan cuando el editor manda un tier distinto de `off`.
pub const DEFAULT_REFLECTION_TIER: ReflectionTier = ReflectionTier::Off;

/// Vista de depuración de reflejos.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReflectionDebugView {
    #[default]
    Final,
    SsrDebug,
}

impl ReflectionDebugView {
    pub const fn wire(self) -> &'static str {
        match self {
            ReflectionDebugView::Final => "final",
            ReflectionDebugView::SsrDebug => "ssr_debug",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "final" => Some(ReflectionDebugView::Final),
            "ssr_debug" => Some(ReflectionDebugView::SsrDebug),
            _ => None,
        }
    }
}
