//! Tiers y presets de reflejos.
//!
//! SSR y RTX son sistemas **independientes y mutuamente excluyentes**: el modo decide cuál de los
//! dos se usa (o ninguno). Cada sistema tiene su propia fila de tiers en el editor:
//! - SSR conserva el comportamiento de la cadena SSR/denoise/composite de siempre.
//! - RTX traduce su tier a `RtxPreset`, aplicado en el trazado (HW o BVH), denoise y TAA.

/// Nivel de calidad de un sistema de reflejos.
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
    #[allow(dead_code)]
    pub const ALL: [Self; 5] = [Self::Off, Self::Low, Self::Medium, Self::High, Self::Ultra];

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

    pub const fn is_off(self) -> bool {
        matches!(self, ReflectionTier::Off)
    }
}

/// Sistema de reflejos activo. SSR y RTX nunca están activos a la vez.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReflectionMode {
    #[default]
    Off,
    Ssr,
    Rt,
}

impl ReflectionMode {
    pub const fn wire(self) -> &'static str {
        match self {
            ReflectionMode::Off => "off",
            ReflectionMode::Ssr => "ssr",
            ReflectionMode::Rt => "rt",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "off" => Some(ReflectionMode::Off),
            "ssr" => Some(ReflectionMode::Ssr),
            "rt" => Some(ReflectionMode::Rt),
            _ => None,
        }
    }

    pub const fn is_active(self) -> bool {
        !matches!(self, ReflectionMode::Off)
    }

    pub const fn uses_rt(self) -> bool {
        matches!(self, ReflectionMode::Rt)
    }
}

/// Por defecto los reflejos van **apagados**.
pub const DEFAULT_REFLECTION_TIER: ReflectionTier = ReflectionTier::Off;

/// Parámetros reales detrás de cada tier RTX (tabla del card).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtxPreset {
    /// Fracción del extent del frame usada por el target de reflejos (0.5 → 50%).
    pub resolution_scale: f32,
    /// Rayos emitidos por píxel.
    pub rays_per_pixel: u32,
    /// Rebotes del rayo principal.
    pub bounces: u32,
    /// Pasadas del filtro de denoise.
    pub denoise_passes: u32,
    /// Paso en píxeles de cada pasada de denoise.
    pub denoise_step: f32,
    /// Mezcla temporal del historial de reflejos.
    pub temporal_blend: f32,
    /// Roughness máxima que todavía recibe reflejo RT.
    pub max_roughness: f32,
    /// Distancia máxima de trazado (m).
    pub trace_distance: f32,
    /// Construir la BVH priorizando velocidad de trazado en vez de construcción.
    pub fast_trace: bool,
}

impl RtxPreset {
    /// Preset asociado a un tier. `Off` devuelve el de `Low` para que el valor siempre sea válido.
    pub const fn for_tier(tier: ReflectionTier) -> Self {
        match tier {
            ReflectionTier::Low => Self {
                resolution_scale: 0.50,
                rays_per_pixel: 1,
                bounces: 1,
                denoise_passes: 1,
                denoise_step: 1.0,
                temporal_blend: 0.35,
                max_roughness: 0.25,
                trace_distance: 60.0,
                fast_trace: false,
            },
            ReflectionTier::Medium => Self {
                resolution_scale: 0.67,
                rays_per_pixel: 1,
                bounces: 1,
                denoise_passes: 2,
                denoise_step: 1.25,
                temporal_blend: 0.60,
                max_roughness: 0.40,
                trace_distance: 120.0,
                fast_trace: false,
            },
            ReflectionTier::High => Self {
                resolution_scale: 0.85,
                rays_per_pixel: 2,
                bounces: 2,
                denoise_passes: 3,
                denoise_step: 1.5,
                temporal_blend: 0.60,
                max_roughness: 0.70,
                trace_distance: 250.0,
                fast_trace: true,
            },
            ReflectionTier::Ultra => Self {
                resolution_scale: 1.0,
                rays_per_pixel: 4,
                bounces: 3,
                denoise_passes: 4,
                denoise_step: 2.0,
                temporal_blend: 0.85,
                max_roughness: 1.0,
                trace_distance: 500.0,
                fast_trace: true,
            },
            ReflectionTier::Off => Self::for_tier(ReflectionTier::Low),
        }
    }
}

/// Vista de depuración de reflejos.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReflectionDebugView {
    #[default]
    Final,
    SsrDebug,
    RtDebug,
}

impl ReflectionDebugView {
    pub const fn wire(self) -> &'static str {
        match self {
            ReflectionDebugView::Final => "final",
            ReflectionDebugView::SsrDebug => "ssr_debug",
            ReflectionDebugView::RtDebug => "rt_debug",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "final" => Some(ReflectionDebugView::Final),
            "ssr_debug" => Some(ReflectionDebugView::SsrDebug),
            "rt_debug" => Some(ReflectionDebugView::RtDebug),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtx_tiers_have_the_documented_values() {
        let low = RtxPreset::for_tier(ReflectionTier::Low);
        assert!((low.resolution_scale - 0.5).abs() < 1e-6);
        assert_eq!((low.rays_per_pixel, low.bounces), (1, 1));
        let ultra = RtxPreset::for_tier(ReflectionTier::Ultra);
        assert!((ultra.resolution_scale - 1.0).abs() < 1e-6);
        assert_eq!((ultra.rays_per_pixel, ultra.bounces), (4, 3));
        assert!((ultra.max_roughness - 1.0).abs() < 1e-6);
    }

    #[test]
    fn reflection_modes_are_mutually_exclusive_by_value() {
        assert_eq!(ReflectionMode::from_wire("ssr"), Some(ReflectionMode::Ssr));
        assert_eq!(ReflectionMode::from_wire("rt"), Some(ReflectionMode::Rt));
        assert!(!ReflectionMode::Ssr.uses_rt());
        assert!(ReflectionMode::Rt.uses_rt());
    }
}
